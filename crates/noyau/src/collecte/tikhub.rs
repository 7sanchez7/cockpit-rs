//! TikHub : la recherche et les profils de TikTok et Instagram par API, sans session ni navigateur.
//! Remplace, quand la clé existe, la page cachée de TikTok et la session d'Instagram pour la veille.
use super::http::{encoder, Http, Requete};
use super::source::Contexte;
use crate::erreur::{format_err, Erreur, Res};
use crate::modeles::Plateforme;
use serde_json::Value;

/// Le nom du secret de la clé TikHub : variable d'environnement du même nom, trousseau du système, ou Secret Manager.
pub const CLE: &str = "TIKHUB_API_KEY";
const BASE: &str = "https://api.tikhub.io";
/// Les vidéos par page d'une recherche TikTok (le maximum de l'API).
const PAGE_TIKTOK: i64 = 20;

/// La clé TikHub, lue sur le premier business qui l'a ; `None` : la veille garde ses anciens chemins.
pub async fn cle(ctx: &Contexte) -> Option<String> {
    for b in &ctx.businesses {
        if let Ok(v) = ctx.secrets.valeur(CLE, b).await {
            return Some(v);
        }
    }
    None
}

/// L'URL d'un appel, ses paramètres encodés.
fn url(chemin: &str, params: &[(&str, String)]) -> String {
    if params.is_empty() {
        return format!("{BASE}{chemin}");
    }
    let q = params.iter().map(|(k, v)| format!("{k}={}", encoder(v))).collect::<Vec<_>>().join("&");
    format!("{BASE}{chemin}?{q}")
}

/// Un appel GET à TikHub ; rend son `data`, décodé si l'API le renvoie en chaîne.
async fn appel(http: &Http, cle: &str, chemin: &str, params: &[(&str, String)]) -> Res<Value> {
    let mut o = http.objet(&Requete::get(url(chemin, params)).jeton(cle)).await?;
    let code = o.get("code").and_then(Value::as_i64).unwrap_or(0);
    if code != 200 {
        let msg = o.get("message").and_then(Value::as_str).unwrap_or("réponse TikHub");
        return Err(format_err(format!("TikHub code {code} : {msg}")));
    }
    Ok(match o.remove("data").unwrap_or(Value::Null) {
        Value::String(s) => serde_json::from_str(&s).unwrap_or(Value::String(s)),
        v => v,
    })
}

/// Une recherche : les corps JSON à lire par `parseurs`, un par page.
pub async fn chercher(http: &Http, cle: &str, p: Plateforme, mot: &str, voulues: i64) -> Res<Vec<Value>> {
    match p {
        Plateforme::Tiktok => chercher_tiktok(http, cle, mot, voulues).await,
        Plateforme::Instagram => Ok(vec![appel(http, cle, "/api/v1/instagram/v2/search_reels", &[("keyword", mot.to_string())]).await?]),
        _ => Err(Erreur::NonConfigure(format!("TikHub ne cherche pas sur {}", p.nom()))),
    }
}

async fn chercher_tiktok(http: &Http, cle: &str, mot: &str, voulues: i64) -> Res<Vec<Value>> {
    let mut corps = vec![];
    let mut offset = 0;
    for _ in 0..pages(voulues) {
        corps.push(appel(http, cle, "/api/v1/tiktok/app/v3/fetch_general_search_result", &[
            ("keyword", mot.to_string()),
            ("offset", offset.to_string()),
            ("count", PAGE_TIKTOK.to_string()),
            ("sort_type", "0".into()),
            ("publish_time", "0".into()),
        ]).await?);
        offset += PAGE_TIKTOK;
    }
    Ok(corps)
}

/// Les dernières vidéos d'un compte, pour sa médiane : un corps JSON.
pub async fn dernieres(http: &Http, cle: &str, p: Plateforme, handle: &str, n: usize) -> Res<Vec<Value>> {
    let data = match p {
        Plateforme::Tiktok => appel(http, cle, "/api/v1/tiktok/app/v3/fetch_user_post_videos_v2", &[
            ("unique_id", handle.to_string()),
            ("count", n.to_string()),
            ("sort_type", "0".into()),
        ]).await?,
        Plateforme::Instagram => appel(http, cle, "/api/v1/instagram/v2/fetch_user_reels", &[("username", handle.to_string())]).await?,
        _ => return Err(Erreur::NonConfigure(format!("TikHub ne lit pas {}", p.nom()))),
    };
    Ok(vec![data])
}

/// Le nombre de pages de 20 vidéos pour en lire `voulues`.
fn pages(voulues: i64) -> i64 { (voulues.clamp(1, 100) + PAGE_TIKTOK - 1) / PAGE_TIKTOK }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_et_pages() {
        assert_eq!(url("/x", &[]), "https://api.tikhub.io/x");
        assert_eq!(url("/x", &[("a", "b c".into())]), "https://api.tikhub.io/x?a=b%20c");
        assert_eq!(pages(10), 1);
        assert_eq!(pages(20), 1);
        assert_eq!(pages(40), 2);
        assert_eq!(pages(100), 5);
    }
}
