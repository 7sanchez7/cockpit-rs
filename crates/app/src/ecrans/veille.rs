//! Veille : la bank des vidéos trouvées, filtrées par facettes et triées, leur carte au survol et leur fiche au clic,
//! et la conversation avec Claude qui y ajoute des vidéos (docs/ecrans.md § Veille).
use crate::commandes::{texte, Rep};
use crate::ecrans::lectures;
use crate::Etat;
use noyau::collecte::miniatures;
use noyau::erreur::Res;
use noyau::jour::Date;
use noyau::lecture::format;
use noyau::lecture::recherche::plier;
use noyau::modeles::Plateforme;
use noyau::modeles_veille::{AnalyseVeille, DemandeVeille, MotVeille, ScoreVeille, VideoVeille};
use noyau::p;
use noyau::sources::veille::{analyse, demande, videos_par_mot, ID, PLATEFORMES, REGLAGE_VIDEOS, VIDEOS_MAX, VIDEOS_MIN, VIDEOS_PAS};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::State;

#[derive(Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tri { Ratio, Vues, Vitesse, Engagement, Recentes, Ajoutees }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EcranVeille {
    sous_titre: String,
    /// Les vidéos qui passent les filtres.
    total: usize,
    /// Toute la bank.
    bank: usize,
    /// Les `limite` premières vidéos filtrées et triées.
    videos: Vec<CaseVeille>,
    /// D'autres vidéos suivent : la grille en charge plus en bas de page.
    suite: bool,
    facettes: Vec<Facette>,
    /// Les filtres choisis, en jetons au-dessus de la grille.
    actifs: Vec<Actif>,
    mots: Vec<PiluleMot>,
    curseur: Curseur,
}

/// Un filtre du panneau : ses options et combien de vidéos chacune donnerait avec les autres filtres.
#[derive(Serialize)]
pub struct Facette {
    cle: &'static str,
    titre: &'static str,
    /// Une option au plus (seuils emboîtés) ; sinon plusieurs, en « ou ».
    unique: bool,
    options: Vec<OptionFacette>,
}

#[derive(Serialize)]
pub struct OptionFacette {
    valeur: String,
    libelle: String,
    n: usize,
    choisi: bool,
}

#[derive(Serialize)]
pub struct Actif {
    cle: &'static str,
    valeur: String,
    libelle: String,
}

/// Les vidéos gardées par mot-clé à chaque passage.
#[derive(Serialize)]
pub struct Curseur {
    min: i64,
    max: i64,
    pas: i64,
    valeur: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PiluleMot {
    business: String,
    mot: String,
    actif: bool,
    /// Les vidéos trouvées par ce mot.
    videos: usize,
    aide: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseVeille {
    id: String,
    plateforme: Plateforme,
    nom_plateforme: &'static str,
    cles: Vec<String>,
    url: String,
    vues: String,
    /// La mesure du tri, en pastille sur la miniature.
    badge: Option<String>,
    outlier: bool,
    legende: String,
    /// L'app promue, sous la légende.
    app: Option<String>,
    /// La carte au survol de la miniature.
    carte: Carte,
}

#[derive(Serialize)]
pub struct Carte {
    compte: String,
    /// « il y a 3 j · TikTok · 42 s ».
    meta: String,
    viralite: Viralite,
    chiffres: Vec<Chiffre>,
    /// L'app promue, la langue, le genre.
    etiquettes: Vec<Pastille>,
    /// Les blocs du script ; vide sans analyse.
    anatomie: Vec<Segment>,
    /// Sans analyse : pourquoi.
    attente: &'static str,
}

/// Le coefficient de viralité : les vues rapportées à la médiane du compte.
#[derive(Serialize)]
pub struct Viralite {
    /// « 471× ».
    valeur: String,
    /// « Virale », « Percée », « Au-dessus de sa médiane », « Sous sa médiane », « Médiane inconnue ».
    verdict: &'static str,
    /// Le jeton de couleur du chiffre et du verdict.
    couleur: &'static str,
    /// « 3,7 M vues, médiane du compte 7 850 ».
    detail: String,
}

#[derive(Serialize)]
pub struct Chiffre {
    nom: &'static str,
    valeur: String,
}

#[derive(Serialize)]
pub struct Segment {
    nom: &'static str,
    couleur: &'static str,
    texte: String,
    /// La part de la durée en %, inconnue des analyses d'avant le 19 nov. 2026 : la barre les montre à parts égales.
    part: Option<i64>,
}

/// La fenêtre ouverte au clic sur une vidéo.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FicheVeille {
    id: String,
    plateforme: Plateforme,
    nom_plateforme: &'static str,
    cles: Vec<String>,
    url: String,
    compte: String,
    meta: String,
    mot: Option<String>,
    legende: Option<String>,
    viralite: Viralite,
    /// « 15,8 % ».
    engagement: Option<String>,
    /// « Top 6 % de la bank ».
    rang: Option<String>,
    chiffres: Vec<Chiffre>,
    etiquettes: Vec<Pastille>,
    anatomie: Vec<Segment>,
    analyse: Option<Analyse>,
    attente: &'static str,
    similaires: Vec<CaseVeille>,
}

#[derive(Serialize)]
pub struct Analyse {
    pastilles: Vec<Pastille>,
    sections: Vec<Section>,
}

#[derive(Serialize)]
pub struct Pastille {
    texte: String,
    couleur: Option<&'static str>,
}

#[derive(Serialize)]
pub struct Section {
    titre: &'static str,
    texte: String,
    /// La dernière section, l'idée à tourner, se lit en couleur de texte.
    fort: bool,
}

/// La conversation avec Claude : les fils à gauche, celui ouvert à droite.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatVeille {
    fils: Vec<FilEcran>,
    /// Le fil ouvert ; aucun : une nouvelle conversation.
    fil: Option<String>,
    /// Le business du fil ouvert : ses suites vont à lui.
    business: Option<String>,
    demandes: Vec<DemandeEcran>,
    /// La veille tourne (passage ou demande) : la saisie attend.
    occupe: bool,
    prompt_max: usize,
}

#[derive(Serialize)]
pub struct FilEcran {
    id: String,
    business: String,
    titre: String,
    /// « il y a 2 h, 3 demandes ».
    meta: String,
    etat: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandeEcran {
    id: String,
    prompt: String,
    /// en_cours, finie ou echouee.
    etat: &'static str,
    /// « il y a 3 min, 0,04 $ ».
    meta: String,
    etapes: Vec<Etape>,
    synthese: Option<String>,
    erreur: Option<String>,
    retenues: Vec<Retenue>,
}

/// Une ligne du journal d'une demande, et son icône.
#[derive(Serialize)]
pub struct Etape {
    genre: &'static str,
    texte: String,
}

#[derive(Serialize)]
pub struct Retenue {
    case: CaseVeille,
    raison: String,
}

/// Les vidéos proches montrées sous une fiche.
const SIMILAIRES: usize = 8;

#[tauri::command]
pub async fn veille(
    etat: State<'_, Etat>,
    business: Option<String>,
    choix: HashMap<String, Vec<String>>,
    recherche: String,
    tri: Tri,
    limite: usize,
) -> Rep<EcranVeille> {
    let l = lectures(&etat.store, business.as_deref())?;
    let m = l.maintenant;
    let mots = l.mots_veille().map_err(texte)?;
    let bank = l.bank_veille().map_err(texte)?;
    let Filtre { mut liste, facettes, actifs } = filtrer(&bank, &choix, &recherche, &m);
    trier(&mut liste, tri);
    Ok(EcranVeille {
        sous_titre: sous_titre(&mots, &bank, &m),
        total: liste.len(),
        bank: bank.len(),
        suite: liste.len() > limite,
        videos: liste.iter().take(limite.max(1)).map(|v| case(v, tri, &m)).collect(),
        facettes,
        actifs,
        mots: mots.iter().map(|w| pilule(w, &bank, &m)).collect(),
        curseur: Curseur { min: VIDEOS_MIN, max: VIDEOS_MAX, pas: VIDEOS_PAS, valeur: videos_par_mot(&etat.store).map_err(texte)? },
    })
}

/// Les fils de conversation et celui ouvert, ses demandes avec leurs étapes et leurs vidéos retenues.
#[tauri::command]
pub async fn chat_veille(etat: State<'_, Etat>, business: Option<String>, fil: Option<String>) -> Rep<ChatVeille> {
    let l = lectures(&etat.store, business.as_deref())?;
    let m = l.maintenant;
    let menee = etat.collecteur.en_cours().contains(ID);
    let fils = l.fils_veille().map_err(texte)?;
    let ouvert = fil.and_then(|f| fils.iter().find(|x| x.id == f));
    let demandes = match ouvert {
        Some(f) => l.fil_veille(&f.id).and_then(|ds| ds.into_iter().map(|d| {
            let cout = l.cout_demande(&d)?;
            let retenues = l.retenues_veille(&d.id)?.into_iter()
                .map(|(v, raison)| Retenue { case: case(&v, Tri::Ratio, &m), raison }).collect();
            Ok(demande_ecran(d, menee, cout, retenues, &m))
        }).collect::<Res<Vec<_>>>()).map_err(texte)?,
        None => vec![],
    };
    Ok(ChatVeille {
        fil: ouvert.map(|f| f.id.clone()),
        business: ouvert.map(|f| f.business.clone()),
        demandes,
        occupe: menee,
        prompt_max: demande::PROMPT_MAX,
        fils: fils.iter().map(|f| FilEcran {
            id: f.id.clone(),
            business: f.business.clone(),
            titre: f.titre.clone(),
            meta: format::majuscule(&format::il_ya(&f.dernier, &m))
                + &if f.demandes > 1 { format!(", {} demandes", f.demandes) } else { String::new() },
            etat: etat_demande(&f.etat, menee),
        }).collect(),
    })
}

/// Demande à Claude des vidéos en fond, à la suite du fil `fil` ou dans un nouveau : Sonnet choisit les mots-clés,
/// Haiku cherche, analyse et retient. Elle réserve la veille, comme un passage. Rend le fil.
#[tauri::command]
pub async fn demander_veille(etat: State<'_, Etat>, business: String, prompt: String, fil: Option<String>) -> Rep<String> {
    let reservation = etat.collecteur.reserver(ID).ok_or("La veille tourne déjà, réessaie quand elle a fini.")?;
    let ctx = etat.collecteur.contexte().map_err(texte)?;
    let d = demande::creer(&etat.store, &business, &prompt, fil.as_deref()).map_err(texte)?;
    let fil = d.fil.clone();
    tauri::async_runtime::spawn(async move {
        demande::mener(&ctx, &d).await;
        drop(reservation);
    });
    Ok(fil)
}

/// Efface une conversation avec Claude, sauf pendant qu'il y cherche.
#[tauri::command]
pub async fn supprimer_fil_veille(etat: State<'_, Etat>, fil: String) -> Rep<()> {
    let menee = etat.collecteur.en_cours().contains(ID);
    let fils = lectures(&etat.store, None)?.fils_veille().map_err(texte)?;
    if fils.iter().any(|f| f.id == fil && etat_demande(&f.etat, menee) == DemandeVeille::EN_COURS) {
        return Err("Claude cherche encore dans cette conversation.".into());
    }
    etat.store.supprimer_fil(&fil).map_err(texte)?;
    etat.store.a_change();
    Ok(())
}

/// Une demande en cours que plus rien ne mène (app quittée en route) se lit échouée.
fn etat_demande(etat: &str, menee: bool) -> &'static str {
    match etat {
        DemandeVeille::EN_COURS if menee => DemandeVeille::EN_COURS,
        DemandeVeille::FINIE => DemandeVeille::FINIE,
        _ => DemandeVeille::ECHOUEE,
    }
}

fn demande_ecran(d: DemandeVeille, menee: bool, cout: f64, retenues: Vec<Retenue>, m: &Date) -> DemandeEcran {
    let perdue = d.etat == DemandeVeille::EN_COURS && !menee;
    DemandeEcran {
        etat: etat_demande(&d.etat, menee),
        meta: format!("{}, {} $", format::majuscule(&format::il_ya(&d.debut, m)), format::decimal(cout, 2)),
        erreur: if perdue { Some(DemandeVeille::INTERROMPUE.into()) } else { d.erreur },
        etapes: d.journal.iter().map(|x| etape(x)).collect(),
        id: d.id,
        prompt: d.prompt,
        synthese: d.synthese,
        retenues,
    }
}

/// Le début d'une ligne du journal (`demande.rs`) et son genre.
const ETAPES: [(&str, &str); 8] = [
    ("Sonnet", "pense"), ("Mots-clés", "mots"), ("Recherche ", "cherche"), ("« ", "trouve"), ("Miniatures", "image"),
    ("Analyse de ", "analyse"), ("Retenue", "retenue"), ("Échec", "echec"),
];

fn etape(ligne: &str) -> Etape {
    let genre = ETAPES.iter().find(|(debut, _)| ligne.starts_with(debut)).map_or("info", |(_, g)| g);
    Etape { genre, texte: ligne.into() }
}

/// Le curseur : lu au début du passage suivant.
#[tauri::command]
pub async fn regler_videos(etat: State<'_, Etat>, nombre: i64) -> Rep<()> {
    etat.store.regler(REGLAGE_VIDEOS, Some(&nombre.clamp(VIDEOS_MIN, VIDEOS_MAX).to_string())).map_err(texte)?;
    etat.store.a_change();
    Ok(())
}

/// Ajoute un mot-clé ; la veille le cherche au prochain lancement.
#[tauri::command]
pub async fn ajouter_mot(etat: State<'_, Etat>, business: String, mot: String, vues_min: i64) -> Rep<()> {
    let mot = mot.trim();
    if mot.is_empty() {
        return Err("mot-clé vide".into());
    }
    etat.store.ajouter_mot(&business, mot, vues_min.max(0)).map_err(texte)?;
    etat.store.a_change();
    Ok(())
}

#[tauri::command]
pub async fn activer_mot(etat: State<'_, Etat>, business: String, mot: String, actif: bool) -> Rep<()> {
    etat.store.activer_mot(&business, &mot, actif).map_err(texte)?;
    etat.store.a_change();
    Ok(())
}

#[tauri::command]
pub async fn retirer_mot(etat: State<'_, Etat>, business: String, mot: String) -> Rep<()> {
    etat.store.retirer_mot(&business, &mot).map_err(texte)?;
    etat.store.a_change();
    Ok(())
}

/// La fiche d'une vidéo de la bank.
#[tauri::command]
pub async fn fiche_veille(etat: State<'_, Etat>, business: Option<String>, id: String) -> Rep<FicheVeille> {
    let l = lectures(&etat.store, business.as_deref())?;
    let v = etat.store.videos_veille("where v.id = ?", p![id]).map_err(texte)?.into_iter().next()
        .ok_or("vidéo absente de la veille")?;
    let bank = l.bank_veille().map_err(texte)?;
    Ok(fiche(&v, &bank, &l.maintenant))
}

/// Le fichier de la vidéo, téléchargé à la demande dans le cache (`analyse::fichier`), pour le lecteur de l'écran ;
/// la seule commande de la Veille qui parle au réseau.
#[tauri::command]
pub async fn lire_video(etat: State<'_, Etat>, id: String) -> Rep<String> {
    let v = etat.store.videos_veille("where v.id = ?", p![id]).map_err(texte)?.into_iter().next()
        .ok_or("vidéo absente de la veille")?;
    let http = etat.collecteur.contexte().map_err(texte)?.http;
    let f = analyse::fichier(&v, &http).await.map_err(texte)?;
    Ok(f.to_string_lossy().into_owned())
}

// facettes

/// Une valeur d'une vidéo pour une facette, et son libellé quand la facette n'a pas d'options fixes.
type Valeurs = fn(&VideoVeille, &Date) -> Vec<(String, String)>;

struct Def {
    cle: &'static str,
    titre: &'static str,
    unique: bool,
    /// Les options toujours montrées, dans l'ordre ; les autres viennent des vidéos, les plus fréquentes d'abord.
    fixes: Vec<(String, String)>,
    valeurs: Valeurs,
}

/// Publiée depuis moins de n jours.
const PUBLIEE: [(i64, &str); 4] = [(7, "7 derniers jours"), (30, "30 derniers jours"), (90, "3 derniers mois"), (365, "Moins d'un an")];
const VUES: [(i64, &str); 4] = [(10_000, "10 k et plus"), (100_000, "100 k et plus"), (1_000_000, "1 M et plus"), (10_000_000, "10 M et plus")];
/// Moins de n secondes, la dernière tranche sans borne.
const DUREES: [(f64, &str, &str); 4] = [
    (15.0, "15", "Moins de 15 s"), (30.0, "30", "15 à 30 s"), (60.0, "60", "30 s à 1 min"), (f64::INFINITY, "plus", "Plus d'une minute"),
];

fn val(x: impl Into<String>) -> (String, String) { (x.into(), String::new()) }

/// Une app quelle que soit sa graphie : « Voca AI », « vocaai » et « Voca-AI » sont la même.
fn cle_app(app: &str) -> String { plier(app).chars().filter(|c| c.is_alphanumeric()).collect() }

fn fixes(l: impl IntoIterator<Item = (String, &'static str)>) -> Vec<(String, String)> {
    l.into_iter().map(|(v, n)| (v, n.to_string())).collect()
}

/// Les facettes, dans l'ordre du panneau.
fn defs() -> Vec<Def> {
    vec![
        Def {
            cle: "performance", titre: "Performance", unique: true,
            fixes: fixes([("virale".into(), "Virales"), ("percee".into(), "2× la médiane et plus"), ("dessus".into(), "Au-dessus de la médiane")]),
            valeurs: |v, _| {
                let r = v.ratio().unwrap_or(0.0);
                [(v.outlier(), "virale"), (r >= ScoreVeille::RATIO_PERCEE, "percee"), (r >= 1.0, "dessus")]
                    .into_iter().filter(|(oui, _)| *oui).map(|(_, x)| val(x)).collect()
            },
        },
        Def {
            cle: "publiee", titre: "Publiée", unique: true,
            fixes: fixes(PUBLIEE.iter().map(|(j, n)| (j.to_string(), *n)).chain([("vieille".to_string(), "Plus d'un an")])),
            valeurs: |v, m| {
                let Some(p) = v.publiee else { return vec![] };
                let jours = (*m - p).num_days();
                let mut r: Vec<_> = PUBLIEE.iter().filter(|(j, _)| jours < *j).map(|(j, _)| val(j.to_string())).collect();
                if jours >= 365 {
                    r.push(val("vieille"));
                }
                r
            },
        },
        Def {
            cle: "vues", titre: "Vues", unique: true,
            fixes: fixes(VUES.iter().map(|(n, l)| (n.to_string(), *l))),
            valeurs: |v, _| VUES.iter().filter(|(n, _)| v.vues.is_some_and(|x| x >= *n)).map(|(n, _)| val(n.to_string())).collect(),
        },
        Def {
            cle: "plateforme", titre: "Plateforme", unique: false,
            fixes: fixes(PLATEFORMES.iter().map(|p| (p.brut().to_string(), p.nom()))),
            valeurs: |v, _| vec![val(v.plateforme.brut())],
        },
        Def {
            cle: "app", titre: "App promue", unique: false,
            fixes: fixes([("*".into(), "Avec une app nommée")]),
            valeurs: |v, _| v.app_promue().map_or(vec![], |a| vec![val("*"), (cle_app(a), a.to_string())]),
        },
        Def {
            cle: "langue", titre: "Langue", unique: false, fixes: vec![],
            valeurs: |v, _| v.langue.as_deref().map(|c| (c.to_string(), VideoVeille::nom_langue(c).to_string())).into_iter().collect(),
        },
        Def {
            cle: "genre", titre: "Genre", unique: false, fixes: vec![],
            valeurs: |v, _| v.genre.as_deref().map(|g| (g.to_string(), format::majuscule(g))).into_iter().collect(),
        },
        Def {
            cle: "duree", titre: "Durée", unique: false,
            fixes: fixes(DUREES.iter().map(|(_, x, n)| (x.to_string(), *n))),
            valeurs: |v, _| v.duree_s.and_then(|d| DUREES.iter().find(|(max, _, _)| d < *max)).map(|(_, x, _)| val(*x)).into_iter().collect(),
        },
        Def {
            cle: "analyse", titre: "Analyse de Claude", unique: true,
            fixes: fixes([("oui".into(), "Analysées"), ("adaptable".into(), "Adaptables (2/3 et plus)"), ("refaisable".into(), "À refaire telles quelles (3/3)"), ("non".into(), "Pas analysées")]),
            valeurs: |v, _| match &v.analyse {
                Some(a) => [(true, "oui"), (a.adaptable >= 2, "adaptable"), (a.adaptable >= 3, "refaisable")]
                    .into_iter().filter(|(oui, _)| *oui).map(|(_, x)| val(x)).collect(),
                None => vec![val("non")],
            },
        },
        Def {
            cle: "hook", titre: "Type de hook", unique: false, fixes: vec![],
            valeurs: |v, _| v.analyse.as_ref().map(|a| (a.hook_type.clone(), format::majuscule(&a.hook_type))).into_iter().collect(),
        },
        Def {
            cle: "visuel", titre: "Hook visuel", unique: false, fixes: vec![],
            valeurs: |v, _| v.analyse.as_ref().map(|a| (a.hook_visuel.clone(), format::majuscule(&a.hook_visuel))).into_iter().collect(),
        },
        Def {
            cle: "format", titre: "Format", unique: false, fixes: vec![],
            valeurs: |v, _| v.analyse.as_ref().map(|a| (a.format.clone(), format::majuscule(&a.format))).into_iter().collect(),
        },
        Def {
            cle: "source", titre: "Trouvée par", unique: false,
            fixes: fixes([("claude".into(), "Claude")]),
            valeurs: |v, _| {
                let mut r: Vec<_> = v.mot.as_deref().map(|w| (format!("mot:{w}"), format!("« {w} »"))).into_iter().collect();
                if v.retenue {
                    r.push(val("claude"));
                }
                r
            },
        },
        Def {
            cle: "compte", titre: "Compte", unique: false, fixes: vec![],
            valeurs: |v, _| vec![(v.compte.clone(), format!("@{}", v.compte))],
        },
    ]
}

struct Filtre<'a> {
    liste: Vec<&'a VideoVeille>,
    facettes: Vec<Facette>,
    actifs: Vec<Actif>,
}

/// Les vidéos qui passent la recherche et chaque facette (« ou » dans une facette, « et » entre elles), et le
/// nombre de chaque option : les vidéos qu'elle donnerait, les choix des autres facettes gardés.
fn filtrer<'a>(videos: &'a [VideoVeille], choix: &HashMap<String, Vec<String>>, recherche: &str, m: &Date) -> Filtre<'a> {
    let defs = defs();
    let termes: Vec<String> = plier(recherche).split_whitespace().map(String::from).collect();
    let choisis: Vec<&[String]> = defs.iter().map(|d| choix.get(d.cle).map_or(&[][..], Vec::as_slice)).collect();
    let mut comptes: Vec<HashMap<String, usize>> = vec![HashMap::new(); defs.len()];
    let mut libelles: Vec<HashMap<String, String>> = vec![HashMap::new(); defs.len()];
    let mut liste = vec![];
    for v in videos {
        let vals: Vec<Vec<(String, String)>> = defs.iter().map(|d| (d.valeurs)(v, m)).collect();
        for (i, vs) in vals.iter().enumerate() {
            for (x, lib) in vs {
                let garde = libelles[i].entry(x.clone()).or_insert_with(|| lib.clone());
                // la graphie de la marque plutôt que tout en minuscules : « Pingo AI », pas « pingo ai »
                if !garde.chars().any(char::is_uppercase) && lib.chars().any(char::is_uppercase) {
                    garde.clone_from(lib);
                }
            }
        }
        if !trouvee(v, &termes) {
            continue;
        }
        let echecs: Vec<usize> = (0..defs.len())
            .filter(|&i| !choisis[i].is_empty() && !vals[i].iter().any(|(x, _)| choisis[i].contains(x)))
            .collect();
        // une vidéo qui ne manque qu'une facette compte pour les options de celle-là
        let a_compter: Vec<usize> = match echecs[..] {
            [] => (0..defs.len()).collect(),
            [i] => vec![i],
            _ => vec![],
        };
        for i in a_compter {
            for (x, _) in &vals[i] {
                *comptes[i].entry(x.clone()).or_default() += 1;
            }
        }
        if echecs.is_empty() {
            liste.push(v);
        }
    }
    let mut actifs = vec![];
    let facettes = defs.iter().enumerate().filter_map(|(i, d)| {
        let n = |x: &str| comptes[i].get(x).copied().unwrap_or(0);
        let choisi = |x: &str| choisis[i].iter().any(|c| c == x);
        let option = |x: &str, lib: &str| OptionFacette {
            valeur: x.into(), libelle: if lib.is_empty() { x.into() } else { lib.into() }, n: n(x), choisi: choisi(x),
        };
        let mut options: Vec<OptionFacette> = d.fixes.iter().map(|(x, lib)| option(x, lib)).collect();
        let mut autres: Vec<OptionFacette> = libelles[i].iter()
            .filter(|(x, _)| !d.fixes.iter().any(|(f, _)| f == *x) && (n(x) > 0 || choisi(x)))
            .map(|(x, lib)| option(x, lib))
            .collect();
        for c in choisis[i] {
            if !options.iter().chain(&autres).any(|o| &o.valeur == c) {
                autres.push(option(c, c));
            }
        }
        autres.sort_by(|a, b| b.n.cmp(&a.n).then_with(|| a.libelle.cmp(&b.libelle)));
        options.extend(autres);
        actifs.extend(options.iter().filter(|o| o.choisi).map(|o| Actif {
            cle: d.cle, valeur: o.valeur.clone(), libelle: format!("{} : {}", d.titre, o.libelle),
        }));
        // une facette sans valeur nulle part (rien d'analysé encore) ne se montre pas
        (!options.is_empty()).then_some(Facette { cle: d.cle, titre: d.titre, unique: d.unique, options })
    }).collect();
    Filtre { liste, facettes, actifs }
}

/// Chaque terme se trouve dans la légende, le compte, l'app, le mot-clé, le genre ou l'analyse, sans casse ni accents.
fn trouvee(v: &VideoVeille, termes: &[String]) -> bool {
    if termes.is_empty() {
        return true;
    }
    let a = v.analyse.as_ref();
    let botte = plier(&[
        Some(v.compte.as_str()), v.legende.as_deref(), v.app_promue(), v.mot.as_deref(), v.genre.as_deref(),
        a.map(|a| a.hook.as_str()), a.map(|a| a.texte_ecran.as_str()), a.map(|a| a.probleme.as_str()),
        a.map(|a| a.idee.as_str()), a.map(|a| a.format.as_str()), a.map(|a| a.hook_type.as_str()),
    ].into_iter().flatten().collect::<Vec<_>>().join(" "));
    termes.iter().all(|t| botte.contains(t.as_str()))
}

/// « 215 vidéos dont 42 virales, 3 mots-clés, cherché il y a 2 h ».
fn sous_titre(mots: &[MotVeille], bank: &[VideoVeille], m: &Date) -> String {
    let mut r = vec![format!("{} vidéos dont {} virales", format::entier(bank.len() as f64), bank.iter().filter(|v| v.outlier()).count())];
    if !mots.is_empty() {
        r.push(format!("{} mots-clés", mots.len()));
    }
    if let Some(d) = mots.iter().filter_map(|w| w.cherche).max() {
        r.push(format!("cherché {}", format::il_ya(&d, m)));
    }
    r.join(", ")
}

/// Les vidéos sans la mesure du tri passent après les autres.
fn trier(v: &mut [&VideoVeille], tri: Tri) {
    let cle = |v: &VideoVeille| -> Option<f64> {
        match tri {
            Tri::Ratio => v.ratio(),
            Tri::Vues => v.vues.map(|n| n as f64),
            Tri::Vitesse => v.vitesse,
            Tri::Engagement => v.engagement(),
            Tri::Recentes => v.publiee.map(|d| d.timestamp_millis() as f64),
            Tri::Ajoutees => Some(v.trouvee.timestamp_millis() as f64),
        }
    };
    v.sort_by(|a, b| cle(b).unwrap_or(f64::NEG_INFINITY).total_cmp(&cle(a).unwrap_or(f64::NEG_INFINITY)));
}

/// La mesure d'une vidéo pour le tri choisi ; le tri par vues montre aussi le ratio, plus parlant.
fn badge(v: &VideoVeille, tri: Tri, m: &Date) -> Option<String> {
    match tri {
        Tri::Ratio | Tri::Vues => v.ratio().map(fois),
        Tri::Vitesse => v.vitesse.map(|x| format!("{}/h", format::compact(x))),
        Tri::Engagement => v.engagement().map(|e| format::pourcent(e, 1)),
        Tri::Recentes => v.publiee.map(|d| format::age(&d, m)),
        Tri::Ajoutees => Some(format::age(&v.trouvee, m)),
    }
}

/// « 2,5× », « 471× ».
fn fois(r: f64) -> String {
    format!("{}×", format::decimal(r, if r < 10.0 { 1 } else { 0 }))
}

fn pilule(w: &MotVeille, bank: &[VideoVeille], m: &Date) -> PiluleMot {
    PiluleMot {
        business: w.business.clone(),
        mot: w.mot.clone(),
        actif: w.actif,
        videos: bank.iter().filter(|v| v.mot.as_deref() == Some(w.mot.as_str())).count(),
        aide: format!("Seuil {} vues", format::entier(w.vues_min as f64))
            + &w.cherche.map_or(String::new(), |d| format!(", cherché {}", format::il_ya(&d, m))),
    }
}

fn case(v: &VideoVeille, tri: Tri, m: &Date) -> CaseVeille {
    CaseVeille {
        id: v.id.clone(),
        plateforme: v.plateforme,
        nom_plateforme: v.plateforme.nom(),
        cles: miniatures::cles(&v.id, v.plateforme),
        url: v.url.clone(),
        vues: v.vues.map_or(format::VIDE.into(), |n| format::compact(n as f64)),
        badge: badge(v, tri, m),
        outlier: v.outlier(),
        legende: format!("@{}{}", v.compte, if v.analyse.is_some() { " ✦" } else { "" }),
        app: v.app_promue().map(String::from),
        carte: Carte {
            compte: format!("@{}", v.compte),
            meta: meta(v, m),
            viralite: viralite(v),
            chiffres: chiffres(&[
                ("Vues", v.vues.map(|n| format::compact(n as f64))),
                ("Likes", v.likes.map(|n| format::compact(n as f64))),
                ("Engagement", v.engagement().map(|e| format::pourcent(e, 1))),
            ]),
            etiquettes: etiquettes(v),
            anatomie: v.analyse.as_ref().map(anatomie).unwrap_or_default(),
            attente: attente(v),
        },
    }
}

/// L'app promue en violet, le genre en bleu, la langue.
fn etiquettes(v: &VideoVeille) -> Vec<Pastille> {
    [
        v.app_promue().map(|a| Pastille { texte: a.into(), couleur: Some("violet") }),
        v.genre.as_deref().map(|g| Pastille { texte: format::majuscule(g), couleur: Some("bleu") }),
        v.langue.as_deref().map(|c| Pastille { texte: VideoVeille::nom_langue(c).into(), couleur: None }),
    ].into_iter().flatten().collect()
}

/// « il y a 3 j · TikTok · 42 s ».
fn meta(v: &VideoVeille, m: &Date) -> String {
    [v.publiee.map(|d| format::il_ya(&d, m)), Some(v.plateforme.nom().to_string()), v.duree_s.map(format::duree)]
        .into_iter().flatten().collect::<Vec<_>>().join(" · ")
}

/// Les chiffres connus.
fn chiffres(liste: &[(&'static str, Option<String>)]) -> Vec<Chiffre> {
    liste.iter().filter_map(|(nom, valeur)| Some(Chiffre { nom, valeur: valeur.clone()? })).collect()
}

/// Virale pour un outlier, percée dès 2× la médiane.
fn viralite(v: &VideoVeille) -> Viralite {
    let compte = |n: Option<i64>| n.map_or(format::VIDE.into(), |n| format::compact(n as f64));
    let Some(r) = v.ratio() else {
        return Viralite {
            valeur: format::VIDE.into(),
            verdict: "Médiane inconnue",
            couleur: "tertiaire",
            detail: format!("{} vues, médiane du compte pas encore relevée", compte(v.vues)),
        };
    };
    let (verdict, couleur) = if v.outlier() {
        ("Virale", "succes")
    } else if r >= ScoreVeille::RATIO_PERCEE {
        ("Percée", "ambre")
    } else if r >= 1.0 {
        ("Au-dessus de sa médiane", "secondaire")
    } else {
        ("Sous sa médiane", "tertiaire")
    };
    Viralite {
        valeur: fois(r),
        verdict,
        couleur,
        detail: format!("{} vues, médiane du compte {}", compte(v.vues), compte(v.mediane)),
    }
}

fn attente(v: &VideoVeille) -> &'static str {
    if v.a_analyser() { "Analyse au prochain passage (clé Anthropic du business)." } else { "Analysée à partir de 2× la médiane du compte." }
}

fn fiche(v: &VideoVeille, bank: &[VideoVeille], m: &Date) -> FicheVeille {
    let compte = |n: Option<i64>| n.map(|n| format::compact(n as f64));
    FicheVeille {
        id: v.id.clone(),
        plateforme: v.plateforme,
        nom_plateforme: v.plateforme.nom(),
        cles: miniatures::cles(&v.id, v.plateforme),
        url: v.url.clone(),
        compte: format!("@{}", v.compte),
        meta: meta(v, m),
        mot: v.mot.clone(),
        legende: v.legende.clone().filter(|l| !l.trim().is_empty()),
        viralite: viralite(v),
        engagement: v.engagement().map(|e| format::pourcent(e, 1)),
        rang: rang(v, bank),
        chiffres: chiffres(&[
            ("Vues", v.vues.map(|n| format::entier(n as f64))),
            ("Likes", compte(v.likes)),
            ("Commentaires", compte(v.commentaires)),
            ("Partages", compte(v.partages)),
            ("Enregistrements", compte(v.enregistrements)),
            ("Vues/h", v.vitesse.map(format::compact)),
            ("Abonnés", compte(v.abonnes)),
            ("Pull", v.pull().map(|p| format::pourcent(p, 1))),
        ]),
        etiquettes: etiquettes(v),
        anatomie: v.analyse.as_ref().map(anatomie).unwrap_or_default(),
        analyse: v.analyse.as_ref().map(analyse),
        attente: attente(v),
        similaires: similaires(v, bank, m),
    }
}

/// La place de l'engagement de `v` parmi les vidéos de la bank qui en ont un ; la moitié basse ne se classe pas.
fn rang(v: &VideoVeille, bank: &[VideoVeille]) -> Option<String> {
    let e = v.engagement()?;
    let autres: Vec<f64> = bank.iter().filter(|x| x.id != v.id).filter_map(VideoVeille::engagement).collect();
    if autres.is_empty() {
        return None;
    }
    let devant = autres.iter().filter(|x| **x > e).count();
    let top = ((devant + 1) as f64 * 100.0 / (autres.len() + 1) as f64).ceil() as i64;
    Some(if top <= 50 { format!("Top {top} % de la bank") } else { "Moitié basse de la bank".into() })
}

/// Même mot-clé, même format d'analyse ou même app promue, les plus fortes d'abord.
fn similaires(v: &VideoVeille, bank: &[VideoVeille], m: &Date) -> Vec<CaseVeille> {
    let format = |x: &VideoVeille| x.analyse.as_ref().map(|a| a.format.clone());
    let app = |x: &VideoVeille| x.app_promue().map(cle_app);
    let mut proches: Vec<&VideoVeille> = bank.iter()
        .filter(|x| x.id != v.id)
        .filter(|x| (v.mot.is_some() && x.mot == v.mot) || (format(v).is_some() && format(x) == format(v)) || (app(v).is_some() && app(x) == app(v)))
        .collect();
    trier(&mut proches, Tri::Ratio);
    proches.into_iter().take(SIMILAIRES).map(|x| case(x, Tri::Ratio, m)).collect()
}

/// Les blocs présents du script, leurs parts ramenées à 100.
fn anatomie(a: &AnalyseVeille) -> Vec<Segment> {
    let (b, p) = (&a.blocs, a.parts.as_ref());
    let blocs = [
        ("Hook", "ambre", &b.hook, p.map(|p| p.hook)),
        ("Confirmation", "bleu", &b.confirmation, p.map(|p| p.confirmation)),
        ("Valeur", "succes", &b.valeur, p.map(|p| p.valeur)),
        ("Placement", "violet", &b.placement, p.map(|p| p.placement)),
        ("Fin", "orange", &b.fin, p.map(|p| p.fin)),
    ];
    let presents: Vec<_> = blocs.into_iter().filter(|(_, _, texte, part)| !absent(texte) && part.is_none_or(|n| n > 0)).collect();
    let total: i64 = presents.iter().filter_map(|s| s.3).sum();
    presents.into_iter()
        .map(|(nom, couleur, texte, part)| Segment { nom, couleur, texte: texte.clone(), part: part.filter(|_| total > 0).map(|n| (n as f64 * 100.0 / total as f64).round() as i64) })
        .collect()
}

fn absent(texte: &str) -> bool {
    texte.trim().trim_end_matches('.').eq_ignore_ascii_case("absent")
}

/// Hook, problème, ce qui change, l'idée ; les blocs passent par l'anatomie.
fn analyse(a: &AnalyseVeille) -> Analyse {
    let section = |titre, texte: &str| Section { titre, texte: texte.into(), fort: false };
    let hook = format!("« {} »", a.hook) + &if a.texte_ecran.is_empty() { String::new() } else { format!("\nÀ l'écran : {}", a.texte_ecran) };
    Analyse {
        pastilles: vec![
            Pastille { texte: a.hook_type.clone(), couleur: Some("ambre") },
            Pastille { texte: a.hook_visuel.clone(), couleur: None },
            Pastille { texte: a.format.clone(), couleur: None },
            Pastille { texte: format!("adaptable {}/3", a.adaptable), couleur: Some(if a.adaptable >= 2 { "succes" } else { "secondaire" }) },
        ],
        sections: vec![
            section("Hook", &hook),
            section("Problème", &a.probleme),
            section("Ce qui change", &a.ecart),
            Section { titre: "À tourner", texte: a.idee.clone(), fort: true },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video(id: &str, vues: i64, likes: i64) -> VideoVeille {
        VideoVeille {
            id: id.into(), business: "b".into(), plateforme: Plateforme::Tiktok, compte: "c".into(), url: String::new(),
            legende: None, publiee: None, duree_s: None, mot: None, trouvee: noyau::jour::maintenant(), media: None,
            vues: Some(vues), likes: Some(likes), commentaires: None, partages: None, enregistrements: None,
            releve_le: None, mediane: None, abonnes: None, vitesse: None, analyse: None, retenue: false,
            app: None, langue: None, genre: None,
        }
    }

    #[test]
    fn rang_dans_la_bank() {
        let bank: Vec<_> = (1..=19).map(|i| video(&i.to_string(), 1000, i)).collect();
        assert_eq!(rang(&video("x", 1000, 100), &bank).as_deref(), Some("Top 5 % de la bank"));
        assert_eq!(rang(&video("x", 1000, 0), &bank).as_deref(), Some("Moitié basse de la bank"));
        assert_eq!(rang(&video("x", 1000, 5), &[]), None);
    }

    #[test]
    fn anatomie_sans_blocs_absents() {
        let mut a: AnalyseVeille = serde_json::from_value(serde_json::json!({
            "hook": "h", "hookType": "question", "hookVisuel": "face", "texteEcran": "", "probleme": "p", "format": "f",
            "blocs": { "hook": "h", "confirmation": "c", "valeur": "v", "placement": "Absent.", "fin": "f" },
            "ecart": "e", "adaptable": 2, "idee": "i"
        })).unwrap();
        let noms = |s: &[Segment]| s.iter().map(|s| (s.nom, s.part)).collect::<Vec<_>>();
        assert_eq!(noms(&anatomie(&a)), [("Hook", None), ("Confirmation", None), ("Valeur", None), ("Fin", None)]);
        a.parts = Some(noyau::modeles_veille::Parts { hook: 10, confirmation: 0, valeur: 60, placement: 0, fin: 10 });
        assert_eq!(noms(&anatomie(&a)), [("Hook", Some(13)), ("Valeur", Some(75)), ("Fin", Some(13))]);
    }

    #[test]
    fn viralite_par_rapport_a_la_mediane() {
        let avec = |vues, likes, mediane| VideoVeille { mediane, ..video("x", vues, likes) };
        let verdict = |v: &VideoVeille| {
            let x = viralite(v);
            (x.valeur, x.verdict, x.couleur)
        };
        assert_eq!(verdict(&avec(3_700_000, 400_000, Some(7_850))), ("471×".into(), "Virale", "succes"));
        assert_eq!(verdict(&avec(25_000, 10, Some(10_000))), ("2,5×".into(), "Percée", "ambre"));
        assert_eq!(verdict(&avec(12_000, 10, Some(10_000))), ("1,2×".into(), "Au-dessus de sa médiane", "secondaire"));
        assert_eq!(verdict(&avec(5_000, 10, Some(10_000))), ("0,5×".into(), "Sous sa médiane", "tertiaire"));
        assert_eq!(verdict(&avec(5_000, 10, None)), ("…".into(), "Médiane inconnue", "tertiaire"));
        assert_eq!(viralite(&avec(3_700_000, 400_000, Some(7_850))).detail, "3,7 M vues, médiane du compte 7\u{202f}850");
    }

    fn bank() -> Vec<VideoVeille> {
        let m = noyau::jour::maintenant();
        vec![
            VideoVeille { langue: Some("es".into()), app: Some("Praktika".into()), publiee: Some(m - chrono::Duration::days(3)), ..video("1", 50_000, 10) },
            VideoVeille { langue: Some("fr".into()), plateforme: Plateforme::Instagram, publiee: Some(m - chrono::Duration::days(400)), ..video("2", 5_000, 10) },
            VideoVeille { langue: Some("es".into()), plateforme: Plateforme::Instagram, legende: Some("Él habla español".into()), ..video("3", 2_000_000, 10) },
        ]
    }

    fn choix(l: &[(&str, &[&str])]) -> HashMap<String, Vec<String>> {
        l.iter().map(|(c, v)| (c.to_string(), v.iter().map(|x| x.to_string()).collect())).collect()
    }

    fn option<'a>(f: &'a Filtre, cle: &str, valeur: &str) -> &'a OptionFacette {
        f.facettes.iter().find(|x| x.cle == cle).unwrap().options.iter().find(|o| o.valeur == valeur).unwrap()
    }

    #[test]
    fn facettes_en_ou_dedans_en_et_entre() {
        let (b, m) = (bank(), noyau::jour::maintenant());
        let ids = |f: &Filtre| f.liste.iter().map(|v| v.id.clone()).collect::<Vec<_>>();

        let f = filtrer(&b, &choix(&[]), "", &m);
        assert_eq!(ids(&f), ["1", "2", "3"]);
        // rien d'analysé : pas de facette de hook
        assert!(f.facettes.iter().all(|x| x.cle != "hook"));
        assert_eq!(option(&f, "app", "praktika").libelle, "Praktika");

        let f = filtrer(&b, &choix(&[("langue", &["es"]), ("plateforme", &["instagram"])]), "", &m);
        assert_eq!(ids(&f), ["3"]);
        // le français compte ce qu'il donnerait avec Instagram gardé, TikTok ce qu'il donnerait avec l'espagnol
        assert_eq!((option(&f, "langue", "fr").n, option(&f, "langue", "es").n), (1, 1));
        assert_eq!((option(&f, "plateforme", "tiktok").n, option(&f, "plateforme", "instagram").n), (1, 1));
        assert_eq!(f.actifs.iter().map(|a| a.libelle.as_str()).collect::<Vec<_>>(), ["Plateforme : Instagram", "Langue : Espagnol"]);

        let f = filtrer(&b, &choix(&[("langue", &["es", "fr"])]), "", &m);
        assert_eq!(ids(&f), ["1", "2", "3"]);

        let f = filtrer(&b, &choix(&[("vues", &["1000000"])]), "", &m);
        assert_eq!(ids(&f), ["3"]);
        // des seuils emboîtés : 10 k et plus compte aussi la vidéo à 50 k
        assert_eq!((option(&f, "vues", "10000").n, option(&f, "vues", "1000000").n), (2, 1));
        assert_eq!(option(&f, "publiee", "vieille").n, 0);
    }

    #[test]
    fn une_app_quelle_que_soit_sa_graphie() {
        let b = vec![
            VideoVeille { app: Some("voca ai".into()), ..video("1", 1_000, 10) },
            VideoVeille { app: Some("Voca-AI".into()), ..video("2", 1_000, 10) },
        ];
        let f = filtrer(&b, &choix(&[]), "", &noyau::jour::maintenant());
        let o = option(&f, "app", "vocaai");
        assert_eq!((o.libelle.as_str(), o.n), ("Voca-AI", 2));
    }

    #[test]
    fn recherche_sans_casse_ni_accents() {
        let (b, m) = (bank(), noyau::jour::maintenant());
        let ids = |r: &str| filtrer(&b, &choix(&[]), r, &m).liste.iter().map(|v| v.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids("EL ESPAÑOL"), ["3"]);
        assert_eq!(ids("praktika"), ["1"]);
        assert!(ids("praktika espanol").is_empty());
    }

    #[test]
    fn etapes_du_journal() {
        let genres = |l: &[&str]| l.iter().map(|x| etape(x).genre).collect::<Vec<_>>();
        assert_eq!(genres(&["Sonnet choisit les mots-clés", "Mots-clés : « a » (TikTok)", "Recherche « a » sur TikTok",
            "« a » sur TikTok : 12 vidéos", "Analyse de @x sur TikTok", "Retenue : @x sur TikTok", "Échec de x : e", "autre"]),
            ["pense", "mots", "cherche", "trouve", "analyse", "retenue", "echec", "info"]);
    }
}
