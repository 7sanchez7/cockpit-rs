//! LurkAPI : la recherche et les profils de TikTok et Instagram par API, sans session ni navigateur.
//! Fournisseur principal de la veille ; TikHub prend le relais si un appel échoue.
use super::http::{encoder, Http, Requete};
use super::source::Contexte;
use crate::erreur::{format_err, Erreur, Res};
use crate::modeles::Plateforme;
use serde_json::Value;

/// Le nom du secret de la clé LurkAPI : variable d'environnement du même nom, trousseau du système, ou Secret Manager.
pub const CLE: &str = "LURKAPI_API_KEY";
/// Le réglage (dans la base) où Réglages range la clé saisie à la main ; il passe avant le trousseau.
pub const REGLAGE: &str = "veille.cle_scrappeur";
const BASE: &str = "https://api.lurkapi.com";

/// La clé LurkAPI : le réglage saisi dans Réglages d'abord, puis l'environnement, le trousseau ou Secret Manager.
pub async fn cle(ctx: &Contexte) -> Option<String> {
    if let Ok(Some(v)) = ctx.store.reglage(REGLAGE) {
        let v = v.trim();
        if !v.is_empty() {
            return Some(v.to_string());
        }
    }
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

/// Un appel GET à LurkAPI ; rend l'objet de la réponse, `success` vérifié.
async fn appel(http: &Http, cle: &str, chemin: &str, params: &[(&str, String)]) -> Res<Value> {
    let o = http.objet(&Requete::get(url(chemin, params)).entete("x-api-key", cle)).await?;
    if o.get("success").and_then(Value::as_bool) != Some(true) {
        let msg = o.get("error").and_then(Value::as_str).unwrap_or("réponse LurkAPI");
        return Err(format_err(format!("LurkAPI : {msg}")));
    }
    Ok(Value::Object(o))
}

/// Une recherche : les pages JSON à lire par `parseurs`.
pub async fn chercher(http: &Http, cle: &str, p: Plateforme, mot: &str, voulues: i64) -> Res<Vec<Value>> {
    match p {
        Plateforme::Tiktok => Ok(paginer(http, cle, "/v1/tiktok/search/keyword", ("query", mot), cursor("has_more", pages(voulues, 20, 5))).await?),
        Plateforme::Instagram => {
            let pages = paginer(http, cle, "/v2/instagram/reels/search", ("query", mot), cursor("has_more", pages(voulues, 12, 3))).await?;
            Ok(pages.into_iter().map(marquer_reels_search).collect())
        }
        _ => Err(Erreur::NonConfigure(format!("LurkAPI ne cherche pas sur {}", p.nom()))),
    }
}

/// Les dernières vidéos d'un compte, pour sa médiane : les pages JSON.
pub async fn dernieres(http: &Http, cle: &str, p: Plateforme, handle: &str, n: usize) -> Res<Vec<Value>> {
    let n = n as i64;
    match p {
        Plateforme::Tiktok => {
            let mut pages = paginer(http, cle, "/v3/tiktok/profile/videos", ("handle", handle), cursor("has_more", pages(n, 15, 2))).await?;
            for page in &mut pages {
                remplir_abonnes(page);
            }
            Ok(pages)
        }
        Plateforme::Instagram => {
            let mut pages = paginer(http, cle, "/v1/instagram/user/reels", ("username", handle), Paging { champ_curseur: "end_cursor", parametre_curseur: "after", champ_plus: "has_next_page", max: pages(n, 12, 2) }).await?;
            for page in &mut pages {
                marquer_reels_profil(page, handle);
            }
            Ok(pages)
        }
        _ => Err(Erreur::NonConfigure(format!("LurkAPI ne lit pas {}", p.nom()))),
    }
}

/// Comment une liste se pagine : le champ du curseur dans la réponse, le nom du paramètre qui le renvoie, et le drapeau
/// « il y a une page de plus ».
struct Paging {
    champ_curseur: &'static str,
    parametre_curseur: &'static str,
    champ_plus: &'static str,
    max: i64,
}

/// Les pages d'une liste paginée : `premier` paramètre, puis le curseur rendu par chaque réponse.
async fn paginer(http: &Http, cle: &str, chemin: &str, premier: (&str, &str), paging: Paging) -> Res<Vec<Value>> {
    let mut pages = vec![];
    let mut curseur: Option<String> = None;
    for _ in 0..paging.max {
        let mut params = vec![(premier.0, premier.1.to_string())];
        if let Some(c) = &curseur {
            params.push((paging.parametre_curseur, c.clone()));
        }
        let page = appel(http, cle, chemin, &params).await?;
        curseur = page.get(paging.champ_curseur).and_then(Value::as_str).map(String::from);
        let plus = page.get(paging.champ_plus).and_then(Value::as_bool).unwrap_or(false);
        pages.push(page);
        if !plus || curseur.is_none() {
            break;
        }
    }
    Ok(pages)
}

/// La pagination où le curseur porte le même nom dans la réponse et dans la requête.
fn cursor(champ_plus: &'static str, max: i64) -> Paging {
    Paging { champ_curseur: "cursor", parametre_curseur: "cursor", champ_plus, max }
}

/// Le nombre de pages pour lire `voulues` vidéos, `par_page` à la fois, au plus `max`.
fn pages(voulues: i64, par_page: i64, max: i64) -> i64 { ((voulues.max(1) + par_page - 1) / par_page).min(max) }

/// La recherche Instagram ne dit pas que ce sont des reels : le parseur l'exige.
fn marquer_reels_search(mut page: Value) -> Value {
    if let Some(liste) = page.get_mut("reels").and_then(Value::as_array_mut) {
        for r in liste {
            if let Some(o) = r.as_object_mut() {
                o.entry("media_type").or_insert(Value::from(2));
            }
        }
    }
    page
}

/// Le profil Instagram ne donne ni l'auteur, ni le type, ni la date : l'id `pk` porte la date du média.
fn marquer_reels_profil(page: &mut Value, handle: &str) {
    let abonnes = page.get("follower_count").and_then(Value::as_i64);
    if let Some(liste) = page.get_mut("reels").and_then(Value::as_array_mut) {
        for r in liste {
            let Some(o) = r.as_object_mut() else { continue };
            o.entry("media_type").or_insert(Value::from(2));
            if let Some(pk) = o.get("pk").cloned() {
                o.entry("id").or_insert(pk);
            }
            o.entry("user").or_insert(serde_json::json!({ "username": handle, "follower_count": abonnes }));
        }
    }
}

/// Le profil TikTok ne met les abonnés qu'au niveau de la réponse : le parseur les lit sur chaque vidéo.
fn remplir_abonnes(page: &mut Value) {
    let Some(n) = page.get("followerCount").and_then(Value::as_i64) else { return };
    if let Some(liste) = page.get_mut("videos").and_then(Value::as_array_mut) {
        for item in liste {
            if let Some(auteur) = item.get_mut("author").and_then(Value::as_object_mut) {
                auteur.entry("followerCount").or_insert(Value::from(n));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn urls_et_pages() {
        assert_eq!(url("/x", &[]), "https://api.lurkapi.com/x");
        assert_eq!(url("/x", &[("a", "b c".into())]), "https://api.lurkapi.com/x?a=b%20c");
        assert_eq!(pages(10, 20, 5), 1);
        assert_eq!(pages(40, 20, 5), 2);
        assert_eq!(pages(100, 12, 3), 3);
    }

    #[test]
    fn reels_du_profil_completes() {
        let mut page = json!({"success": true, "follower_count": 5000, "reels": [
            {"pk": "3993300839542953931", "code": "DdrDfTGOhfL", "play_count": 12145, "like_count": 107}
        ]});
        marquer_reels_profil(&mut page, "nike");
        let r = &page["reels"][0];
        assert_eq!(r["media_type"], 2);
        assert_eq!(r["id"], "3993300839542953931");
        assert_eq!(r["user"]["username"], "nike");
        assert_eq!(r["user"]["follower_count"], 5000);
    }

    #[test]
    fn abonnes_repartis_sur_les_videos_tiktok() {
        let mut page = json!({"followerCount": 43700, "videos": [{"id": "1", "author": {"uniqueId": "x"}}]});
        remplir_abonnes(&mut page);
        assert_eq!(page["videos"][0]["author"]["followerCount"], 43700);
    }

    /// La réponse TikTok de LurkAPI est la forme web que `parseurs::tiktok` lit déjà.
    #[test]
    fn recherche_tiktok_devient_des_trouvailles() {
        let page = json!({"success": true, "videos": [{
            "id": "7694350820791291167", "desc": "3 erreurs en espagnol", "createTime": 1799900000,
            "author": {"uniqueId": "profe.ana", "followerCount": 12000},
            "stats": {"playCount": 250000, "diggCount": 9000, "commentCount": 300, "shareCount": 800, "collectCount": 1200},
            "video": {"duration": 21, "cover": "https://p16/cover.jpg", "playUrl": "https://cdn/v.mp4"}
        }]});
        let t = &crate::sources::veille::parseurs::trouvailles_objets(Plateforme::Tiktok, &[page])[0];
        assert_eq!(t.compte, "profe.ana");
        assert_eq!((t.vues, t.likes, t.enregistrements, t.abonnes), (Some(250_000), Some(9_000), Some(1_200), Some(12_000)));
        assert_eq!(t.duree_s, Some(21.0));
        assert_eq!(t.legende.as_deref(), Some("3 erreurs en espagnol"));
    }

    /// La recherche Instagram de LurkAPI : `caption` en chaîne, et `media_type` posé pour le parseur.
    #[test]
    fn recherche_instagram_devient_des_trouvailles() {
        let page = marquer_reels_search(json!({"success": true, "reels": [{
            "pk": "3878214262204433501", "code": "DXSL1LljIRd", "caption": "POV: un chien sous la douche",
            "taken_at": 1799900000, "like_count": 9648, "comment_count": 85, "play_count": 477403,
            "display_url": "https://cdn/i.jpg", "video_url": "https://cdn/v.mp4",
            "user": {"username": "laughingpaws.tv"}
        }]}));
        let t = &crate::sources::veille::parseurs::trouvailles_objets(Plateforme::Instagram, &[page])[0];
        assert_eq!((t.brut.as_str(), t.compte.as_str()), ("DXSL1LljIRd", "laughingpaws.tv"));
        assert_eq!((t.vues, t.likes, t.commentaires), (Some(477_403), Some(9_648), Some(85)));
        assert_eq!(t.legende.as_deref(), Some("POV: un chien sous la douche"));
        assert_eq!(t.media.as_deref(), Some("https://cdn/v.mp4"));
    }
}
