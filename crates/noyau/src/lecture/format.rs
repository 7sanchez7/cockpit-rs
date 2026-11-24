//! Nombres, argent, dates et durées tels que l'UI les écrit, au format français (`48 200`, `7,10 €`, `−4 %`).
//! Les jours s'écrivent à l'heure de Paris, les milliers sont séparés par une espace fine insécable (U+202F).
use crate::jour::{self, Date};
use chrono::{Datelike, Timelike};
use chrono_tz::Europe::Paris;

/// Ce qu'on affiche tant qu'une source n'a rien écrit.
pub const VIDE: &str = "…";
/// La cadence d'une source sans cadence, qui ne passe que sur demande.
pub const A_LA_MAIN: &str = "à la main";
/// Le vrai signe moins, pas le trait d'union.
pub const MOINS: &str = "\u{2212}";
const MILLIERS: char = '\u{202F}';

const MOIS_COURTS: [&str; 12] =
    ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];
const MOIS: [&str; 12] = [
    "janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre",
    "décembre",
];
const JOURS: [&str; 7] = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];

pub fn entier(v: f64) -> String { decimal(v.round(), 0) }

/// `v` à `decimales` chiffres après la virgule, arrondi au pair le plus proche comme ICU.
pub fn decimal(v: f64, decimales: usize) -> String {
    if !v.is_finite() { return v.to_string(); }
    let brut = format!("{}", v.abs());
    let (ent, frac) = brut.split_once('.').unwrap_or((&brut, ""));
    let mut chiffres: Vec<u8> = ent.bytes().chain(frac.bytes()).map(|c| c - b'0').collect();
    let garde = ent.len() + decimales;
    chiffres.resize(chiffres.len().max(garde), 0);
    let reste = &chiffres[garde..];
    let haut = match reste.first() {
        None => false,
        Some(&c) if c > 5 => true,
        Some(&5) => reste[1..].iter().any(|&c| c > 0) || (garde > 0 && chiffres[garde - 1] % 2 == 1),
        Some(_) => false,
    };
    chiffres.truncate(garde);
    let mut longueur_entiere = ent.len();
    if haut {
        let mut i = chiffres.len();
        loop {
            if i == 0 {
                chiffres.insert(0, 1);
                longueur_entiere += 1;
                break;
            }
            i -= 1;
            if chiffres[i] == 9 { chiffres[i] = 0 } else { chiffres[i] += 1; break; }
        }
    }
    let texte = |c: &[u8]| c.iter().map(|d| char::from(b'0' + d)).collect::<String>();
    let partie_entiere = texte(&chiffres[..longueur_entiere]);
    let mut s = String::new();
    if v.is_sign_negative() { s.push('-'); }
    for (i, c) in partie_entiere.chars().enumerate() {
        if i > 0 && (partie_entiere.len() - i) % 3 == 0 { s.push(MILLIERS); }
        s.push(c);
    }
    if decimales > 0 {
        s.push(',');
        s.push_str(&texte(&chiffres[longueur_entiere..]));
    }
    s
}

/// Une décimale au plus : `25,4`, `48`.
fn au_plus_une(v: f64) -> String {
    let s = decimal(v, 1);
    s.strip_suffix(",0").map(str::to_string).unwrap_or(s)
}

/// `6 420`, `25,4 k`, `48 k`, `337 k`, `1,2 M` : une décimale sous 100 k, pour qu'un axe serré ne répète pas `26 k`.
pub fn compact(v: f64) -> String {
    let a = v.abs();
    let s = if v < 0.0 { MOINS } else { "" };
    if a < 10_000.0 { return format!("{s}{}", entier(a)); }
    if a < 100_000.0 { return format!("{s}{} k", au_plus_une(a / 1000.0)); }
    if a < 1_000_000.0 { return format!("{s}{} k", entier(a / 1000.0)); }
    format!("{s}{} M", au_plus_une(a / 1_000_000.0))
}

/// `+412`, `−96`, `+0`.
pub fn delta(v: f64, compacte: bool) -> String {
    let corps = if compacte { compact(v.abs()) } else { entier(v.abs()) };
    format!("{}{corps}", if v < 0.0 { MOINS } else { "+" })
}

/// Un écart en unités, `= 0` s'il est nul.
pub fn unites(ecart: Option<f64>) -> Option<String> {
    ecart.map(|e| if e == 0.0 { "= 0".to_string() } else { delta(e, false) })
}

/// `3 essais`, `1 achat`, `0 rond` : le pluriel commence à 2.
pub fn nombre(v: Option<f64>, mot: &str) -> Option<String> {
    v.map(|v| format!("{} {mot}{}", entier(v), if v.abs() < 2.0 { "" } else { "s" }))
}

/// `0,99 %` pour 0,0099.
pub fn pourcent(ratio: f64, decimales: usize) -> String { format!("{} %", decimal(ratio * 100.0, decimales)) }

/// Variation relative : `+9 %`, `−4 %` ; None si la référence est nulle.
pub fn variation(actuel: f64, avant: f64) -> Option<String> {
    if avant == 0.0 { return None; }
    let r = ((actuel - avant) / avant * 100.0).round();
    Some(format!("{}{} %", if r < 0.0 { MOINS } else { "+" }, entier(r.abs())))
}

/// Variation relative entre deux relevés facultatifs : `= 0` si les deux sont nuls, None s'il en manque un.
pub fn evolution(actuel: Option<f64>, avant: Option<f64>) -> Option<String> {
    let (actuel, avant) = (actuel?, avant?);
    variation(actuel, avant).or_else(|| (actuel == 0.0).then(|| "= 0".to_string()))
}

/// Écart brut entre deux relevés facultatifs ; None s'il en manque un.
pub fn ecart(actuel: Option<f64>, avant: Option<f64>) -> Option<f64> { Some(actuel? - avant?) }

/// Un écart de taux en points : `+1,2 pt` ; None s'il manque un taux.
pub fn points(actuel: Option<f64>, avant: Option<f64>) -> Option<String> {
    ecart(actuel, avant).map(|e| {
        let p = (e * 1000.0).round() / 10.0;
        if p == 0.0 { "= 0 pt".to_string() } else { format!("{}{} pt", if p < 0.0 { MOINS } else { "+" }, decimal(p.abs(), 1)) }
    })
}

/// Un montant signé : `+97 €`, `−12 €`.
pub fn signe(v: f64, format: impl Fn(f64) -> String) -> String {
    format!("{}{}", if v < 0.0 { MOINS } else { "+" }, format(v.abs()))
}

/// `7,10 €`, `1 240 €`.
pub fn euros(v: f64) -> String {
    format!("{} €", if v.abs() >= 100.0 { entier(v) } else { decimal(v, 2) })
}

/// `dont 4,10 € de voix`.
pub fn dont_voix(v: f64) -> String { format!("dont {} de voix", euros(v)) }

/// `152e`, `1er`.
pub fn rang(p: i64) -> String { if p == 1 { "1er".to_string() } else { format!("{p}e") } }

// dates

fn paris(d: &Date) -> chrono::DateTime<chrono_tz::Tz> { d.with_timezone(&Paris) }

/// `14:52`
pub fn heure(d: &Date) -> String { paris(d).format("%H:%M").to_string() }

/// `17 nov.`
pub fn jour_court(d: &Date) -> String {
    let p = paris(d);
    format!("{} {}", p.day(), MOIS_COURTS[p.month0() as usize])
}

/// `mars 2027`.
pub fn mois_annee(d: &Date) -> String {
    let p = paris(d);
    format!("{} {}", MOIS[p.month0() as usize], p.year())
}

/// `17 nov.` depuis une clé `yyyy-MM-dd`.
pub fn jour_court_cle(cle: &str) -> String { jour::date(cle).map(|d| jour_court(&d)).unwrap_or_else(|| cle.to_string()) }

/// `12 nov., 14:52`
pub fn jour_heure(d: &Date) -> String { format!("{}, {:02}:{:02}", jour_court(d), paris(d).hour(), paris(d).minute()) }

/// `jeudi`
pub fn jour_semaine(d: &Date) -> String { JOURS[paris(d).weekday().num_days_from_monday() as usize].to_string() }

/// `Jeudi 12 novembre`
pub fn date_longue(d: &Date) -> String {
    let p = paris(d);
    majuscule(&format!("{} {} {}", jour_semaine(d), p.day(), MOIS[p.month0() as usize]))
}

/// La première lettre en capitale.
pub fn majuscule(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|p| p.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// `à l'instant`, `il y a 3 min`, `il y a 2 h`, `il y a 4 j`.
pub fn il_ya(d: &Date, maintenant: &Date) -> String {
    let s = ((*maintenant - *d).num_milliseconds() as f64 / 1000.0).max(0.0);
    if s < 60.0 { return "à l'instant".to_string(); }
    if s < 3600.0 { return format!("il y a {} min", (s / 60.0) as i64); }
    if s < 86_400.0 { return format!("il y a {} h", (s / 3600.0) as i64); }
    format!("il y a {} j", (s / 86_400.0) as i64)
}

/// Âge d'une vidéo en jours calendaires de Paris : `J+0` à `J+6`, puis `19 j`.
pub fn age(d: &Date, maintenant: &Date) -> String {
    let n = (paris(maintenant).date_naive() - paris(d).date_naive()).num_days().max(0);
    if n < 7 { format!("J+{n}") } else { format!("{n} j") }
}

/// Cadence d'une source : `15 min`, `1 h`, `3 h`.
pub fn cadence(s: f64) -> String {
    if s < 3600.0 { format!("{} min", entier(s / 60.0)) } else { format!("{} h", entier(s / 3600.0)) }
}

/// Une durée courte : `0,8 s`, `12 s`.
pub fn duree(s: f64) -> String { if s < 10.0 { format!("{} s", decimal(s, 1)) } else { format!("{} s", entier(s)) } }

/// Un temps cumulé : `45 s`, `12 min`, `3,4 h`, `164 h`.
pub fn cumul(s: f64) -> String {
    if s < 60.0 { duree(s) }
    else if s < 3600.0 { format!("{} min", entier(s / 60.0)) }
    else if s < 36_000.0 { format!("{} h", decimal(s / 3600.0, 1)) }
    else { format!("{} h", entier(s / 3600.0)) }
}

/// Durée d'un passage, à la seconde comme les horodatages de `collecte` : `< 1 s`, `12 s`.
pub fn passage(s: f64) -> String { if s < 1.0 { "< 1 s".to_string() } else { format!("{} s", entier(s)) } }
