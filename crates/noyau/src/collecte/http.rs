use crate::erreur::{Erreur, Res};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::time::Duration;

/// Pour les pages publiques qui ne répondent qu'à un navigateur.
pub const UA_NAVIGATEUR: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36";
/// Safari, pour les sessions lues dans Safari : un site peut refuser une session qui change de navigateur.
pub const UA_SAFARI: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/27.2 Safari/605.1.15";
/// Le navigateur dont vient la session : Safari sur Mac, WebView2 (Edge) sous Windows.
#[cfg(target_os = "macos")]
pub const UA_SESSION: &str = UA_SAFARI;
#[cfg(not(target_os = "macos"))]
pub const UA_SESSION: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36 Edg/140.0.0.0";

/// L'en-tête `Cookie` d'une session, quelle que soit sa provenance (Safari ou la fenêtre Sessions).
pub fn entete_cookie(cookies: &HashMap<String, String>) -> String {
    let mut v: Vec<String> = cookies.iter().map(|(k, v)| format!("{k}={v}")).collect();
    v.sort();
    v.join("; ")
}

/// Une requête prête, rejouable.
#[derive(Debug, Clone)]
pub struct Requete {
    pub url: String,
    pub methode: String,
    pub entetes: Vec<(String, String)>,
    pub corps: Option<Vec<u8>>,
    /// Le délai de la requête, s'il dépasse celui du client (60 s).
    pub delai: Option<Duration>,
}

impl Requete {
    pub fn get(url: impl Into<String>) -> Requete { Requete::new(url, "GET") }

    pub fn new(url: impl Into<String>, methode: &str) -> Requete {
        Requete { url: url.into(), methode: methode.into(), entetes: vec![], corps: None, delai: None }
    }

    /// `Authorization: Bearer <jeton>`.
    pub fn jeton(self, jeton: &str) -> Requete { self.entete("Authorization", &format!("Bearer {jeton}")) }

    /// Laisse `secondes` à la réponse, pour un appel lent (une analyse d'images par Claude).
    pub fn delai(mut self, secondes: u64) -> Requete {
        self.delai = Some(Duration::from_secs(secondes));
        self
    }

    /// Remplace un en-tête du même nom.
    pub fn entete(mut self, nom: &str, valeur: &str) -> Requete {
        self.entetes.retain(|(k, _)| !k.eq_ignore_ascii_case(nom));
        self.entetes.push((nom.into(), valeur.into()));
        self
    }

    /// Le corps en JSON.
    pub fn json(self, corps: &Value) -> Requete {
        let mut r = self.entete("Content-Type", "application/json");
        r.corps = Some(serde_json::to_vec(corps).unwrap_or_default());
        r
    }

    /// Le corps en `application/x-www-form-urlencoded`.
    pub fn formulaire(self, champs: &[(&str, &str)]) -> Requete {
        let corps = champs.iter().map(|(k, v)| format!("{}={}", encoder(k), encoder(v))).collect::<Vec<_>>().join("&");
        self.brut(corps.into_bytes(), "application/x-www-form-urlencoded")
    }

    pub fn brut(self, corps: Vec<u8>, type_: &str) -> Requete {
        let mut r = self.entete("Content-Type", type_);
        r.corps = Some(corps);
        r
    }
}

/// L'encodage d'une valeur dans une URL ou un formulaire (RFC 3986, caractères non réservés gardés).
pub fn encoder(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Appels réseau des sources : une session sans cookies partagés, rejeu sur 429 et 5xx.
#[derive(Clone)]
pub struct Http {
    client: reqwest::Client,
}

impl Default for Http {
    fn default() -> Self { Http::new() }
}

impl Http {
    pub fn new() -> Http {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("client HTTP");
        Http { client }
    }

    /// Le corps d'une réponse 2xx. Rejoue jusqu'à 3 fois sur 429 et 5xx (Retry-After respecté, 30 s au plus).
    pub async fn donnees(&self, r: &Requete) -> Res<Vec<u8>> {
        let methode = reqwest::Method::from_bytes(r.methode.as_bytes()).map_err(|_| Erreur::Format("méthode invalide".into()))?;
        let mut attente = 1.0_f64;
        for essai in 1..=3 {
            let mut b = self.client.request(methode.clone(), &r.url);
            for (k, v) in &r.entetes {
                b = b.header(k, v);
            }
            if let Some(c) = &r.corps {
                b = b.body(c.clone());
            }
            if let Some(d) = r.delai {
                b = b.timeout(d);
            }
            let reponse = b.send().await.map_err(|e| Erreur::Commande(format!("réseau : {e}")))?;
            let code = reponse.status().as_u16();
            let retry = reponse.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.trim().parse::<f64>().ok());
            let data = reponse.bytes().await.map_err(|e| Erreur::Commande(format!("réseau : {e}")))?.to_vec();
            if (200..300).contains(&code) {
                return Ok(data);
            }
            if (code == 429 || code >= 500) && essai < 3 {
                let delai = retry.unwrap_or(attente).min(30.0);
                tokio::time::sleep(Duration::from_secs_f64(delai)).await;
                attente *= 2.0;
                continue;
            }
            return Err(Erreur::Http(code, extrait(&data, 300)));
        }
        Err(Erreur::Http(0, "aucune réponse".into()))
    }

    pub async fn texte(&self, r: &Requete) -> Res<String> { Ok(String::from_utf8_lossy(&self.donnees(r).await?).into_owned()) }

    /// Le corps décodé en `T`.
    pub async fn json<T: DeserializeOwned>(&self, r: &Requete) -> Res<T> {
        let data = self.donnees(r).await?;
        decoder(&data)
    }

    /// Le corps en objet, pour les réponses qu'on ne lit qu'en partie.
    pub async fn objet(&self, r: &Requete) -> Res<Map<String, Value>> {
        let data = self.donnees(r).await?;
        objet(&data)
    }
}

/// Des octets JSON décodés en `T` ; l'erreur nomme le type.
pub fn decoder<T: DeserializeOwned>(data: &[u8]) -> Res<T> {
    serde_json::from_slice(data).map_err(|e| {
        let nom = std::any::type_name::<T>().rsplit("::").next().unwrap_or("?");
        Erreur::Format(format!("{nom} : {e}"))
    })
}

pub fn objet(data: &[u8]) -> Res<Map<String, Value>> {
    match serde_json::from_slice::<Value>(data) {
        Ok(Value::Object(o)) => Ok(o),
        _ => Err(Erreur::Format(format!("pas un objet JSON : {}", extrait(data, 200)))),
    }
}

/// Les `n` premiers octets en texte, sans couper un caractère.
pub fn extrait(data: &[u8], n: usize) -> String {
    let s = String::from_utf8_lossy(&data[..data.len().min(n)]).into_owned();
    s.trim_end_matches('\u{FFFD}').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requetes() {
        let r = Requete::get("https://x").jeton("a").entete("authorization", "Bearer b");
        assert_eq!(r.entetes, vec![("authorization".to_string(), "Bearer b".to_string())]);
        let f = Requete::new("https://x", "POST").formulaire(&[("a b", "é/=")]);
        assert_eq!(f.corps.unwrap(), b"a%20b=%C3%A9%2F%3D");
        assert!(objet(b"[1]").is_err());
        assert_eq!(extrait("éa".as_bytes(), 1), "");
    }
}
