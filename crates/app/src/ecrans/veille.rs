//! Veille : les vidéos qui montent sur les mots-clés suivis, triées et filtrées, leur carte au survol et leur fiche
//! au clic, avec l'analyse de Claude et le lecteur (docs/ecrans.md § Veille).
use crate::commandes::{texte, Rep};
use crate::ecrans::lectures;
use crate::Etat;
use noyau::collecte::miniatures;
use noyau::jour::Date;
use noyau::lecture::format;
use noyau::modeles::Plateforme;
use noyau::modeles_veille::{AnalyseVeille, DemandeVeille, MotVeille, ScoreVeille, VideoVeille};
use noyau::p;
use noyau::sources::veille::{analyse, demande, videos_par_mot, ID, PLATEFORMES, REGLAGE_VIDEOS, VIDEOS_MAX, VIDEOS_MIN, VIDEOS_PAS};
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
    curseur: Curseur,
    /// La dernière demande à Claude du business, au-dessus de la grille.
    demande: Option<DemandeEcran>,
    prompt_max: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandeEcran {
    prompt: String,
    /// en_cours, finie ou echouee.
    etat: &'static str,
    /// « il y a 3 min, 0,04 $ ».
    meta: String,
    journal: Vec<String>,
    synthese: Option<String>,
    erreur: Option<String>,
    retenues: Vec<Retenue>,
}

#[derive(Serialize)]
pub struct Retenue {
    case: CaseVeille,
    raison: String,
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
    /// « Top 6 % de la bibliothèque ».
    rang: Option<String>,
    chiffres: Vec<Chiffre>,
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

/// Les vidéos proches montrées sous une fiche.
const SIMILAIRES: usize = 8;

#[tauri::command]
pub async fn veille(
    etat: State<'_, Etat>,
    business: Option<String>,
    tri: Tri,
    plateforme: Option<Plateforme>,
    outliers: bool,
    mot: Option<String>,
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
    let demande = l.demande_veille().map_err(texte)?;
    let retenues = match &demande {
        Some(d) => l.retenues_veille(&d.id).map_err(texte)?,
        None => vec![],
    };
    Ok(EcranVeille {
        sous_titre: sous_titre(&mots, &toutes, &m),
        mots: mots.iter().map(|w| pilule(w, &toutes, &m)).collect(),
        plateformes: PLATEFORMES.iter().map(|p| Choix { id: *p, libelle: p.nom() }).collect(),
        plateforme,
        mot,
        videos: liste.iter().map(|v| case(v, tri, &m)).collect(),
        sans_mot: mots.is_empty(),
        curseur: Curseur { min: VIDEOS_MIN, max: VIDEOS_MAX, pas: VIDEOS_PAS, valeur: videos_par_mot(&etat.store).map_err(texte)? },
        demande: match demande {
            Some(d) => {
                let cout = l.cout_demande(&d).map_err(texte)?;
                let retenues = retenues.iter().map(|(v, raison)| Retenue { case: case(v, tri, &m), raison: raison.clone() }).collect();
                Some(demande_ecran(d, etat.collecteur.en_cours().contains(ID), cout, retenues, &m))
            }
            None => None,
        },
        prompt_max: demande::PROMPT_MAX,
    })
}

/// Demande à Claude des vidéos en fond : Sonnet choisit les mots-clés, Haiku cherche, analyse et retient. Elle
/// réserve la veille, comme un passage.
#[tauri::command]
pub async fn demander_veille(etat: State<'_, Etat>, business: String, prompt: String) -> Rep<()> {
    let reservation = etat.collecteur.reserver(ID).ok_or("La veille tourne déjà, réessaie quand elle a fini.")?;
    let ctx = etat.collecteur.contexte().map_err(texte)?;
    let d = demande::creer(&etat.store, &business, &prompt).map_err(texte)?;
    tauri::async_runtime::spawn(async move {
        demande::mener(&ctx, &d).await;
        drop(reservation);
    });
    Ok(())
}

/// Une demande en cours que plus rien ne mène (app quittée en route) se lit interrompue.
fn demande_ecran(d: DemandeVeille, menee: bool, cout: f64, retenues: Vec<Retenue>, m: &Date) -> DemandeEcran {
    let perdue = d.etat == DemandeVeille::EN_COURS && !menee;
    DemandeEcran {
        etat: match d.etat.as_str() {
            DemandeVeille::EN_COURS if !perdue => DemandeVeille::EN_COURS,
            DemandeVeille::FINIE => DemandeVeille::FINIE,
            _ => DemandeVeille::ECHOUEE,
        },
        meta: format!("{}, {} $", format::majuscule(&format::il_ya(&d.debut, m)), format::decimal(cout, 2)),
        erreur: if perdue { Some("interrompue".into()) } else { d.erreur },
        prompt: d.prompt,
        journal: d.journal,
        synthese: d.synthese,
        retenues,
    }
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

/// La fiche d'une vidéo, de la grille ou retenue par une demande (même plus vieille que 30 j).
#[tauri::command]
pub async fn fiche_veille(etat: State<'_, Etat>, business: Option<String>, id: String) -> Rep<FicheVeille> {
    let l = lectures(&etat.store, business.as_deref())?;
    let v = etat.store.videos_veille("where v.id = ?", p![id]).map_err(texte)?.into_iter().next()
        .ok_or("vidéo absente de la veille")?;
    let toutes = l.videos_veille(JOURS).map_err(texte)?;
    Ok(fiche(&v, &toutes, &l.maintenant))
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
        Tri::Ratio | Tri::Vues => v.ratio().map(fois),
        Tri::Vitesse => v.vitesse.map(|x| format!("{}/h", format::compact(x))),
        Tri::Recentes => v.publiee.map(|d| format::age(&d, m)),
    }
}

/// « 2,5× », « 471× ».
fn fois(r: f64) -> String {
    format!("{}×", format::decimal(r, if r < 10.0 { 1 } else { 0 }))
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
        carte: Carte {
            compte: format!("@{}", v.compte),
            meta: meta(v, m),
            viralite: viralite(v),
            chiffres: chiffres(&[
                ("Vues", v.vues.map(|n| format::compact(n as f64))),
                ("Likes", v.likes.map(|n| format::compact(n as f64))),
                ("Engagement", v.engagement().map(|e| format::pourcent(e, 1))),
            ]),
            anatomie: v.analyse.as_ref().map(anatomie).unwrap_or_default(),
            attente: attente(v),
        },
    }
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

fn fiche(v: &VideoVeille, toutes: &[VideoVeille], m: &Date) -> FicheVeille {
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
        rang: rang(v, toutes),
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
        anatomie: v.analyse.as_ref().map(anatomie).unwrap_or_default(),
        analyse: v.analyse.as_ref().map(analyse),
        attente: attente(v),
        similaires: similaires(v, toutes, m),
    }
}

/// La place de l'engagement de `v` parmi les vidéos de la veille qui en ont un ; la moitié basse ne se classe pas.
fn rang(v: &VideoVeille, toutes: &[VideoVeille]) -> Option<String> {
    let e = v.engagement()?;
    let autres: Vec<f64> = toutes.iter().filter(|x| x.id != v.id).filter_map(VideoVeille::engagement).collect();
    if autres.is_empty() {
        return None;
    }
    let devant = autres.iter().filter(|x| **x > e).count();
    let top = ((devant + 1) as f64 * 100.0 / (autres.len() + 1) as f64).ceil() as i64;
    Some(if top <= 50 { format!("Top {top} % de la bibliothèque") } else { "Moitié basse de la bibliothèque".into() })
}

/// Même mot-clé ou même format d'analyse, les plus fortes d'abord.
fn similaires(v: &VideoVeille, toutes: &[VideoVeille], m: &Date) -> Vec<CaseVeille> {
    let format = |x: &VideoVeille| x.analyse.as_ref().map(|a| a.format.clone());
    let mut proches: Vec<&VideoVeille> = toutes.iter()
        .filter(|x| x.id != v.id)
        .filter(|x| (v.mot.is_some() && x.mot == v.mot) || (format(v).is_some() && format(x) == format(v)))
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
            releve_le: None, mediane: None, abonnes: None, vitesse: None, analyse: None,
        }
    }

    #[test]
    fn rang_dans_la_bibliotheque() {
        let toutes: Vec<_> = (1..=19).map(|i| video(&i.to_string(), 1000, i)).collect();
        assert_eq!(rang(&video("x", 1000, 100), &toutes).as_deref(), Some("Top 5 % de la bibliothèque"));
        assert_eq!(rang(&video("x", 1000, 0), &toutes).as_deref(), Some("Moitié basse de la bibliothèque"));
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
}
