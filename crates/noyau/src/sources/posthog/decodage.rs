//! Le décodage des réponses HogQL et leur écriture dans la base.
use super::{nom, Lignes, RONDS_DEPUIS, SEGMENTS};
use crate::dimension::Dimension;
use crate::erreur::{format_err, Res};
use crate::jour::{self, Date};
use crate::metrique::{self, Boutique};
use crate::modeles::{Evenement, Install, Porte, TypeFil};
use crate::sources::pays_fr;
use crate::store::Store;
use chrono::DateTime;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};

/// Ce qu'un passage a lu de PostHog pour un business, prêt à écrire.
#[derive(Debug, Default)]
pub struct LecturePostHog {
    /// Le total (`None`) puis chaque store, d'après le `$os` des events.
    pub series: HashMap<Option<Boutique>, Series>,
    /// Par segment, par jour puis par heure.
    pub actifs_heure: Horaires,
    pub fil: Vec<Evenement>,
    /// Nul quand le passage ne l'a pas recalculé (une fois par heure).
    pub horaire: Option<Horaire>,
}

/// Les installs et les portes des jours à partir de `debut`.
#[derive(Debug)]
pub struct Horaire {
    pub debut: String,
    pub installs: Vec<Install>,
    pub portes: Vec<Porte>,
}

/// Par jour, le compte entier et le compte avant la coupe (l'heure qu'il est, 7 jours plus tôt).
#[derive(Debug, Default, Clone)]
pub struct Series {
    pub actifs: HashMap<String, Compte>,
    pub essais: HashMap<String, Compte>,
    pub ronds: HashMap<String, Compte>,
    pub installs: HashMap<String, Compte>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Compte {
    pub tout: i64,
    pub coupe: i64,
}

/// Les portes du paywall hors tunnel (`PaywallSource` de l'app Roleo).
pub const NOMS_PORTES: [(&str, &str); 14] = [
    ("round_locked", "rond verrouillé"), ("app_open", "retour dans l'app"), ("tutor_call", "appel au tuteur"),
    ("navigation_wall", "mur de l'app"), ("account", "onglet Compte"), ("mic", "micro"), ("listen", "écoute"),
    ("situations", "situations"), ("debates", "débats"), ("home_rounds", "ronds de l'accueil"), ("quota", "quota"),
    ("immersion", "immersion"), ("server_refusal", "refus du serveur"), ("round_done", "rond fini"),
];

// décodage

/// Les lignes d'une réponse HogQL, colonnes dans l'ordre du select.
pub fn lignes(reponse: &Map<String, Value>) -> Res<Lignes> {
    let lignes = reponse
        .get("results")
        .and_then(Value::as_array)
        .and_then(|r| r.iter().map(|l| l.as_array().cloned()).collect::<Option<Lignes>>());
    let Some(lignes) = lignes else {
        let mut cles: Vec<&str> = reponse.keys().map(String::as_str).collect();
        cles.sort();
        return Err(format_err(format!("HogQL sans results ({})", cles.join(", "))));
    };
    // jamais de chiffre tronqué en silence
    if reponse.get("hasMore").and_then(Value::as_bool) == Some(true) {
        return Err(format_err("HogQL coupé par sa limit"));
    }
    Ok(lignes)
}

/// Lignes `jour, n1, n2…` en `jour → [n1, n2…]`.
pub fn par_jour(lignes: &Lignes, colonnes: usize) -> Res<HashMap<String, Vec<i64>>> {
    let mut r = HashMap::new();
    for l in lignes {
        forme(l, colonnes + 1)?;
        r.insert(texte(&l[0])?, l[1..].iter().map(entier).collect::<Res<Vec<_>>>()?);
    }
    Ok(r)
}

/// Les lignes de `requete_jours` et `requete_installs` en séries, une par segment.
pub fn series(jours: &Lignes, installs: &Lignes) -> Res<HashMap<Option<Boutique>, Series>> {
    let j = par_jour(jours, 6 * SEGMENTS.len())?;
    let i = par_jour(installs, 2 * SEGMENTS.len())?;
    let comptes = |d: &HashMap<String, Vec<i64>>, c: usize| -> HashMap<String, Compte> {
        d.iter().map(|(k, v)| (k.clone(), Compte { tout: v[c], coupe: v[c + 1] })).collect()
    };
    Ok(SEGMENTS
        .iter()
        .enumerate()
        .map(|(k, b)| {
            let s = Series {
                actifs: comptes(&j, 6 * k),
                essais: comptes(&j, 6 * k + 2),
                ronds: comptes(&j, 6 * k + 4),
                installs: comptes(&i, 2 * k),
            };
            (*b, s)
        })
        .collect())
}

/// Segment → jour → heure → n.
pub type Horaires = HashMap<Option<Boutique>, HashMap<String, HashMap<i64, i64>>>;

/// Lignes `jour, heure`, puis un compte par segment, en `segment → jour → heure → n`.
pub fn par_heure(lignes: &Lignes) -> Res<Horaires> {
    let mut r = Horaires::new();
    for l in lignes {
        forme(l, 2 + SEGMENTS.len())?;
        let (j, heure) = (texte(&l[0])?, entier(&l[1])?);
        for (k, b) in SEGMENTS.iter().enumerate() {
            r.entry(*b).or_default().entry(j.clone()).or_default().insert(heure, entier(&l[2 + k])?);
        }
    }
    Ok(r)
}

/// Lignes `uuid, instant, produit, porte, pays` en lignes du fil.
pub fn fil(lignes: &Lignes, business: &str) -> Res<Vec<Evenement>> {
    lignes
        .iter()
        .map(|l| {
            forme(l, 5)?;
            let (uuid, instant) = (texte(&l[0])?, entier(&l[1])?);
            let at: Date = DateTime::from_timestamp(instant, 0).ok_or_else(|| format_err(format!("HogQL, instant illisible : {instant}")))?;
            Ok(Evenement {
                id: format!("posthog:essai:{uuid}"),
                business: business.into(),
                at,
                type_: TypeFil::Essai,
                texte: texte_essai(l[2].as_str(), l[3].as_str(), l[4].as_str()),
                lien: None,
            })
        })
        .collect()
}

/// `Essai annuel, Espagne`, plus la porte du paywall quand l'essai ne vient pas du tunnel.
pub fn texte_essai(produit: Option<&str>, porte: Option<&str>, pays: Option<&str>) -> String {
    let p = produit.unwrap_or("").to_lowercase();
    let offre = if p.contains("year") || p.contains("annual") {
        "Essai annuel"
    } else if p.contains("quarter") {
        "Essai trimestriel"
    } else if p.contains("month") {
        "Essai mensuel"
    } else if p.contains("week") {
        "Essai hebdo"
    } else {
        "Essai"
    };
    let mut morceaux = vec![offre];
    if let Some(n) = pays.and_then(pays_fr::nom) {
        morceaux.push(n);
    }
    if let Some(porte) = porte.filter(|p| *p != "onboarding") {
        morceaux.push(NOMS_PORTES.iter().find(|(c, _)| *c == porte).map_or(porte, |(_, n)| n));
    }
    morceaux.join(", ")
}

/// Lignes `jour, loin, paywall, essai, direct, rond, j1, j7, j14, j30, client`, puis une valeur par `Dimension::lues`
/// (plusieurs séparées par des virgules, texte vide sans réponse) ; la zone se déduit du pays.
pub fn installs(lignes: &Lignes) -> Res<Vec<Install>> {
    lignes
        .iter()
        .map(|l| {
            forme(l, 11 + Dimension::lues().count())?;
            let n = l[1..=9].iter().map(entier).collect::<Res<Vec<_>>>()?;
            let mut valeurs: BTreeMap<Dimension, Vec<String>> = BTreeMap::new();
            for (d, v) in Dimension::lues().zip(&l[11..]) {
                let liste: Vec<String> =
                    texte(v)?.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect();
                if !liste.is_empty() {
                    valeurs.insert(d, liste);
                }
            }
            if let Some(z) = valeurs.get(&Dimension::Pays).and_then(|p| p.first()).and_then(|p| Dimension::zone(p)) {
                valeurs.insert(Dimension::Zone, vec![z.to_string()]);
            }
            let client = texte(&l[10])?;
            Ok(Install {
                jour: texte(&l[0])?,
                loin: n[0],
                paywall: n[1] != 0,
                essai: n[2] != 0,
                direct: n[3] != 0,
                rond: n[4] != 0,
                j1: n[5] != 0,
                j7: n[6] != 0,
                j14: n[7] != 0,
                j30: n[8] != 0,
                client: (!client.is_empty()).then_some(client),
                valeurs,
            })
        })
        .collect()
}

/// Lignes `jour, porte, vues, essais, directs`.
pub fn portes(lignes: &Lignes) -> Res<Vec<Porte>> {
    lignes
        .iter()
        .map(|l| {
            forme(l, 5)?;
            Ok(Porte { jour: texte(&l[0])?, porte: texte(&l[1])?, vues: entier(&l[2])?, essais: entier(&l[3])?, directs: entier(&l[4])? })
        })
        .collect()
}

fn forme(l: &[Value], colonnes: usize) -> Res<()> {
    if l.len() != colonnes {
        return Err(format_err(format!("HogQL, {} colonnes au lieu de {colonnes}", l.len())));
    }
    Ok(())
}

fn texte(v: &Value) -> Res<String> {
    v.as_str().map(String::from).ok_or_else(|| format_err(format!("HogQL, texte attendu : {v}")))
}

/// Un nombre JSON (tronqué s'il a des décimales) ou un booléen, comme `NSNumber`.
fn entier(v: &Value) -> Res<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|x| x as i64)),
        Value::Bool(b) => Some(i64::from(*b)),
        _ => None,
    }
    .ok_or_else(|| format_err(format!("HogQL, nombre attendu : {v}")))
}

// écriture

/// Écrit une lecture et rend le nombre de lignes écrites : les séries des `jours` avec leur `_j7`, pour chaque segment
/// (0 pour un jour absent ; les ronds seulement depuis `RONDS_DEPUIS`), les actifs par heure des `jours_heures`
/// (jusqu'à l'heure en cours aujourd'hui), le fil, et les installs et les portes s'ils ont été lus.
pub fn ecrire(l: &LecturePostHog, business: &str, jours: &[String], jours_heures: &[String], maintenant: &Date, store: &Store) -> Res<usize> {
    let mut lignes = 0;
    let mut serie = |metrique: &str, j: &str, heure: i64, v: i64| -> Res<()> {
        store.serie(business, metrique, j, heure, v as f64)?;
        lignes += 1;
        Ok(())
    };
    store.lot(|| {
        let vide = Series::default();
        for b in SEGMENTS {
            let s = l.series.get(&b).unwrap_or(&vide);
            let lu = |d: &HashMap<String, Compte>, j: &str| d.get(j).copied().unwrap_or_default();
            for j in jours {
                let avant = jour::decale(j, -7);
                serie(&nom(b, metrique::ACTIFS), j, -1, lu(&s.actifs, j).tout)?;
                serie(&nom(b, metrique::ACTIFS_J7), j, -1, lu(&s.actifs, &avant).coupe)?;
                serie(&nom(b, metrique::INSTALLS), j, -1, lu(&s.installs, j).tout)?;
                serie(&nom(b, metrique::INSTALLS_J7), j, -1, lu(&s.installs, &avant).coupe)?;
                serie(&nom(b, metrique::ESSAIS), j, -1, lu(&s.essais, j).tout)?;
                serie(&nom(b, metrique::ESSAIS_J7), j, -1, lu(&s.essais, &avant).coupe)?;
                if j.as_str() >= RONDS_DEPUIS {
                    serie(&nom(b, metrique::RONDS), j, -1, lu(&s.ronds, j).tout)?;
                }
                if avant.as_str() >= RONDS_DEPUIS {
                    serie(&nom(b, metrique::RONDS_J7), j, -1, lu(&s.ronds, &avant).coupe)?;
                }
            }
        }
        let aujourdhui = jour::cle(maintenant);
        for b in SEGMENTS {
            for j in jours_heures {
                let fin = if *j == aujourdhui { jour::heure(maintenant) } else { 23 };
                for h in 0..=fin {
                    let v = l.actifs_heure.get(&b).and_then(|x| x.get(j)).and_then(|x| x.get(&h)).copied().unwrap_or(0);
                    serie(&nom(b, metrique::ACTIFS_HEURE), j, h, v)?;
                }
            }
        }
        Ok(())
    })?;
    store.lot(|| {
        for e in &l.fil {
            if store.fil(e)? {
                lignes += 1;
            }
        }
        if let Some(h) = &l.horaire {
            store.installs(business, &h.debut, &h.installs)?;
            store.portes(business, &h.debut, &h.portes)?;
            lignes += h.installs.len() + h.portes.len();
        }
        Ok(lignes)
    })
}
