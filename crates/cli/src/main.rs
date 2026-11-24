//! La CLI `cockpit` : la base de l'app lue en quelques lignes, pour un agent qui compte ses tokens.
//! Lecture seule, sur la même base que l'app (`noyau::chemins::base()`), qui peut tourner en même temps (WAL).
//!
//! Installer, sur Mac comme sur Windows : `cargo install --path crates/cli`, puis `cockpit` est dans
//! `~/.cargo/bin` (`%USERPROFILE%\.cargo\bin` sous Windows). La sortie est en UTF-8 sans couleur ANSI ; la console
//! Windows l'affiche telle quelle.
use noyau::db::{Database, Valeur};
use noyau::jour::{self, Date};
use noyau::lecture::{format, Lectures};
use noyau::store::Store;
use noyau::{p, Erreur, Res};

const USAGE: &str = "\
cockpit etat                                 sources, derniers événements
cockpit videos [n]                           nos vidéos, plus récentes d'abord
cockpit sql \"<select>\" [--tout] [--csv]      lecture seule ; 50 lignes coupées à 60 caractères, sauf --tout ;
                                             --csv sort tout en CSV";

fn erreur(m: &str) -> Erreur { Erreur::Commande(m.into()) }

struct Args(Vec<String>);

impl Args {
    fn drapeau(&mut self, nom: &str) -> bool {
        let Some(i) = self.0.iter().position(|a| a == nom) else { return false };
        self.0.remove(i);
        true
    }

    fn premier(&self) -> Option<&str> { self.0.first().map(String::as_str) }
    fn entier(&self) -> Option<i64> { self.premier().and_then(|a| a.parse().ok()) }
}

fn n(v: Option<i64>) -> String { v.map(|v| format::compact(v as f64)).unwrap_or_else(|| "?".into()) }

/// `40 min`, `5 h`, `3 j`.
fn age(d: Option<Date>) -> String {
    let Some(d) = d else { return "?".into() };
    let s = (jour::maintenant() - d).num_milliseconds() as f64 / 1000.0;
    if s < 3_600.0 { return format!("{} min", (s / 60.0).max(0.0) as i64); }
    if s < 172_800.0 { return format!("{} h", (s / 3_600.0) as i64); }
    format!("{} j", (s / 86_400.0) as i64)
}

fn ligne(s: Option<&str>, max: usize) -> String {
    let t = s.unwrap_or("").replace('\n', " ");
    let t = t.trim();
    if t.chars().count() > max { format!("{}…", t.chars().take(max - 1).collect::<String>()) } else { t.into() }
}

fn etat(store: &Store, l: &Lectures) -> Res<()> {
    let collectes = store.db.query("
        select c.source, c.etat, c.fin, c.erreur from collecte c
        where c.debut = (select max(debut) from collecte where source = c.source) order by c.source", p![])?;
    println!("sources");
    for c in collectes {
        let erreur = c.texte("erreur").map(|e| format!(" : {}", ligne(Some(&e), 80))).unwrap_or_default();
        println!("  {} {} il y a {}{erreur}", c.texte("source").unwrap_or_default(), c.texte("etat").unwrap_or_default(),
                 age(c.date("fin")));
    }
    println!("fil");
    for e in l.fil(None, None, 8)? { println!("  {} {} {}", age(Some(e.at)), e.type_.brut(), ligne(Some(&e.texte), 80)); }
    Ok(())
}

fn videos(l: &Lectures, args: &Args) -> Res<()> {
    let limite = args.entier().unwrap_or(10).max(0) as usize;
    let mut vs = l.videos(1000)?;
    let passe = jour::loin_passe();
    vs.sort_by(|a, b| b.publiee_le.unwrap_or(passe).cmp(&a.publiee_le.unwrap_or(passe)));
    for v in vs.iter().take(limite) {
        let gain = v.gain24h.map(|g| format!(" +{}/24h", format::compact(g as f64))).unwrap_or_default();
        let partages = v.part_partages().map(|p| format!(" part. {}", format::pourcent(p, 1))).unwrap_or_default();
        let enreg = v.part_enreg().map(|p| format!(" enr. {}", format::pourcent(p, 1))).unwrap_or_default();
        println!("@{} {} {} vues{gain}{partages}{enreg}{} · {}", v.compte, age(v.publiee_le), n(Some(v.vues)),
                 if v.seuil_atteint() { " ✓" } else { "" }, ligne(Some(&v.titre), 50));
    }
    Ok(())
}

/// Une valeur SQLite écrite comme Swift l'interpole : `12`, `0.5`, `3.0`, `16 bytes`.
fn cellule(v: &Valeur) -> String {
    match v {
        Valeur::Null => String::new(),
        Valeur::Int(i) => i.to_string(),
        Valeur::Reel(r) if r.is_finite() && r.fract() == 0.0 && r.abs() < 1e16 => format!("{r:.1}"),
        Valeur::Reel(r) => r.to_string(),
        Valeur::Texte(t) => t.clone(),
        Valeur::Blob(b) => format!("{} bytes", b.len()),
    }
}

fn sql(args: &mut Args) -> Res<()> {
    let csv = args.drapeau("--csv");
    let tout = csv || args.drapeau("--tout");
    let requete = args.0.join(" ");
    let requete = requete.trim();
    let debut = requete.to_lowercase();
    let sans_dernier: String = requete.chars().take(requete.chars().count().saturating_sub(1)).collect();
    if !(debut.starts_with("select") || debut.starts_with("with")) || sans_dernier.contains(';') {
        return Err(erreur("un seul select ou with"));
    }
    let chemin = noyau::chemins::base();
    let chemin = chemin.to_str().ok_or_else(|| erreur("chemin de la base illisible"))?;
    let (colonnes, lignes) = Database::ouvrir(chemin, true)?.tableau(requete)?;
    if csv {
        println!("{}", colonnes.iter().map(|c| champ_csv(c)).collect::<Vec<_>>().join(","));
        for l in &lignes { println!("{}", l.iter().map(|v| champ_csv(&cellule(v))).collect::<Vec<_>>().join(",")); }
        return Ok(());
    }
    if lignes.is_empty() { println!("0 ligne"); return Ok(()) }
    println!("{}", colonnes.join(" | "));
    let max = if tout { lignes.len() } else { 50 };
    for l in lignes.iter().take(max) {
        println!("{}", l.iter()
            .map(|v| if tout { cellule(v).replace('\n', " ") } else { ligne(Some(&cellule(v)), 60) })
            .collect::<Vec<_>>().join(" | "));
    }
    if lignes.len() > max { println!("… {} de plus, --tout pour tout voir", lignes.len() - max); }
    Ok(())
}

/// Un champ CSV, entre guillemets s'il le faut.
fn champ_csv(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.into() }
}

/// Lance la commande ; `Ok(false)` pour une commande inconnue.
fn lancer(commande: &str, args: &mut Args) -> Res<bool> {
    let store = Store::ouvrir()?;
    let l = Lectures::new(&store.db, store.businesses()?);
    match commande {
        "etat" => etat(&store, &l)?,
        "videos" => videos(&l, args)?,
        "sql" => sql(args)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn main() {
    let mut args = Args(std::env::args().skip(1).collect());
    if args.0.is_empty() {
        println!("{USAGE}");
        return;
    }
    let commande = args.0.remove(0);
    match lancer(&commande, &mut args) {
        Ok(true) => {}
        Ok(false) => {
            println!("{USAGE}");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("cockpit : {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn drapeaux_retires() {
        let mut a = Args(["--tout", "12"].map(String::from).to_vec());
        assert!(a.drapeau("--tout"));
        assert!(!a.drapeau("--tout"));
        assert_eq!(a.0, ["12"]);
        assert_eq!(a.entier(), Some(12));
    }

    #[test]
    fn ligne_coupee() {
        assert_eq!(ligne(Some(" a\nb "), 70), "a b");
        assert_eq!(ligne(Some("abcdef"), 4), "abc…");
        assert_eq!(ligne(None, 4), "");
    }

    #[test]
    fn champs_csv() {
        assert_eq!(champ_csv("a"), "a");
        assert_eq!(champ_csv("a,b"), "\"a,b\"");
        assert_eq!(champ_csv("dit \"oui\""), "\"dit \"\"oui\"\"\"");
    }

    #[test]
    fn cellules_comme_swift() {
        assert_eq!(cellule(&Valeur::Reel(3.0)), "3.0");
        assert_eq!(cellule(&Valeur::Reel(0.5)), "0.5");
        assert_eq!(cellule(&Valeur::Blob(vec![0; 16])), "16 bytes");
    }

    #[test]
    fn ages_courts() {
        let maintenant = jour::maintenant();
        assert_eq!(age(None), "?");
        assert_eq!(age(Some(maintenant - Duration::minutes(40))), "40 min");
        assert_eq!(age(Some(maintenant - Duration::hours(5))), "5 h");
        assert_eq!(age(Some(maintenant - Duration::days(3))), "3 j");
    }
}
