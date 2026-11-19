use super::commande;
use super::http::{Http, Requete};
use crate::erreur::{Erreur, Res};
use crate::jour::{maintenant, Date};
use chrono::Duration;
use serde_json::Value;

/// Un jeton OAuth Google de l'utilisateur, gardé jusqu'à 5 minutes de son expiration.
/// Il ouvre Secret Manager et Firestore en REST (avec l'en-tête `x-goog-user-project`).
/// D'abord les identifiants par défaut (`gcloud auth application-default login`), rafraîchis en HTTP ;
/// sinon `gcloud auth print-access-token`.
pub struct GCloud {
    http: Http,
    // tenu pendant le rafraîchissement : deux sources qui demandent en même temps attendent le même jeton
    cache: tokio::sync::Mutex<Option<(String, Date)>>,
}

impl GCloud {
    pub fn new(http: Http) -> GCloud { GCloud { http, cache: tokio::sync::Mutex::new(None) } }

    pub async fn jeton(&self) -> Res<String> {
        let mut cache = self.cache.lock().await;
        if let Some((j, expire)) = cache.as_ref() {
            if *expire > maintenant() {
                return Ok(j.clone());
            }
        }
        let (j, expire) = self.nouveau().await?;
        *cache = Some((j.clone(), expire));
        Ok(j)
    }

    async fn nouveau(&self) -> Res<(String, Date)> {
        if let Some((client, secret, refresh)) = identifiants() {
            let r = Requete::new("https://oauth2.googleapis.com/token", "POST").formulaire(&[
                ("grant_type", "refresh_token"),
                ("client_id", &client),
                ("client_secret", &secret),
                ("refresh_token", &refresh),
            ]);
            let reponse = self.http.objet(&r).await?;
            let jeton = reponse.get("access_token").and_then(Value::as_str).ok_or_else(|| Erreur::Format("jeton Google absent".into()))?;
            let duree = reponse.get("expires_in").and_then(Value::as_f64).unwrap_or(3600.0);
            return Ok((jeton.to_string(), maintenant() + Duration::seconds(duree as i64 - 300)));
        }
        let sortie = commande::texte("gcloud", &["auth", "print-access-token"]).await.map_err(|e| {
            Erreur::NonConfigure(format!("pas de compte Google (gcloud auth application-default login) : {e}"))
        })?;
        match sortie.lines().map(str::trim).rfind(|l| !l.is_empty()) {
            Some(j) if j.starts_with("ya29.") => Ok((j.to_string(), maintenant() + Duration::minutes(50))),
            _ => Err(Erreur::NonConfigure("pas de compte Google : gcloud auth application-default login".into())),
        }
    }
}

/// `client_id`, `client_secret`, `refresh_token` d'identifiants `authorized_user`.
fn identifiants() -> Option<(String, String, String)> {
    let data = std::fs::read(crate::chemins::adc()).ok()?;
    let id: Value = serde_json::from_slice(&data).ok()?;
    if id.get("type")?.as_str()? != "authorized_user" {
        return None;
    }
    let champ = |k: &str| id.get(k).and_then(Value::as_str).map(String::from);
    Some((champ("client_id")?, champ("client_secret")?, champ("refresh_token")?))
}
