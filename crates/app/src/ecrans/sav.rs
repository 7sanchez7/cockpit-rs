//! L'écran SAV : les fils avec les users et la réponse, les retours, les autres tâches, l'ajout à la main et les faites.
//! Répondre, fermer un fil et traiter un retour écrivent dans le Firestore du business (seul écran qui le fait).
use super::sante::etat_source;
use crate::commandes::{texte, Rep};
use crate::Etat;
use noyau::jour;
use noyau::lecture::format as F;
use noyau::lecture::{Lectures, LigneTache};
use noyau::modeles::{CleFil, EtatFil, FilSAV, MessageSAV, RetourSAV, Ton};
use noyau::sources::firebase::{self, sav as action};
use serde::Serialize;
use tauri::State;

/// Les fils et les retours lus au plus.
const LIMITE: i64 = 100;
/// Les tâches faites montrées, en jours.
const JOURS_FAITES: i64 = 7;
/// Les sources qui écrivent dans `a_traiter` autre chose que le SAV (docs/donnees.md).
const POSEUSES: [&str; 2] = ["asc", "contenu"];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sav {
    sous_titre: String,
    outils: Outils,
    fils: Vec<Fil>,
    /// Le fil ouvert : celui demandé, sinon le premier.
    choisi: Option<CleFil>,
    conversation: Option<Conversation>,
    retours: Vec<Retour>,
    /// Les tâches ouvertes autres que le SAV, par genre ; vide, `rien` dit quand leurs sources ont lu.
    groupes: Vec<Groupe>,
    rien: Vec<SourceLue>,
    /// Le business d'une tâche ajoutée : celui affiché s'il est seul.
    ajout: Ajout,
    faites: Vec<Tache>,
    longueur_reponse: usize,
}

/// Une pastille : son texte et sa couleur (nom de variable de theme.css).
#[derive(Serialize)]
pub struct Etiquette {
    pub texte: String,
    pub couleur: &'static str,
}

/// Quand Firebase a lu le SAV pour la dernière fois.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outils {
    couleur: &'static str,
    texte: String,
    aide: Option<String>,
    en_cours: bool,
}

#[derive(Serialize)]
pub struct Fil {
    cle: CleFil,
    nom: String,
    quand: String,
    apercu: String,
    etat: Etiquette,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    cle: CleFil,
    nom: String,
    /// L'email et l'uid.
    ident: String,
    etat: Option<Etiquette>,
    ouvert: bool,
    messages: Vec<Bulle>,
}

#[derive(Serialize)]
pub struct Bulle {
    equipe: bool,
    texte: String,
    quand: String,
}

#[derive(Serialize)]
pub struct Retour {
    business: String,
    id: String,
    /// Le fil de l'auteur, quand il a un compte.
    fil: Option<CleFil>,
    source: &'static str,
    categorie: Option<Etiquette>,
    etoiles: Option<Etiquette>,
    quand: String,
    citation: Option<String>,
    message: String,
    /// Le compte, l'appareil, quand il a été traité.
    pied: String,
    traite: bool,
}

#[derive(Serialize)]
pub struct Groupe {
    titre: &'static str,
    compte: Etiquette,
    taches: Vec<Tache>,
}

#[derive(Serialize)]
pub struct Tache {
    id: String,
    texte: String,
    detail: Option<String>,
    faite: bool,
    alerte: bool,
    pastille: Option<Etiquette>,
    lien: Option<String>,
    supprimable: bool,
}

#[derive(Serialize)]
pub struct SourceLue {
    nom: &'static str,
    couleur: &'static str,
    lu: String,
}

#[derive(Serialize)]
pub struct Ajout {
    business: Option<String>,
    texte: String,
    /// L'échéance proposée : aujourd'hui, en clé de jour.
    jour: String,
}

/// L'écran SAV ; `fil` est le fil ouvert, `traites` filtre les retours (None = tous).
#[tauri::command]
pub async fn sav(etat: State<'_, Etat>, business: Option<String>, fil: Option<CleFil>, traites: Option<bool>) -> Rep<Sav> {
    let l = crate::ecrans::lectures(&etat.store, business.as_deref())?;
    let maintenant = l.maintenant;
    let aujourdhui = jour::cle(&maintenant);
    let en_cours = etat.collecteur.en_cours();
    let passages = l.derniers_passages().map_err(texte)?;
    let lu = |id: &str, jamais: &str| passages.get(id).map(|p| format!("lu {}", F::il_ya(&p.fin, &maintenant))).unwrap_or_else(|| jamais.into());

    let fils = l.fils_sav(LIMITE).map_err(texte)?;
    let choisi = fil.or_else(|| fils.first().map(FilSAV::cle));
    let conversation = choisi.as_ref().map(|c| conversation(&l, c)).transpose()?;

    // le nom du business d'une tâche, quand « Tous » en montre plusieurs
    let tous = etat.store.businesses().map_err(texte)?;
    let nom_business = |t: &LigneTache| {
        (business.is_none() && tous.len() > 1)
            .then(|| tous.iter().find(|b| Some(&b.id) == t.business.as_ref()).map(|b| b.nom.clone()))
            .flatten()
    };
    let tache = |t: &LigneTache| ligne_tache(t, &aujourdhui, &maintenant, nom_business(t));

    let groupes = ranger(l.taches().map_err(texte)?, &aujourdhui).into_iter().map(|(g, ts)| Groupe {
        titre: g.titre(),
        compte: Etiquette {
            texte: F::entier(ts.len() as f64),
            couleur: if ts.iter().any(|t| urgente(t, &aujourdhui)) { "ambre" } else { "secondaire" },
        },
        taches: ts.iter().map(tache).collect(),
    }).collect::<Vec<_>>();
    let rien = if groupes.is_empty() {
        etat.collecteur.sources.iter().filter(|s| POSEUSES.contains(&s.id())).map(|s| SourceLue {
            nom: s.nom(),
            couleur: etat_source(passages.get(s.id()), en_cours.contains(s.id())).couleur,
            lu: lu(s.id(), "jamais lue"),
        }).collect()
    } else {
        vec![]
    };
    let seul = (l.businesses.len() == 1).then(|| &l.businesses[0]);
    let p = passages.get(firebase::ID_SOURCE);

    Ok(Sav {
        sous_titre: sous_titre(&l)?,
        outils: Outils {
            couleur: etat_source(p, en_cours.contains(firebase::ID_SOURCE)).couleur,
            texte: lu(firebase::ID_SOURCE, "jamais lu"),
            aide: p.and_then(|p| p.erreur.clone()),
            en_cours: en_cours.contains(firebase::ID_SOURCE),
        },
        fils: fils.iter().map(|f| Fil {
            cle: f.cle(),
            nom: nom_compte(f.nom.as_deref(), f.email.as_deref()),
            quand: F::il_ya(&f.dernier_at, &maintenant),
            apercu: format!("{}{}", if f.dernier_equipe { "Équipe : " } else { "" }, f.apercu),
            etat: etat_fil(f.etat()),
        }).collect(),
        choisi,
        conversation,
        retours: l.retours_sav(traites, LIMITE).map_err(texte)?.iter().map(|r| retour(r, &maintenant)).collect(),
        groupes,
        rien,
        ajout: Ajout {
            business: seul.map(|b| b.id.clone()),
            texte: seul.map(|b| format!("Pour {}", b.nom)).unwrap_or_else(|| "Sans business".into()),
            jour: aujourdhui.clone(),
        },
        faites: l.taches_faites(JOURS_FAITES).map_err(texte)?.iter().filter(|t| Genre::de(t).is_some()).map(tache).collect(),
        longueur_reponse: action::LONGUEUR_REPONSE,
    })
}

/// Envoie la réponse de l'équipe ; rend ce que l'écran affiche sous le champ.
#[tauri::command]
pub async fn sav_repondre(etat: State<'_, Etat>, texte_reponse: String, cle: CleFil) -> Rep<String> {
    let ctx = etat.collecteur.contexte().map_err(texte)?;
    let r = action::repondre(&ctx, &texte_reponse, &cle).await.map_err(texte)?;
    etat.store.a_change();
    Ok(r)
}

/// Le fil marqué traité sans réponse.
#[tauri::command]
pub async fn sav_fermer(etat: State<'_, Etat>, cle: CleFil) -> Rep<String> {
    let ctx = etat.collecteur.contexte().map_err(texte)?;
    action::fermer(&ctx, &cle).await.map_err(texte)?;
    etat.store.a_change();
    Ok("Marqué traité".into())
}

/// Un retour marqué traité, ou rouvert.
#[tauri::command]
pub async fn sav_traiter(etat: State<'_, Etat>, business: String, id: String, traite: bool) -> Rep<()> {
    let l = crate::ecrans::lectures(&etat.store, Some(&business))?;
    let r = l.retour_sav(&business, &id).map_err(texte)?.ok_or_else(|| format!("retour {id} introuvable"))?;
    let ctx = etat.collecteur.contexte().map_err(texte)?;
    action::traiter(&ctx, &r, traite).await.map_err(texte)?;
    etat.store.a_change();
    Ok(())
}

#[tauri::command]
pub async fn sav_cocher(etat: State<'_, Etat>, id: String, fait: bool) -> Rep<()> {
    etat.store.cocher(&id, fait).map_err(texte)?;
    etat.store.a_change();
    Ok(())
}

/// Une tâche à la main ; `echeance` est une clé de jour (`2026-11-19`).
#[tauri::command]
pub async fn sav_ajouter(etat: State<'_, Etat>, texte_tache: String, business: Option<String>, echeance: Option<String>) -> Rep<()> {
    let t = texte_tache.trim();
    if t.is_empty() {
        return Err("tâche vide".into());
    }
    etat.store.ajouter_tache(t, business.as_deref(), echeance.as_deref()).map_err(texte)?;
    etat.store.a_change();
    Ok(())
}

#[tauri::command]
pub async fn sav_supprimer(etat: State<'_, Etat>, id: String) -> Rep<()> {
    etat.store.supprimer_tache(&id).map_err(texte)?;
    etat.store.a_change();
    Ok(())
}

fn sous_titre(l: &Lectures) -> Rep<String> {
    let n = l.nb_sav().map_err(texte)?;
    let morceaux: Vec<String> = [
        (n.fils > 0).then(|| format!("{} {}", n.fils, if n.fils > 1 { "fils à répondre" } else { "fil à répondre" })),
        (n.retours > 0).then(|| format!("{} {}", n.retours, if n.retours > 1 { "retours à traiter" } else { "retour à traiter" })),
    ].into_iter().flatten().collect();
    Ok(if morceaux.is_empty() { "Rien à répondre".into() } else { morceaux.join(", ") })
}

fn conversation(l: &Lectures, cle: &CleFil) -> Rep<Conversation> {
    let fil = l.fil_sav(cle).map_err(texte)?;
    let email = match &fil {
        Some(f) => f.email.clone(),
        None => l.email_sav(cle).map_err(texte)?,
    };
    let ident = [email.as_deref(), Some(cle.uid.as_str())].into_iter().flatten().filter(|s| !s.is_empty()).collect::<Vec<_>>();
    Ok(Conversation {
        cle: cle.clone(),
        nom: nom_compte(fil.as_ref().and_then(|f| f.nom.as_deref()), email.as_deref()),
        ident: ident.join(" · "),
        etat: fil.as_ref().map(|f| etat_fil(f.etat())),
        ouvert: fil.as_ref().is_some_and(|f| f.ouvert),
        messages: l.messages_sav(cle).map_err(texte)?.iter().map(bulle).collect(),
    })
}

fn bulle(m: &MessageSAV) -> Bulle {
    Bulle {
        equipe: m.equipe,
        texte: m.texte.clone(),
        quand: format!("{}{}", if m.equipe { "Équipe · " } else { "" }, F::jour_heure(&m.at)),
    }
}

fn retour(r: &RetourSAV, maintenant: &jour::Date) -> Retour {
    let souci = matches!(r.categorie.as_deref(), Some("problem" | "bug"));
    let compte = match (&r.email, &r.uid) {
        (Some(e), _) if !e.is_empty() => e.clone(),
        (_, None) => "Anonyme".into(),
        _ => "Invité".into(),
    };
    let plateforme = r.plateforme.as_deref().map(|p| match p {
        "ios" => "iOS".to_string(),
        "android" => "Android".to_string(),
        _ => p.to_string(),
    });
    let appareil = [plateforme, r.build.as_ref().map(|b| format!("build {b}"))].into_iter().flatten().collect::<Vec<_>>().join(" ");
    let traite = r.traite.then(|| r.traite_le.map(|d| format!("traité le {}", F::jour_heure(&d)))).flatten();
    let pied = [Some(compte), (!appareil.is_empty()).then_some(appareil), traite].into_iter().flatten().collect::<Vec<_>>();
    Retour {
        business: r.business.clone(),
        id: r.id.clone(),
        fil: r.uid.as_ref().map(|uid| CleFil { business: r.business.clone(), uid: uid.clone() }),
        source: match r.source.as_deref() {
            Some("profile") => "Profil",
            Some("rating") => "Note",
            Some("chat_report") => "Signalement",
            _ => "Retour",
        },
        categorie: r.categorie.as_deref().map(|c| Etiquette {
            texte: match c {
                "idea" => "Idée",
                "problem" => "Souci",
                "weird" => "Réponse bizarre",
                "language" => "Faute ou mauvaise langue",
                "off_topic" => "Hors sujet",
                "bug" => "Bug de l'app",
                "other" => "Autre",
                autre => autre,
            }.into(),
            couleur: if souci { "ambre" } else { "secondaire" },
        }),
        etoiles: r.etoiles.map(|e| Etiquette { texte: format!("{e}/5"), couleur: if e <= 2 { "ambre" } else { "secondaire" } }),
        quand: F::il_ya(&r.at, maintenant),
        citation: r.citation.clone(),
        message: r.message.clone(),
        pied: pied.join(" · "),
        traite: r.traite,
    }
}

fn etat_fil(e: EtatFil) -> Etiquette {
    let (texte, couleur) = match e {
        EtatFil::ARepondre => ("À répondre", "ambre"),
        EtatFil::PasLu => ("Pas encore lu", "accent"),
        EtatFil::Lu => ("Lu", "succes"),
        EtatFil::Traite => ("Traité", "secondaire"),
    };
    Etiquette { texte: texte.into(), couleur }
}

/// Le nom affiché d'un compte : son prénom, sinon son email, sinon invité.
fn nom_compte(nom: Option<&str>, email: Option<&str>) -> String {
    nom.or(email.filter(|e| !e.is_empty())).unwrap_or("Invité").into()
}

fn ligne_tache(t: &LigneTache, aujourdhui: &str, maintenant: &jour::Date, business: Option<String>) -> Tache {
    let faite = t.fait_le.is_some();
    let detail = [business, t.fait_le.map(|d| format!("fait {}", F::il_ya(&d, maintenant)))].into_iter().flatten().collect::<Vec<_>>();
    Tache {
        id: t.id.clone(),
        texte: t.texte.clone(),
        detail: (!detail.is_empty()).then(|| detail.join(", ")),
        faite,
        alerte: t.ton == Ton::Alerte,
        pastille: if faite { None } else {
            t.pastille(aujourdhui).map(|p| Etiquette {
                couleur: if t.ton == Ton::Alerte || p == "retard" { "ambre" } else { "secondaire" },
                texte: p,
            })
        },
        lien: t.lien.clone(),
        supprimable: t.source == "manuel",
    }
}

/// En alerte, ou échéance passée.
fn urgente(t: &LigneTache, aujourdhui: &str) -> bool {
    t.ton == Ton::Alerte || t.echeance.as_deref().is_some_and(|e| e < aujourdhui)
}

/// Le genre d'une tâche, lu dans son id (`asc:avis:…`, docs/donnees.md), dans l'ordre des panneaux.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Genre { Avis, Manuel, Verdict, Autre }

impl Genre {
    /// None pour un message ou un retour, que les panneaux du SAV montrent déjà.
    fn de(t: &LigneTache) -> Option<Genre> {
        if t.source == "manuel" {
            return Some(Genre::Manuel);
        }
        match t.id.split(':').filter(|s| !s.is_empty()).nth(1) {
            Some("msg" | "retour") => None,
            Some("avis") => Some(Genre::Avis),
            Some("verdict") => Some(Genre::Verdict),
            _ => Some(Genre::Autre),
        }
    }

    fn titre(self) -> &'static str {
        match self {
            Genre::Avis => "Avis App Store",
            Genre::Manuel => "À la main",
            Genre::Verdict => "Verdicts de vidéos",
            Genre::Autre => "Autres",
        }
    }
}

/// Les groupes, ceux qui ont de l'urgent d'abord ; dans un groupe, l'urgent d'abord, l'ordre de `taches()` ensuite.
fn ranger(taches: Vec<LigneTache>, aujourdhui: &str) -> Vec<(Genre, Vec<LigneTache>)> {
    let mut groupes: Vec<(Genre, Vec<LigneTache>)> = vec![];
    for t in taches {
        let Some(g) = Genre::de(&t) else { continue };
        match groupes.iter_mut().find(|(x, _)| *x == g) {
            Some((_, ts)) => ts.push(t),
            None => groupes.push((g, vec![t])),
        }
    }
    for (_, ts) in &mut groupes {
        ts.sort_by_key(|t| !urgente(t, aujourdhui));
    }
    groupes.sort_by_key(|(g, ts)| (!ts.iter().any(|t| urgente(t, aujourdhui)), *g));
    groupes
}
