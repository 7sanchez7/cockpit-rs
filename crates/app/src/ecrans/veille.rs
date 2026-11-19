//! Veille : les vidéos qui montent sur les mots-clés suivis, triées et filtrées, et le détail de la vidéo choisie
//! avec l'analyse de Claude, et le lecteur (docs/ecrans.md § Veille).
use crate::commandes::{texte, Rep};
use crate::ecrans::lectures;
use crate::Etat;
use noyau::collecte::miniatures;
use noyau::jour::Date;
use noyau::lecture::format;
use noyau::modeles::Plateforme;
use noyau::modeles_veille::{AnalyseVeille, MotVeille, VideoVeille};
use noyau::p;
use noyau::sources::veille::{analyse, videos_par_mot, PLATEFORMES, REGLAGE_VIDEOS, VIDEOS_MAX, VIDEOS_MIN, VIDEOS_PAS};
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tri { Ratio, Vues, Vitesse, Recentes }

/// La fenêtre de la veille, en jours.
const JOURS: i64 = 30;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EcranVeille {
    sous_titre: String,
    mots: Vec<PiluleMot>,
    plateformes: Vec<Choix>,
    plateforme: Option<Plateforme>,
    mot: Option<String>,
    videos: Vec<CaseVeille>,
    /// Aucun mot-clé : la grille invite à en ajouter un.
    sans_mot: bool,
    choisie: Option<String>,
    detail: Option<DetailVeille>,
    curseur: Curseur,
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
pub struct Choix {
    id: Plateforme,
    libelle: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PiluleMot {
    business: String,
    mot: String,
    actif: bool,
    /// Les outliers trouvés par ce mot.
    outliers: usize,
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
    /// Les chiffres connus de la vidéo, sur la miniature au survol.
    stats: Vec<Stat>,
}

#[derive(Serialize)]
pub struct Stat {
    nom: &'static str,
    valeur: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailVeille {
    id: String,
    plateforme: Plateforme,
    nom_plateforme: &'static str,
    cles: Vec<String>,
    url: String,
    compte: String,
    meta: String,
    legende: Option<String>,
    mesures: Vec<LigneMesure>,
    analyse: Option<Analyse>,
    /// Sans analyse : pourquoi.
    attente: &'static str,
}

#[derive(Serialize)]
pub struct LigneMesure {
    nom: &'static str,
    valeur: Option<String>,
    succes: bool,
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

#[tauri::command]
pub async fn veille(
    etat: State<'_, Etat>,
    business: Option<String>,
    tri: Tri,
    plateforme: Option<Plateforme>,
    outliers: bool,
    mot: Option<String>,
    choisie: Option<String>,
) -> Rep<EcranVeille> {
    let l = lectures(&etat.store, business.as_deref())?;
    let m = l.maintenant;
    let mots = l.mots_veille().map_err(texte)?;
    let toutes = l.videos_veille(JOURS).map_err(texte)?;
    let mut liste: Vec<&VideoVeille> = toutes.iter()
        .filter(|v| plateforme.is_none_or(|p| v.plateforme == p))
        .filter(|v| !outliers || v.outlier())
        .filter(|v| mot.is_none() || v.mot == mot)
        .collect();
    trier(&mut liste, tri);
    let choisie = choisie.filter(|id| liste.iter().any(|v| &v.id == id)).or_else(|| liste.first().map(|v| v.id.clone()));
    Ok(EcranVeille {
        sous_titre: sous_titre(&mots, &toutes, &m),
        mots: mots.iter().map(|w| pilule(w, &toutes, &m)).collect(),
        plateformes: PLATEFORMES.iter().map(|p| Choix { id: *p, libelle: p.nom() }).collect(),
        plateforme,
        mot,
        videos: liste.iter().map(|v| case(v, tri, &m)).collect(),
        sans_mot: mots.is_empty(),
        detail: liste.iter().find(|v| Some(&v.id) == choisie.as_ref()).map(|v| detail(v, &m)),
        choisie,
        curseur: Curseur { min: VIDEOS_MIN, max: VIDEOS_MAX, pas: VIDEOS_PAS, valeur: videos_par_mot(&etat.store).map_err(texte)? },
    })
}

/// Le curseur : lu au début du passage suivant.
#[tauri::command]
pub async fn regler_videos(etat: State<'_, Etat>, nombre: i64) -> Rep<()> {
    etat.store.regler(REGLAGE_VIDEOS, Some(&nombre.clamp(VIDEOS_MIN, VIDEOS_MAX).to_string())).map_err(texte)?;
    etat.store.a_change();
    Ok(())
}

/// Ajoute un mot-clé ; la veille le cherche aussitôt.
#[tauri::command]
pub async fn ajouter_mot(etat: State<'_, Etat>, business: String, mot: String, vues_min: i64) -> Rep<()> {
    let mot = mot.trim();
    if mot.is_empty() {
        return Err("mot-clé vide".into());
    }
    etat.store.ajouter_mot(&business, mot, vues_min.max(0)).map_err(texte)?;
    etat.store.a_change();
    etat.collecteur.relancer(Some("veille"));
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

/// « 3 mots, 42 vidéos dont 9 outliers en 30 j, cherché il y a 2 h ».
fn sous_titre(mots: &[MotVeille], toutes: &[VideoVeille], m: &Date) -> String {
    if mots.is_empty() {
        return "Aucun mot-clé".into();
    }
    let cherche = mots.iter().filter_map(|w| w.cherche).max()
        .map_or("jamais cherché".to_string(), |d| format!("cherché {}", format::il_ya(&d, m)));
    [
        format!("{} mots", mots.len()),
        format!("{} vidéos dont {} outliers en {JOURS} j", format::entier(toutes.len() as f64), toutes.iter().filter(|v| v.outlier()).count()),
        cherche,
    ].join(", ")
}

/// Les vidéos sans la mesure du tri passent après les autres.
fn trier(v: &mut [&VideoVeille], tri: Tri) {
    let cle = |v: &VideoVeille| -> Option<f64> {
        match tri {
            Tri::Ratio => v.ratio(),
            Tri::Vues => v.vues.map(|n| n as f64),
            Tri::Vitesse => v.vitesse,
            Tri::Recentes => v.publiee.map(|d| d.timestamp_millis() as f64),
        }
    };
    v.sort_by(|a, b| cle(b).unwrap_or(f64::NEG_INFINITY).total_cmp(&cle(a).unwrap_or(f64::NEG_INFINITY)));
}

/// La mesure d'une vidéo pour le tri choisi ; le tri par vues montre aussi le ratio, plus parlant.
fn badge(v: &VideoVeille, tri: Tri, m: &Date) -> Option<String> {
    match tri {
        Tri::Ratio | Tri::Vues => v.ratio().map(|r| format!("{}×", format::decimal(r, if r < 10.0 { 1 } else { 0 }))),
        Tri::Vitesse => v.vitesse.map(|x| format!("{}/h", format::compact(x))),
        Tri::Recentes => v.publiee.map(|d| format::age(&d, m)),
    }
}

fn pilule(w: &MotVeille, toutes: &[VideoVeille], m: &Date) -> PiluleMot {
    PiluleMot {
        business: w.business.clone(),
        mot: w.mot.clone(),
        actif: w.actif,
        outliers: toutes.iter().filter(|v| v.mot.as_deref() == Some(w.mot.as_str()) && v.outlier()).count(),
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
        stats: stats(v, m),
    }
}

/// Publiée, vues, likes, commentaires, partages, enregistrements, × médiane, vues/h, durée : ceux qui sont connus.
fn stats(v: &VideoVeille, m: &Date) -> Vec<Stat> {
    let compte = |n: Option<i64>| n.map(|n| format::compact(n as f64));
    [
        ("Publiée", v.publiee.map(|d| format::il_ya(&d, m))),
        ("Vues", v.vues.map(|n| format::entier(n as f64))),
        ("Likes", compte(v.likes)),
        ("Commentaires", compte(v.commentaires)),
        ("Partages", compte(v.partages)),
        ("Enregistrements", compte(v.enregistrements)),
        ("× médiane", v.ratio().map(|r| format!("{}×", format::decimal(r, 1)))),
        ("Vues/h", v.vitesse.map(format::compact)),
        ("Durée", v.duree_s.map(format::duree)),
    ].into_iter().filter_map(|(nom, valeur)| Some(Stat { nom, valeur: valeur? })).collect()
}

fn detail(v: &VideoVeille, m: &Date) -> DetailVeille {
    let meta = [v.mot.as_ref().map(|w| format!("« {w} »")), v.publiee.map(|d| format::age(&d, m)), v.duree_s.map(format::duree)]
        .into_iter().flatten().collect::<Vec<_>>().join(" · ");
    let entier = |n: Option<i64>| n.map(|n| format::entier(n as f64));
    DetailVeille {
        id: v.id.clone(),
        plateforme: v.plateforme,
        nom_plateforme: v.plateforme.nom(),
        cles: miniatures::cles(&v.id, v.plateforme),
        url: v.url.clone(),
        compte: format!("@{}", v.compte),
        meta,
        legende: v.legende.clone(),
        mesures: vec![
            LigneMesure { nom: "Vues", valeur: entier(v.vues), succes: false },
            LigneMesure { nom: "Médiane du compte", valeur: entier(v.mediane), succes: false },
            LigneMesure { nom: "× médiane", valeur: v.ratio().map(|r| format!("{}×", format::decimal(r, 1))), succes: v.outlier() },
            LigneMesure { nom: "Pull", valeur: v.pull().map(|p| format::pourcent(p, 1)), succes: false },
            LigneMesure { nom: "Vues/h", valeur: v.vitesse.map(format::compact), succes: false },
            LigneMesure { nom: "Abonnés", valeur: v.abonnes.map(|n| format::compact(n as f64)), succes: false },
        ],
        analyse: v.analyse.as_ref().map(analyse),
        attente: if v.a_analyser() { "Analyse au prochain passage (clé Anthropic du business)." } else { "Analysée à partir de 2× la médiane du compte." },
    }
}

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
            section("Confirmation", &a.blocs.confirmation),
            section("Valeur", &a.blocs.valeur),
            section("Placement", &a.blocs.placement),
            section("Fin", &a.blocs.fin),
            section("Ce qui change", &a.ecart),
            Section { titre: "À tourner", texte: a.idee.clone(), fort: true },
        ],
    }
}
