//! Vidéos : les comptes suivis et leurs abonnés, la grille (ou le tableau) des vidéos triée et filtrée, le détail de
//! la vidéo choisie avec ce qui l'a fait percer (docs/ecrans.md § Vidéos).
use crate::commandes::{texte, Rep};
use crate::ecrans::lectures;
use crate::Etat;
use chrono::Duration;
use noyau::collecte::miniatures;
use noyau::jour::{self, Date};
use noyau::lecture::percee::{self, Levier, Mesure};
use noyau::lecture::videos::LigneCompte;
use noyau::lecture::{format, AnalyseDatee, LigneVideo, PointVues, SEUIL_VIDEO};
use noyau::modeles::{Analyse, Compte, Part, Plateforme};
use noyau::sources::pays_fr;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

#[derive(Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tri { Recentes, Vues, Gain, Engagement, Conversion }

/// Au-delà, une liste de parts s'arrête : la traîne ne dit rien.
const PARTS_MAX: usize = 6;
const ABSENT: &str = "Pas encore relevé pour cette vidéo.";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EcranVideos {
    sous_titre: String,
    comptes: Vec<CarteCompte>,
    sans_compte: bool,
    /// Vide quand une seule plateforme est suivie : pas de filtre.
    plateformes: Vec<Choix>,
    /// Les plateformes du formulaire de la carte +.
    suivables: Vec<Choix>,
    plateforme: Option<Plateforme>,
    compte: Option<String>,
    seuil: String,
    videos: Vec<CaseVideo>,
    choisie: Option<String>,
    detail: Option<DetailVideo>,
}

#[derive(Serialize)]
pub struct Choix {
    id: Plateforme,
    libelle: &'static str,
}

/// Un compte suivi : avatar, abonnés, gain sur 7 jours, courbe de 14 jours.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CarteCompte {
    id: String,
    plateforme: Plateforme,
    avatar: String,
    handle: String,
    nom: String,
    actif: bool,
    abonnes: Option<i64>,
    detail: String,
    gain: Option<String>,
    ton: &'static str,
    serie: Vec<Option<f64>>,
    aide: String,
    profil: String,
}

/// Une vidéo de la grille et du tableau.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseVideo {
    id: String,
    plateforme: Plateforme,
    cles: Vec<String>,
    titre: String,
    aide: String,
    vues: String,
    vues_entier: String,
    /// La mesure du tri, en pastille sur la miniature.
    badge: Option<String>,
    age: String,
    gain: String,
    gain_positif: bool,
    part_partages: String,
    part_enreg: String,
    /// None sans part de partages ni d'enregistrements.
    seuil: Option<bool>,
    aide_ligne: String,
}

/// La vidéo choisie : chaque donnée garde sa place d'une vidéo à l'autre.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailVideo {
    id: String,
    titre: String,
    plateforme: Plateforme,
    nom_plateforme: &'static str,
    url: Option<String>,
    fiche: bool,
    meta: String,
    vues: i64,
    gain: Option<Pastille>,
    verdict: Verdict,
    seuil: Option<Pastille>,
    percee: Percee,
    visionnage: Vec<Compteur>,
    note: String,
    engagement: Vec<Compteur>,
    /// TikTok seulement.
    retention: Option<Serie>,
    releves: Serie,
    /// TikTok seulement.
    sept_jours: Option<Serie>,
    /// Les listes de parts de la plateforme, par rangée.
    parts: Vec<Vec<Parts>>,
}

#[derive(Serialize)]
pub struct Pastille {
    texte: String,
    couleur: &'static str,
}

/// « Pas de fiche » en texte, sinon une pastille.
#[derive(Serialize)]
pub struct Verdict {
    texte: &'static str,
    couleur: Option<&'static str>,
}

#[derive(Serialize)]
pub struct Percee {
    titre: &'static str,
    valeur: String,
    explication: String,
    ton: &'static str,
}

#[derive(Serialize)]
pub struct Compteur {
    libelle: String,
    valeur: Option<String>,
    /// Une part au-dessus du seuil.
    passe: bool,
    /// La case du levier, entourée.
    levier: bool,
}

/// Un graphique titré ; sans sa donnée, `absent` prend sa place.
#[derive(Serialize)]
pub struct Serie {
    valeurs: Vec<f64>,
    etiquettes: Vec<String>,
    absent: Option<&'static str>,
}

#[derive(Serialize)]
pub struct Parts {
    titre: &'static str,
    /// Toujours `PARTS_MAX` lignes ; None pour une ligne manquante, gardée invisible.
    lignes: Vec<Option<LignePart>>,
    absent: Option<&'static str>,
}

#[derive(Serialize)]
pub struct LignePart {
    nom: String,
    part: f64,
    valeur: String,
}

#[tauri::command]
pub async fn videos(
    etat: State<'_, Etat>,
    business: Option<String>,
    tri: Tri,
    plateforme: Option<Plateforme>,
    compte: Option<String>,
    choisie: Option<String>,
) -> Rep<EcranVideos> {
    let l = lectures(&etat.store, business.as_deref())?;
    let m = l.maintenant;
    let toutes = l.videos(1000).map_err(texte)?;
    let comptes = l.comptes().map_err(texte)?;
    let plateformes: Vec<Plateforme> = Plateforme::TOUTES.into_iter().filter(|p| comptes.iter().any(|c| c.plateforme == *p)).collect();
    let filtre = plateforme.filter(|p| plateformes.contains(p));
    let compte = compte.filter(|id| comptes.iter().any(|c| &c.id == id));
    let mut liste: Vec<&LigneVideo> = toutes.iter()
        .filter(|v| filtre.is_none_or(|p| v.plateforme == p))
        .filter(|v| compte.as_ref().is_none_or(|id| &Compte::id_de(v.plateforme, &v.compte) == id))
        .collect();
    trier(&mut liste, tri);
    let choisie = choisie.filter(|id| liste.iter().any(|v| &v.id == id)).or_else(|| liste.first().map(|v| v.id.clone()));
    let detail = match liste.iter().find(|v| Some(&v.id) == choisie.as_ref()) {
        Some(v) => {
            let accroches = l.accroches().map_err(texte)?;
            let accroche = accroches.get(&v.id).copied();
            let contexte = Contexte {
                releves: l.releves(&v.id).map_err(texte)?,
                analyse: l.analyse(&v.id).map_err(texte)?,
                levier: percee::levier(v, &toutes, &accroches),
                accroche,
                comparable: percee::compte(v, &toutes).len() >= percee::MINIMUM,
            };
            Some(detail(v, contexte, &m))
        }
        None => None,
    };
    Ok(EcranVideos {
        sous_titre: sous_titre(&toutes, &comptes, l.publications(7, &jour::cle(&m)).map_err(texte)?, &m),
        sans_compte: comptes.is_empty(),
        comptes: comptes.iter().filter(|c| filtre.is_none_or(|p| c.plateforme == p)).map(|c| carte(c, &m)).collect(),
        plateformes: if plateformes.len() > 1 { plateformes.iter().map(|p| Choix { id: *p, libelle: p.nom() }).collect() } else { vec![] },
        suivables: Plateforme::TOUTES.iter().map(|p| Choix { id: *p, libelle: p.nom() }).collect(),
        plateforme: filtre,
        compte,
        seuil: format!("seuil partages et enreg. {}", format::pourcent(SEUIL_VIDEO, 1)),
        videos: liste.iter().map(|v| case(v, tri, &m)).collect(),
        choisie,
        detail,
    })
}

/// Le nom de compte d'une saisie (`@compte` ou lien du profil), None s'il est illisible.
#[tauri::command]
pub fn handle_compte(saisie: String) -> Option<String> { Compte::handle(&saisie) }

/// Suit un compte ; la source de sa plateforme le relève aussitôt.
#[tauri::command]
pub async fn suivre_compte(etat: State<'_, Etat>, business: String, plateforme: Plateforme, saisie: String) -> Rep<()> {
    let handle = Compte::handle(&saisie).ok_or_else(|| format!("nom de compte illisible : {saisie}"))?;
    etat.store.ajouter(&Compte::nouveau(&business, plateforme, &handle)).map_err(texte)?;
    etat.store.a_change();
    etat.collecteur.relancer(Some(plateforme.brut()));
    Ok(())
}

/// Met un compte en pause ou le reprend ; une reprise le relève aussitôt.
#[tauri::command]
pub async fn activer_compte(etat: State<'_, Etat>, id: String, actif: bool) -> Rep<()> {
    etat.store.activer_compte(&id, actif).map_err(texte)?;
    etat.store.a_change();
    if actif {
        let p = id.split(':').next().and_then(Plateforme::depuis).ok_or_else(|| format!("compte inconnu : {id}"))?;
        etat.collecteur.relancer(Some(p.brut()));
    }
    Ok(())
}

#[tauri::command]
pub async fn retirer_compte(etat: State<'_, Etat>, id: String) -> Rep<()> {
    etat.store.retirer_compte(&id).map_err(texte)?;
    etat.store.a_change();
    Ok(())
}

/// Ouvre la fiche de la vidéo dans l'éditeur du système.
#[tauri::command]
pub async fn ouvrir_fiche(app: AppHandle, etat: State<'_, Etat>, video: String) -> Rep<()> {
    let l = lectures(&etat.store, None)?;
    let fiche = l.videos(1000).map_err(texte)?.into_iter().find(|v| v.id == video).and_then(|v| v.fiche)
        .ok_or_else(|| format!("pas de fiche pour {video}"))?;
    app.opener().open_path(fiche, None::<&str>).map_err(texte)
}

/// « 218 vidéos sur 4 comptes, 5 publiées en 7 j, dernier relevé il y a 3 min ».
fn sous_titre(toutes: &[LigneVideo], comptes: &[LigneCompte], publiees: Vec<f64>, m: &Date) -> String {
    if comptes.is_empty() {
        return "Aucun compte suivi".into();
    }
    let releve = toutes.iter().map(|v| v.releve_le).max()
        .map_or("aucun relevé".to_string(), |d| format!("dernier relevé {}", format::il_ya(&d, m)));
    [
        format!("{} vidéos sur {} comptes", format::entier(toutes.len() as f64), comptes.len()),
        format!("{} publiées en 7 j", format::entier(publiees.iter().sum())),
        releve,
    ].join(", ")
}

/// Les vidéos sans la mesure du tri passent après les autres.
fn trier(v: &mut [&LigneVideo], tri: Tri) {
    let cle = |v: &LigneVideo| -> Option<f64> {
        match tri {
            Tri::Recentes => v.publiee_le.map(|d| d.timestamp_millis() as f64),
            Tri::Vues => Some(v.vues as f64),
            Tri::Gain => v.gain24h.map(|g| g as f64),
            Tri::Engagement => v.engagement(),
            Tri::Conversion => v.conversion(),
        }
    };
    v.sort_by(|a, b| cle(b).unwrap_or(f64::NEG_INFINITY).total_cmp(&cle(a).unwrap_or(f64::NEG_INFINITY)));
}

/// La mesure d'une vidéo pour le tri choisi, dite en pastille sur sa miniature.
fn badge(v: &LigneVideo, tri: Tri, m: &Date) -> Option<String> {
    match tri {
        Tri::Recentes => v.publiee_le.map(|d| format::age(&d, m)),
        Tri::Vues => None,
        Tri::Gain => v.gain24h.map(|g| format::delta(g as f64, true)),
        Tri::Engagement => v.engagement().map(|e| format::pourcent(e, 1)),
        Tri::Conversion => v.conversion().map(|c| format!("{} / 1k", format::decimal(c, 1))),
    }
}

fn ton(x: Option<i64>) -> &'static str {
    match x {
        Some(g) if g > 0 => "positif",
        Some(g) if g < 0 => "negatif",
        _ => "neutre",
    }
}

fn carte(c: &LigneCompte, m: &Date) -> CarteCompte {
    CarteCompte {
        id: c.id.clone(),
        plateforme: c.plateforme,
        avatar: miniatures::avatar(&c.id),
        handle: format!("@{}", c.handle),
        nom: if c.actif { c.plateforme.nom().into() } else { format!("{}, en pause", c.plateforme.nom()) },
        actif: c.actif,
        abonnes: c.abonnes,
        detail: format!("abonnés, {} vidéos", format::entier(c.videos as f64)),
        gain: c.gain7j.map(|g| format!("{} en 7 j", format::delta(g as f64, true))),
        ton: ton(c.gain7j),
        serie: c.serie.clone(),
        aide: c.releve_le.map_or("Pas encore relevé".into(), |d| format!("Relevé {}", format::il_ya(&d, m))),
        profil: c.plateforme.profil(&c.handle),
    }
}

fn titre(v: &LigneVideo) -> String { if v.titre.is_empty() { v.id.clone() } else { v.titre.clone() } }

fn case(v: &LigneVideo, tri: Tri, m: &Date) -> CaseVideo {
    let pourcent = |p: Option<f64>| p.map_or(format::VIDE.into(), |p| format::pourcent(p, 1));
    let (pp, pe) = (v.part_partages(), v.part_enreg());
    CaseVideo {
        id: v.id.clone(),
        plateforme: v.plateforme,
        cles: miniatures::cles(&v.id, v.plateforme),
        titre: titre(v),
        aide: v.titre.clone(),
        vues: format::compact(v.vues as f64),
        vues_entier: format::entier(v.vues as f64),
        badge: badge(v, tri, m),
        age: v.publiee_le.map_or(format::VIDE.into(), |d| format::age(&d, m)),
        gain: v.gain24h.map_or(format::VIDE.into(), |g| format::delta(g as f64, true)),
        gain_positif: v.gain24h.unwrap_or(0) > 0,
        part_partages: pourcent(pp),
        part_enreg: pourcent(pe),
        seuil: (pp.is_some() || pe.is_some()).then(|| v.seuil_atteint()),
        aide_ligne: format!("@{} · relevé {}", v.compte, format::il_ya(&v.releve_le, m)),
    }
}

/// Ce que le détail lit en plus de la vidéo.
struct Contexte {
    releves: Vec<PointVues>,
    analyse: Option<AnalyseDatee>,
    levier: Option<Levier>,
    /// La part encore là à `percee::ACCROCHE_S` secondes.
    accroche: Option<f64>,
    /// Le compte a assez de vidéos vues pour chercher un levier.
    comparable: bool,
}

fn detail(v: &LigneVideo, c: Contexte, m: &Date) -> DetailVideo {
    let vide = Analyse::default();
    let a = c.analyse.as_ref().map_or(&vide, |d| &d.analyse);
    let publiee = v.publiee_le.map(|d| format!("{}, {}", format::jour_court(&d), format::age(&d, m)));
    let meta = [Some(v.plateforme.nom().to_string()), Some(format!("@{}", v.compte)), publiee, v.duree_s.map(format::duree), v.format.clone()]
        .into_iter().flatten().collect::<Vec<_>>().join(" · ");
    let verdict = if v.fiche.is_none() {
        Verdict { texte: "Pas de fiche", couleur: None }
    } else if v.verdict {
        Verdict { texte: "verdict écrit", couleur: Some("accent") }
    } else {
        // la source Contenu ouvre la tâche de verdict 7 jours après la sortie
        let en_retard = v.publiee_le.unwrap_or(*m) < *m - Duration::days(7);
        Verdict { texte: "verdict à écrire", couleur: Some(if en_retard { "ambre" } else { "secondaire" }) }
    };
    let releves = &c.releves;
    let duree = match (releves.first(), releves.last()) {
        (Some(p), Some(d)) => (d.at - p.at).num_seconds(),
        _ => 0,
    };
    let date = |d: &Date| if duree < 36 * 3600 { format::heure(d) } else { format::jour_court(d) };
    let parts = match v.plateforme {
        Plateforme::Tiktok => vec![
            vec![parts("Sources de trafic", &a.sources, false), parts("Recherches", &a.recherches, false)],
            vec![parts("Âges", &a.ages, false), parts("Pays", &a.pays, true)],
        ],
        Plateforme::Facebook => vec![vec![parts("Jalons de visionnage", &a.jalons, false)]],
        Plateforme::Youtube | Plateforme::Instagram => vec![],
    };
    DetailVideo {
        id: v.id.clone(),
        titre: titre(v),
        plateforme: v.plateforme,
        nom_plateforme: v.plateforme.nom(),
        url: v.url.clone(),
        fiche: v.fiche.is_some(),
        meta,
        vues: v.vues,
        gain: v.gain24h.map(|g| Pastille {
            texte: format!("{} en 24 h", format::delta(g as f64, true)),
            couleur: if g > 0 { "succes" } else { "secondaire" },
        }),
        verdict,
        seuil: (v.part_partages().is_some() || v.part_enreg().is_some()).then(|| {
            if v.seuil_atteint() { Pastille { texte: "seuil atteint".into(), couleur: "succes" } } else { Pastille { texte: "sous le seuil".into(), couleur: "secondaire" } }
        }),
        percee: percee(v, &c),
        visionnage: visionnage(v, a, &c),
        note: note(v, &c, m),
        engagement: engagement(v, &c),
        retention: (v.plateforme == Plateforme::Tiktok).then(|| Serie {
            valeurs: a.retention.clone(),
            etiquettes: (0..a.retention.len()).map(|s| format!("{s} s")).collect(),
            absent: (a.retention.len() <= 1).then_some(ABSENT),
        }),
        releves: Serie {
            valeurs: releves.iter().map(|p| p.vues as f64).collect(),
            etiquettes: releves.iter().map(|p| date(&p.at)).collect(),
            absent: (releves.len() <= 1).then_some("Un seul relevé : la courbe se dessine au suivant."),
        },
        sept_jours: (v.plateforme == Plateforme::Tiktok).then(|| Serie {
            valeurs: a.vues_par_jour.iter().map(|&n| n as f64).collect(),
            etiquettes: (1..=a.vues_par_jour.len()).map(|j| format!("J{j}")).collect(),
            absent: a.vues_par_jour.is_empty().then_some(ABSENT),
        }),
        parts,
    }
}

/// La mesure qui a fait percer la vidéo, ou son meilleur atout quand elle est sous la médiane de son compte.
fn percee(v: &LigneVideo, c: &Contexte) -> Percee {
    match c.levier {
        Some(l) => Percee {
            titre: if l.percee { "Ce qui l'a fait percer" } else { "Sous la médiane des vues, son meilleur atout" },
            valeur: format!("{} : {}", l.mesure.nom(), l.mesure.texte(l.valeur)),
            explication: format!(
                "Mieux que {} des vidéos de @{} (médiane {}) ; sur ce compte, cette mesure suit les vues à {}.",
                format::pourcent(l.rang, 0), v.compte, l.mesure.court(l.mediane), format::decimal(l.poids, 2),
            ),
            ton: if l.percee { "positif" } else { "negatif" },
        },
        None => Percee {
            titre: "Ce qui l'a fait percer",
            valeur: format::VIDE.into(),
            explication: if c.comparable {
                format!("Aucune mesure liée aux vues de @{} n'est au-dessus de sa médiane.", v.compte)
            } else {
                format!("Moins de {} vidéos vues sur @{} : rien à comparer.", percee::MINIMUM, v.compte)
            },
            ton: "neutre",
        },
    }
}

fn compteur(c: &Contexte, libelle: &str, valeur: Option<String>, passe: bool, mesure: Option<Mesure>) -> Compteur {
    Compteur { libelle: libelle.into(), valeur, passe, levier: mesure.is_some() && mesure == c.levier.map(|l| l.mesure) }
}

/// Le watch time des stats privées : la plateforme ne le donne qu'au propriétaire du compte.
fn visionnage(v: &LigneVideo, a: &Analyse, c: &Contexte) -> Vec<Compteur> {
    vec![
        compteur(c, "Visionnage moyen", v.watch_moyen_s.map(|s| {
            format::duree(s) + &v.duree_s.map_or(String::new(), |d| format!(" sur {}", format::duree(d)))
        }), false, Some(Mesure::Visionnage)),
        compteur(c, &format!("Encore là à {} s", percee::ACCROCHE_S), c.accroche.map(|x| format::pourcent(x, 0)), false, Some(Mesure::Accroche)),
        compteur(c, "Complétion", v.completion.map(|x| format::pourcent(x, 1)), false, Some(Mesure::Completion)),
        compteur(c, "Temps total", a.watch_total_s.map(format::cumul), false, None),
        compteur(c, "Spectateurs", a.spectateurs.map(|n| format::entier(n as f64)), false, None),
        compteur(c, "Nouveaux spectateurs", a.nouveaux.map(|x| format::pourcent(x, 0)), false, None),
        compteur(c, "Vues d'abonnés", a.abonnes_part.map(|x| format::pourcent(x, 1)), false, None),
        compteur(c, "Abonnés gagnés", v.abonnes.map(|n| format::entier(n as f64)), false, None),
        compteur(c, "Pour 1 000 vues", v.conversion().map(|x| format!("{} abonnés", format::decimal(x, 1))), false, Some(Mesure::Abonnes)),
    ]
}

/// D'où viennent les stats privées, ou pourquoi elles manquent.
fn note(v: &LigneVideo, c: &Contexte, m: &Date) -> String {
    if let Some(d) = &c.analyse {
        return format!("Stats privées relevées {}.", format::il_ya(&d.at, m));
    }
    if v.watch_moyen_s.is_some() {
        return "Analyse détaillée pas encore relevée : elle arrive avec le relevé du jour.".into();
    }
    match v.plateforme {
        Plateforme::Tiktok => "Pas de watch time : seul le compte TikTok connecté dans Safari a ses stats privées.",
        Plateforme::Youtube => "Pas de watch time : il passe par l'API YouTube Analytics, pas encore branchée.",
        Plateforme::Facebook => "Pas de watch time : la Page n'est pas dans le Business Suite de la session Safari.",
        Plateforme::Instagram => "Pas de watch time : les stats privées d'Instagram ne sont pas encore branchées.",
    }.into()
}

/// Les compteurs publics avec leur part des vues ; `passe` met en avant une part au-dessus du seuil.
fn engagement(v: &LigneVideo, c: &Contexte) -> Vec<Compteur> {
    let part = |n: Option<i64>, m: Mesure| {
        n.map(|n| format::entier(n as f64) + &m.valeur(v, None).map_or(String::new(), |x| format!(" · {}", m.court(x))))
    };
    vec![
        compteur(c, "Taux d'engagement", v.engagement().map(|e| format::pourcent(e, 1)), false, None),
        compteur(c, "Likes", part(v.likes, Mesure::Likes), false, Some(Mesure::Likes)),
        compteur(c, "Commentaires", part(v.commentaires, Mesure::Commentaires), false, Some(Mesure::Commentaires)),
        compteur(c, "Partages", part(v.partages, Mesure::Partages), v.part_partages().unwrap_or(0.0) >= SEUIL_VIDEO, Some(Mesure::Partages)),
        compteur(c, "Enregistrements", part(v.enregistrements, Mesure::Enregistrements), v.part_enreg().unwrap_or(0.0) >= SEUIL_VIDEO, Some(Mesure::Enregistrements)),
    ]
}

/// Les `PARTS_MAX` plus grosses parts ; `pays` traduit les codes de pays.
fn parts(titre: &'static str, liste: &[Part], pays: bool) -> Parts {
    let mut tries: Vec<&Part> = liste.iter().collect();
    tries.sort_by(|a, b| b.part.total_cmp(&a.part));
    tries.truncate(PARTS_MAX);
    Parts {
        titre,
        lignes: (0..PARTS_MAX).map(|i| tries.get(i).map(|p| LignePart {
            nom: if pays { pays_fr::nom(&p.nom).unwrap_or(&p.nom).to_string() } else { p.nom.clone() },
            part: p.part.clamp(0.0, 1.0),
            valeur: format::pourcent(p.part, if p.part < 0.1 { 1 } else { 0 }),
        })).collect(),
        absent: tries.is_empty().then_some(ABSENT),
    }
}
