//! Les commandes de l'écran Réglages : la config des business, les clés du trousseau (jamais lues ni rendues,
//! seulement présentes ou absentes), les notifications, la collecte et les sessions des réseaux.
use crate::commandes::{texte, Rep, SITES};
use crate::Etat;
use noyau::business::{Business, Config};
use noyau::collecte::collecteur::REGLAGE_PAUSE;
use noyau::collecte::tikhub;
use noyau::lecture::format;
use noyau::lecture::recherche::plier;
use noyau::modeles::TypeFil;
use noyau::sources::{app_store_connect, posthog, revenuecat};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

/// La config d'un business en texte, le temps de l'éditer (le formulaire de l'onglet Business).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Brouillon {
    id: String,
    nom: String,
    lettre: String,
    teinte: String,
    app_store_id: String,
    asc_issuer: String,
    asc_vendor: String,
    play_package: String,
    play_bucket: String,
    firebase_project: String,
    posthog_host: String,
    contenu_dir: String,
    pays: String,
    genre: String,
    objectif: String,
}

impl Brouillon {
    fn depuis(b: &Business) -> Brouillon {
        let c = &b.config;
        let t = |o: &Option<String>| o.clone().unwrap_or_default();
        Brouillon {
            id: b.id.clone(),
            nom: b.nom.clone(),
            lettre: b.lettre.clone(),
            teinte: b.teinte.clone(),
            app_store_id: t(&c.app_store_id),
            asc_issuer: t(&c.asc_issuer),
            asc_vendor: t(&c.asc_vendor),
            play_package: t(&c.play_package),
            play_bucket: t(&c.play_bucket),
            firebase_project: t(&c.firebase_project),
            posthog_host: t(&c.posthog_host),
            contenu_dir: t(&c.contenu_dir),
            pays: c.pays.join(", "),
            genre: c.genre.to_string(),
            objectif: (c.objectif_mensuel_eur as i64).to_string(),
        }
    }

    /// Le business à enregistrer par-dessus `avant` (dont il garde les événements PostHog et la clé Claude) ;
    /// un nouveau prend pour id son nom en minuscules sans accents.
    fn business(&self, avant: Option<&Business>) -> Rep<Business> {
        let nom = self.nom.trim().to_string();
        let id = if self.id.is_empty() { identifiant(&nom) } else { self.id.clone() };
        if nom.is_empty() || id.is_empty() {
            return Err("Il faut un nom.".into());
        }
        let genre = self.genre.trim().parse().map_err(|_| "La catégorie App Store est un nombre (6017 = Éducation).")?;
        let objectif = self.objectif.replace([' ', '\u{202F}', '\u{A0}'], "").replace(',', ".").parse()
            .map_err(|_| "L'objectif mensuel est un nombre d'euros.")?;
        let t = |s: &str| Some(s.trim().to_string()).filter(|s| !s.is_empty());
        let base = avant.map(|b| b.config.clone()).unwrap_or_default();
        let config = Config {
            app_store_id: t(&self.app_store_id),
            asc_issuer: t(&self.asc_issuer),
            asc_vendor: t(&self.asc_vendor),
            play_package: t(&self.play_package),
            play_bucket: t(&self.play_bucket),
            firebase_project: t(&self.firebase_project),
            posthog_host: t(&self.posthog_host),
            contenu_dir: t(&self.contenu_dir),
            pays: self.pays.split(',').map(|p| p.trim().to_lowercase()).filter(|p| !p.is_empty()).collect(),
            genre,
            objectif_mensuel_eur: objectif,
            ..base
        };
        let lettre = self.lettre.trim();
        let lettre = if lettre.is_empty() { nom.chars().take(1).collect::<String>().to_uppercase() } else { lettre.chars().take(2).collect() };
        let teinte = if self.teinte.is_empty() { "menthe".to_string() } else { self.teinte.clone() };
        Ok(Business { id, nom, lettre, teinte, config })
    }
}

/// L'id d'un nouveau business : son nom en minuscules, sans accents, lettres et chiffres seulement.
fn identifiant(nom: &str) -> String { plier(nom).chars().filter(|c| c.is_alphanumeric()).collect() }

fn business(etat: &Etat, id: &str) -> Rep<Business> {
    etat.store.businesses().map_err(texte)?.into_iter().find(|b| b.id == id).ok_or_else(|| format!("business inconnu : {id}"))
}

/// Les business en brouillons, dans l'ordre de la barre du haut.
#[tauri::command]
pub async fn reglages_business(etat: State<'_, Etat>) -> Rep<Vec<Brouillon>> {
    Ok(etat.store.businesses().map_err(texte)?.iter().map(Brouillon::depuis).collect())
}

/// Enregistre un brouillon (`nouveau` : il ne doit pas écraser un business existant) et le rend relu.
#[tauri::command]
pub async fn enregistrer_business(etat: State<'_, Etat>, brouillon: Brouillon, nouveau: bool) -> Rep<Brouillon> {
    let tous = etat.store.businesses().map_err(texte)?;
    let avant = if nouveau { None } else { tous.iter().find(|b| b.id == brouillon.id) };
    let b = brouillon.business(avant)?;
    if nouveau && tous.iter().any(|x| x.id == b.id) {
        return Err(format!("Un business porte déjà l'id {}.", b.id));
    }
    etat.store.enregistrer(&b).map_err(texte)?;
    etat.store.a_change();
    Ok(Brouillon::depuis(&b))
}

/// Retire un business du cockpit (jamais le dernier) ; ses chiffres restent dans la base.
#[tauri::command]
pub async fn supprimer_business(etat: State<'_, Etat>, id: String) -> Rep<()> {
    if etat.store.businesses().map_err(texte)?.len() < 2 {
        return Err("Le cockpit garde au moins un business.".into());
    }
    etat.store.supprimer_business(&id).map_err(texte)?;
    if etat.store.reglage("business").map_err(texte)?.as_deref() == Some(id.as_str()) {
        etat.store.regler("business", None).map_err(texte)?;
    }
    etat.store.a_change();
    Ok(())
}

/// Les clés rangées par business dans le trousseau, avec leur libellé.
const CLES: [(&str, &str); 4] = [
    (posthog::CLE_PROJET, "PostHog, id du projet"),
    (posthog::CLE_API, "PostHog, clé personnelle scopée au projet"),
    (revenuecat::NOM_CLE, "RevenueCat, clé secrète v2 en lecture"),
    (tikhub::CLE, "TikHub, clé API (veille TikTok et Instagram)"),
];

/// Le nom du trousseau du système, tel que l'onglet Clés l'écrit.
const COFFRE: &str = if cfg!(target_os = "macos") { "Trousseau" } else if cfg!(windows) { "Gestionnaire d'identifiants" } else { "trousseau" };

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cles {
    coffre: &'static str,
    cles: Vec<Cle>,
    pied: String,
    fichiers: Vec<Fichier>,
}

#[derive(Serialize)]
pub struct Cle {
    nom: &'static str,
    libelle: &'static str,
    presente: bool,
}

#[derive(Serialize)]
pub struct Fichier {
    libelle: &'static str,
    valeur: String,
    present: bool,
}

/// L'onglet Clés d'un business : présentes ou absentes, jamais une valeur.
#[tauri::command]
pub async fn reglages_cles(etat: State<'_, Etat>, business: String) -> Rep<Cles> {
    let b = self::business(&etat, &business)?;
    let secrets = &etat.collecteur.secrets;
    let cles = CLES.iter().map(|&(nom, libelle)| Cle { nom, libelle, presente: secrets.dans_trousseau(nom, &b) }).collect();
    let projet = b.config.firebase_project.as_ref().map(|p| format!(" du projet {p}")).unwrap_or_default();
    let asc = app_store_connect::dossier_cles().and_then(|d| app_store_connect::cle(&d)).is_ok();
    let adc = noyau::chemins::adc().exists();
    Ok(Cles {
        coffre: COFFRE,
        cles,
        pied: format!("Sans clé dans le {COFFRE}, le cockpit lit la variable d'environnement du même nom, puis Secret Manager{projet}."),
        fichiers: vec![
            Fichier {
                libelle: "App Store Connect",
                valeur: if asc { "clé AuthKey trouvée" } else { "aucune clé dans ~/.appstoreconnect/private_keys" }.into(),
                present: asc,
            },
            Fichier {
                libelle: "Google, ADC",
                valeur: if adc { "identifiants présents" } else { "gcloud auth application-default login" }.into(),
                present: adc,
            },
        ],
    })
}

fn nom_cle(nom: &str) -> Rep<&'static str> {
    CLES.iter().map(|(n, _)| *n).find(|n| *n == nom).ok_or_else(|| format!("clé inconnue : {nom}"))
}

/// Range une clé collée dans le trousseau ; une saisie vide ne fait rien.
#[tauri::command]
pub async fn ranger_cle(etat: State<'_, Etat>, business: String, nom: String, valeur: String) -> Rep<()> {
    let valeur = valeur.trim();
    if valeur.is_empty() {
        return Ok(());
    }
    let b = self::business(&etat, &business)?;
    etat.collecteur.secrets.enregistrer(nom_cle(&nom)?, &b, valeur).map_err(texte)
}

#[tauri::command]
pub async fn effacer_cle(etat: State<'_, Etat>, business: String, nom: String) -> Rep<()> {
    let b = self::business(&etat, &business)?;
    etat.collecteur.secrets.effacer(nom_cle(&nom)?, &b).map_err(texte)
}

#[derive(Serialize)]
pub struct Notif {
    cle: String,
    libelle: &'static str,
    actif: bool,
}

/// Un interrupteur par type du fil ; coupé, son réglage vaut "0" (le changer passe par `ecrire_reglage`).
#[tauri::command]
pub async fn reglages_notifs(etat: State<'_, Etat>) -> Rep<Vec<Notif>> {
    let mut r = Vec::new();
    for t in TypeFil::TOUS {
        let cle = t.reglage_notif();
        let actif = etat.store.reglage(&cle).map_err(texte)?.as_deref() != Some("0");
        r.push(Notif { cle, libelle: t.libelle(), actif });
    }
    Ok(r)
}

#[derive(Serialize)]
pub struct Collecte {
    pause: bool,
    sources: Vec<SourceCadence>,
    fichier: String,
    montrer: &'static str,
}

#[derive(Serialize)]
pub struct SourceCadence {
    nom: &'static str,
    cadence: String,
}

/// L'onglet Collecte : la pause, la cadence de chaque source, le fichier de la base.
#[tauri::command]
pub async fn reglages_collecte(etat: State<'_, Etat>) -> Rep<Collecte> {
    let sources = etat.collecteur.sources.iter()
        .map(|s| SourceCadence { nom: s.nom(), cadence: format!("toutes les {}", format::cadence(s.cadence() as f64)) })
        .collect();
    Ok(Collecte {
        pause: etat.store.reglage(REGLAGE_PAUSE).map_err(texte)?.as_deref() == Some("1"),
        sources,
        fichier: noyau::chemins::base().to_string_lossy().into_owned(),
        montrer: if cfg!(target_os = "macos") { "Montrer dans le Finder" } else if cfg!(windows) { "Montrer dans l'Explorateur" } else { "Montrer le dossier" },
    })
}

/// Montre le fichier de la base dans le gestionnaire de fichiers.
#[tauri::command]
pub async fn montrer_base(app: AppHandle) -> Rep<()> { app.opener().reveal_item_in_dir(noyau::chemins::base()).map_err(texte) }

#[derive(Serialize)]
pub struct Session {
    id: &'static str,
    nom: &'static str,
    connecte: bool,
    etat: &'static str,
}

/// Le cookie sans lequel un site n'a pas de session (le même que lisent les sources).
fn requis(site: &str) -> &'static str { if site == "facebook" { "c_user" } else { "sessionid" } }

/// L'onglet Sessions : pour chaque réseau, si le cockpit trouve une session (Safari sur Mac, sinon la fenêtre de
/// connexion de l'app). Ni cookie ni valeur, seulement oui ou non.
#[tauri::command]
pub async fn reglages_sessions(etat: State<'_, Etat>) -> Rep<Vec<Session>> {
    let hote = &etat.collecteur.hote;
    Ok(SITES.iter().map(|&(id, nom, _)| {
        let domaine = format!("{id}.com");
        let (connecte, etat) = if noyau::collecte::safari::cookies(&domaine, requis(id)).is_ok() {
            (true, "session de Safari")
        } else {
            match hote.cookies(&domaine, requis(id)) {
                Ok(_) => (true, "session de l'app"),
                Err(_) => (false, "pas de session"),
            }
        };
        Session { id, nom, connecte, etat }
    }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brouillon() -> Brouillon {
        Brouillon { nom: " Équipe Été ".into(), pays: "FR, es ,, us".into(), genre: "6017".into(), objectif: "12 000,5".into(), ..Default::default() }
    }

    #[test]
    fn nouveau_business_depuis_le_brouillon() {
        let b = brouillon().business(None).unwrap();
        assert_eq!((b.id.as_str(), b.nom.as_str(), b.lettre.as_str(), b.teinte.as_str()), ("equipeete", "Équipe Été", "É", "menthe"));
        assert_eq!(b.config.pays, ["fr", "es", "us"]);
        assert_eq!(b.config.objectif_mensuel_eur, 12_000.5);
        assert_eq!(b.config.app_store_id, None);
        assert_eq!(Brouillon::depuis(&b).objectif, "12000");
    }

    #[test]
    fn garde_les_evenements_et_refuse_le_faux() {
        let mut avant = brouillon().business(None).unwrap();
        avant.config.claude = "AUTRE".into();
        let b = Brouillon { lettre: "Rol".into(), ..Brouillon::depuis(&avant) }.business(Some(&avant)).unwrap();
        assert_eq!((b.lettre.as_str(), b.config.claude.as_str()), ("Ro", "AUTRE"));
        assert_eq!(Brouillon { genre: "x".into(), ..brouillon() }.business(None).unwrap_err(), "La catégorie App Store est un nombre (6017 = Éducation).");
        assert_eq!(Brouillon { objectif: "dix".into(), ..brouillon() }.business(None).unwrap_err(), "L'objectif mensuel est un nombre d'euros.");
        assert_eq!(Brouillon { nom: " ".into(), ..brouillon() }.business(None).unwrap_err(), "Il faut un nom.");
    }
}
