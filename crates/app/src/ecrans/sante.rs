//! Les commandes de l'écran Santé : les sources répondent-elles, l'IA tient-elle (reprises, 429, latence), combien
//! coûte la journée. Les couleurs partent en nom de variable de theme.css (`succes`), l'interface écrit `var(--succes)`.
use crate::commandes::{texte, Rep};
use crate::Etat;
use noyau::collecte::collecteur::REGLAGE_PAUSE;
use noyau::jour;
use noyau::lecture::format as F;
use noyau::lecture::Lectures;
use noyau::metrique as M;
use noyau::modeles::{EtatPassage, Passage};
use noyau::sources::firebase;
use serde::Serialize;
use tauri::State;

/// Une couleur par fournisseur, stable sur 14 jours ; ni vert ni rouge, réservés aux états.
const PALETTE: [&str; 5] = ["orange", "bleu", "aqua", "violet", "blanc-serie"];
const JOURS_COUT: i64 = 14;
const PASSAGES_JOURNAL: i64 = 30;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sante {
    sous_titre: String,
    kpis: Vec<Kpi>,
    pause: bool,
    sources: Vec<LigneSource>,
    ia: Vec<LigneIA>,
    cout: Cout,
    journal: Vec<LigneJournal>,
    /// Le menu du journal : chaque source, dans l'ordre du collecteur.
    choix: Vec<Choix>,
}

/// Une carte KPI ; `format` dit comment l'interface écrit la valeur (`entier`, `euros`).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Kpi {
    titre: &'static str,
    valeur: Option<f64>,
    format: &'static str,
    delta: Option<String>,
    ton: &'static str,
    serie: Option<Vec<Option<f64>>>,
}

#[derive(Serialize)]
pub struct EtatSource {
    texte: &'static str,
    pub couleur: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LigneSource {
    id: &'static str,
    nom: &'static str,
    etat: EtatSource,
    en_cours: bool,
    cadence: String,
    dernier: String,
    dernier_aide: Option<String>,
    duree: String,
    lignes: String,
    succes: String,
    /// L'erreur d'un passage raté, sur une ligne, et en entier pour la bulle.
    erreur: Option<Erreur>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Erreur {
    ligne: String,
    entiere: String,
    couleur: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LigneIA {
    nom: String,
    couleur: &'static str,
    requetes: String,
    part: String,
    q429: Taux,
    erreurs: Taux,
    latence: String,
    /// Les requêtes de chaque heure, pour les barres empilées.
    heures: Vec<f64>,
}

/// Une part d'échecs, en alerte dès le premier.
#[derive(Serialize)]
pub struct Taux {
    texte: String,
    aide: String,
    alerte: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cout {
    mois: Option<String>,
    jours: Vec<JourCout>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JourCout {
    /// L'étiquette de l'axe, un jour sur quatre en partant d'aujourd'hui.
    etiquette: Option<String>,
    valeur: Option<f64>,
    /// « 18 nov. : 4,20 €, dont 1,10 € de voix ».
    texte: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LigneJournal {
    debut: String,
    source: String,
    couleur: &'static str,
    duree: String,
    lignes: String,
    erreur: Option<Erreur>,
}

#[derive(Serialize)]
pub struct Choix {
    id: &'static str,
    nom: &'static str,
}

/// L'écran Santé ; `hier` pour le panneau IA, `source` filtre le journal.
#[tauri::command]
pub async fn sante(etat: State<'_, Etat>, business: Option<String>, hier: bool, source: Option<String>) -> Rep<Sante> {
    let l = crate::ecrans::lectures(&etat.store, business.as_deref())?;
    let maintenant = l.maintenant;
    let jour = jour::cle(&maintenant);
    let en_cours = etat.collecteur.en_cours();
    let pause = etat.store.reglage(REGLAGE_PAUSE).map_err(texte)?.as_deref() == Some("1");
    let passages = l.derniers_passages().map_err(texte)?;
    let succes = l.derniers_succes().map_err(texte)?;
    let sources = &etat.collecteur.sources;

    let pannes: Vec<&str> = sources.iter()
        .filter(|s| passages.get(s.id()).is_some_and(|p| p.etat == EtatPassage::Erreur)).map(|s| s.nom()).collect();
    let ok = sources.iter().filter(|s| passages.get(s.id()).is_some_and(|p| p.etat == EtatPassage::Ok)).count();
    let mut sous_titre = format!("{} sources, {}", sources.len(),
                                 if pannes.is_empty() { "aucune en panne".to_string() } else { format!("{} en panne", pannes.len()) });
    if let Some(fin) = passages.values().map(|p| p.fin).max() {
        sous_titre += &format!(", dernier passage {}", F::il_ya(&fin, &maintenant));
    }
    if pause { sous_titre += ", collecte en pause"; }

    let lignes = sources.iter().map(|s| {
        let p = passages.get(s.id());
        LigneSource {
            id: s.id(),
            nom: s.nom(),
            etat: etat_source(p, en_cours.contains(s.id())),
            en_cours: en_cours.contains(s.id()),
            cadence: s.cadence().map_or(F::A_LA_MAIN.into(), |c| F::cadence(c as f64)),
            dernier: p.map(|p| F::il_ya(&p.debut, &maintenant)).unwrap_or_else(|| F::VIDE.into()),
            dernier_aide: p.map(|p| F::jour_heure(&p.debut)),
            duree: p.map(duree).unwrap_or_else(|| F::VIDE.into()),
            lignes: p.map(|p| F::entier(p.lignes as f64)).unwrap_or_else(|| F::VIDE.into()),
            succes: succes.get(s.id()).map(|d| F::il_ya(d, &maintenant)).unwrap_or_else(|| "jamais".into()),
            erreur: p.filter(|p| p.etat != EtatPassage::Ok).and_then(erreur),
        }
    }).collect();

    let noms: Vec<Choix> = sources.iter().map(|s| Choix { id: s.id(), nom: s.nom() }).collect();
    let journal = l.journal(source.as_deref(), PASSAGES_JOURNAL).map_err(texte)?.iter().map(|p| LigneJournal {
        debut: F::jour_heure(&p.debut),
        source: noms.iter().find(|c| c.id == p.source).map(|c| c.nom.to_string()).unwrap_or_else(|| p.source.clone()),
        couleur: etat_source(Some(p), false).couleur,
        duree: duree(p),
        lignes: F::entier(p.lignes as f64),
        erreur: erreur(p),
    }).collect();

    Ok(Sante {
        sous_titre,
        kpis: kpis(&l, &jour, &pannes, ok)?,
        pause,
        sources: lignes,
        ia: ia(&l, if hier { jour::decale(&jour, -1) } else { jour.clone() }, &jour)?,
        cout: cout(&l, &jour)?,
        journal,
        choix: noms,
    })
}

fn kpis(l: &Lectures, jour: &str, pannes: &[&str], ok: usize) -> Rep<Vec<Kpi>> {
    let valeur = |m: &str| l.valeur(m, jour, false).map_err(texte);
    let serie = |m: &str| l.jours(m, JOURS_COUT, jour, false).map_err(texte);
    let echecs = valeur(M::ECHECS_IA)?;
    let echecs7: Vec<f64> = l.jours(M::ECHECS_IA, 7, jour, false).map_err(texte)?.into_iter().flatten().collect();
    Ok(vec![
        Kpi {
            titre: "Sources en panne",
            valeur: Some(pannes.len() as f64),
            format: "entier",
            delta: Some(if pannes.is_empty() { format!("{ok} ok") } else { pannes.join(", ") }),
            ton: if pannes.is_empty() { "positif" } else { "negatif" },
            serie: None,
        },
        Kpi {
            titre: "Coût IA du jour",
            valeur: valeur(M::COUT_IA)?,
            format: "euros",
            delta: valeur(M::COUT_VOIX)?.map(F::dont_voix),
            ton: "neutre",
            serie: Some(serie(M::COUT_IA)?),
        },
        Kpi {
            titre: "Échecs IA du jour",
            valeur: echecs,
            format: "entier",
            delta: (!echecs7.is_empty()).then(|| format!("7 j : {}", F::entier(echecs7.iter().sum()))),
            ton: if echecs.unwrap_or(0.0) > 0.0 { "negatif" } else { "neutre" },
            serie: Some(serie(M::ECHECS_IA)?),
        },
        Kpi {
            titre: "Messages ouverts",
            valeur: valeur(M::MESSAGES_OUVERTS)?,
            format: "entier",
            delta: valeur(M::RETOURS_NOUVEAUX)?.map(|n| format!("{} {}", F::entier(n), if n > 1.0 { "retours" } else { "retour" })),
            ton: "neutre",
            serie: Some(serie(M::MESSAGES_OUVERTS)?),
        },
    ])
}

/// Les fournisseurs appelés ce jour-là, les plus sollicités d'abord ; la couleur suit l'ordre sur 14 jours.
fn ia(l: &Lectures, jour: String, aujourdhui: &str) -> Rep<Vec<LigneIA>> {
    let mut lus = Vec::new();
    for (i, f) in l.fournisseurs_ia(&jour::decale(aujourdhui, 1 - JOURS_COUT)).map_err(texte)?.into_iter().enumerate() {
        let valeur = |m: String, moyenne: bool| l.valeur(&m, &jour, moyenne).map_err(texte);
        let Some(n) = valeur(M::requetes_ia(&f), false)?.filter(|n| *n > 0.0) else { continue };
        let heures = l.heures(&M::requetes_ia(&f), &jour, false).map_err(texte)?.into_iter().map(|v| v.unwrap_or(0.0)).collect();
        lus.push((i, n, valeur(M::q429_ia(&f), false)?.unwrap_or(0.0), valeur(M::erreurs_ia(&f), false)?.unwrap_or(0.0),
                  valeur(M::latence_ia(&f), true)?, heures, f));
    }
    lus.sort_by(|a, b| b.1.total_cmp(&a.1));
    let total: f64 = lus.iter().map(|x| x.1).sum();
    let taux = |n: f64| Taux {
        texte: F::pourcent(n / total, 1),
        aide: format!("{} sur {}", F::entier(n), F::entier(total)),
        alerte: n > 0.0,
    };
    Ok(lus.into_iter().map(|(i, n, q429, erreurs, latence, heures, f)| LigneIA {
        nom: firebase::nom_fournisseur(&f),
        couleur: PALETTE[i.min(PALETTE.len() - 1)],
        requetes: F::entier(n),
        part: F::pourcent(n / total, 0),
        q429: taux(q429),
        erreurs: taux(erreurs),
        latence: latence.map(|ms| format!("{} ms", F::entier(ms))).unwrap_or_else(|| F::VIDE.into()),
        heures,
    }).collect())
}

/// Le coût des 14 derniers jours et le total du mois en cours.
fn cout(l: &Lectures, jour: &str) -> Rep<Cout> {
    let cles = jour::derniers(JOURS_COUT, jour);
    let valeurs = l.jours(M::COUT_IA, JOURS_COUT, jour, false).map_err(texte)?;
    let jour_du_mois: i64 = jour[8..].parse().map_err(texte)?;
    let mois: Vec<f64> = l.jours(M::COUT_IA, jour_du_mois, jour, false).map_err(texte)?.into_iter().flatten().collect();
    let mut jours = Vec::new();
    for (i, (c, v)) in cles.iter().zip(valeurs).enumerate() {
        let voix = l.valeur(M::COUT_VOIX, c, false).map_err(texte)?;
        jours.push(JourCout {
            etiquette: (cles.len() - 1 - i).is_multiple_of(4).then(|| F::jour_court_cle(c)),
            valeur: v,
            texte: format!("{} : {}{}", F::jour_court_cle(c), v.map(F::euros).unwrap_or_else(|| F::VIDE.into()),
                           voix.map(|x| format!(", {}", F::dont_voix(x))).unwrap_or_default()),
        });
    }
    Ok(Cout { mois: (!mois.is_empty()).then(|| format!("Ce mois : {}", F::euros(mois.iter().sum()))), jours })
}

/// L'état d'une source tel que le point le montre : en cours, jamais lue, ok, à régler ou en panne.
pub fn etat_source(p: Option<&Passage>, en_cours: bool) -> EtatSource {
    let (texte, couleur) = match p {
        _ if en_cours => ("…", "texte"),
        None => ("jamais lue", "tertiaire"),
        Some(p) => match p.etat {
            EtatPassage::Ok => ("ok", "succes"),
            EtatPassage::NonConfigure => ("à régler", "tertiaire"),
            EtatPassage::Erreur => ("panne", "danger"),
        },
    };
    EtatSource { texte, couleur }
}

fn duree(p: &Passage) -> String { F::passage((p.fin - p.debut).num_milliseconds() as f64 / 1000.0) }

/// L'erreur d'un passage, ambre si c'est une panne, sur une ligne.
fn erreur(p: &Passage) -> Option<Erreur> {
    let e = p.erreur.as_ref()?;
    Some(Erreur {
        ligne: e.split_whitespace().collect::<Vec<_>>().join(" "),
        entiere: e.clone(),
        couleur: if p.etat == EtatPassage::Erreur { "ambre" } else { "secondaire" },
    })
}

