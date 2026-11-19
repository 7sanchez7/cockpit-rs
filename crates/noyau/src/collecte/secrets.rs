use super::gcloud::GCloud;
use super::http::{Http, Requete};
use crate::business::Business;
use crate::erreur::{Erreur, Res};
use base64::Engine;
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Le service des clés dans le trousseau du système (Trousseau macOS, Gestionnaire d'identification Windows,
/// Secret Service sous Linux).
pub const SERVICE: &str = "cockpit";

/// Les clés des sources. Ordre : variable d'environnement du même nom, trousseau du système
/// (service `cockpit`, compte `<business>.<NOM>`), Secret Manager du projet Firebase du business.
/// Une valeur n'est jamais écrite sur le disque, dans la base ni dans un log.
pub struct Secrets {
    gcloud: Arc<GCloud>,
    http: Http,
    cache: Mutex<HashMap<String, String>>,
}

impl Secrets {
    pub fn new(gcloud: Arc<GCloud>, http: Http) -> Secrets { Secrets { gcloud, http, cache: Mutex::new(HashMap::new()) } }

    pub async fn valeur(&self, nom: &str, b: &Business) -> Res<String> {
        let compte = format!("{}.{nom}", b.id);
        if let Some(v) = self.cache.lock().get(&compte) {
            return Ok(v.clone());
        }
        if let Some(v) = std::env::var(nom).ok().filter(|v| !v.is_empty()) {
            return Ok(v);
        }
        if let Some(v) = trousseau::lire(&compte)? {
            self.cache.lock().insert(compte, v.clone());
            return Ok(v);
        }
        let Some(projet) = b.config.firebase_project.as_deref() else {
            return Err(Erreur::NonConfigure(format!("clé {nom} absente du Trousseau pour {}", b.nom)));
        };
        let v = self.secret_manager(nom, projet).await?;
        self.cache.lock().insert(compte, v.clone());
        Ok(v)
    }

    /// Range une clé dans le trousseau (Réglages).
    pub fn enregistrer(&self, nom: &str, b: &Business, valeur: &str) -> Res<()> {
        let compte = format!("{}.{nom}", b.id);
        trousseau::ecrire(&compte, valeur)?;
        self.cache.lock().insert(compte, valeur.to_string());
        Ok(())
    }

    pub fn effacer(&self, nom: &str, b: &Business) -> Res<()> {
        let compte = format!("{}.{nom}", b.id);
        trousseau::effacer(&compte)?;
        self.cache.lock().remove(&compte);
        Ok(())
    }

    /// Si la clé est dans le trousseau, sans la chercher ailleurs : pour Réglages.
    pub fn dans_trousseau(&self, nom: &str, b: &Business) -> bool {
        matches!(trousseau::lire(&format!("{}.{nom}", b.id)), Ok(Some(_)))
    }

    async fn secret_manager(&self, nom: &str, projet: &str) -> Res<String> {
        let jeton = self.gcloud.jeton().await?;
        let url = format!("https://secretmanager.googleapis.com/v1/projects/{projet}/secrets/{nom}/versions/latest:access");
        let r = Requete::get(url).jeton(&jeton).entete("x-goog-user-project", projet);
        let reponse = match self.http.objet(&r).await {
            Err(Erreur::Http(404 | 403, _)) => {
                return Err(Erreur::NonConfigure(format!("clé {nom} absente du Trousseau et de Secret Manager ({projet})")))
            }
            x => x?,
        };
        let b64 = reponse.get("payload").and_then(|p| p.get("data")).and_then(Value::as_str);
        let data = b64
            .and_then(|s| base64::engine::general_purpose::STANDARD.decode(s).ok())
            .ok_or_else(|| Erreur::Format(format!("Secret Manager, {nom} sans payload")))?;
        Ok(String::from_utf8_lossy(&data).trim().to_string())
    }
}

/// Mots de passe génériques du trousseau du système.
pub mod trousseau {
    use super::SERVICE;
    use crate::erreur::{Erreur, Res};

    fn entree(compte: &str) -> Res<keyring::Entry> {
        keyring::Entry::new(SERVICE, compte).map_err(erreur)
    }

    pub fn lire(compte: &str) -> Res<Option<String>> {
        match entree(compte)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(erreur(e)),
        }
    }

    pub fn ecrire(compte: &str, valeur: &str) -> Res<()> { entree(compte)?.set_password(valeur).map_err(erreur) }

    pub fn effacer(compte: &str) -> Res<()> {
        match entree(compte)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(erreur(e)),
        }
    }

    fn erreur(e: keyring::Error) -> Erreur { Erreur::Commande(format!("Trousseau : {e}")) }
}
