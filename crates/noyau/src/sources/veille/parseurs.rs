//! Les vidéos dans le JSON que reçoivent les pages TikTok et Instagram. On cherche les objets vidéo partout dans
//! l'arbre plutôt qu'à un chemin précis : la recherche, le profil et les balises de la page les rangent différemment.
use crate::collecte::ytdlp::Liste;
use crate::modeles::Plateforme;
use crate::modeles_veille::Trouvaille;
use chrono::{TimeZone, Utc};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Les vidéos des corps reçus par la page, chacune une fois, lectures fusionnées.
pub fn trouvailles(p: Plateforme, corps: &[String]) -> Vec<Trouvaille> {
    let mut par_id: HashMap<String, Trouvaille> = HashMap::new();
    let mut ordre: Vec<String> = vec![];
    for o in corps.iter().flat_map(|c| objets(c)) {
        parcourir(&o, &mut |d| {
            let Some(t) = (if p == Plateforme::Tiktok { tiktok(d) } else { instagram(d) }) else { return };
            match par_id.get(&t.id()) {
                Some(avant) => {
                    let f = avant.fusion(&t);
                    par_id.insert(t.id(), f);
                }
                None => {
                    ordre.push(t.id());
                    par_id.insert(t.id(), t);
                }
            }
        });
    }
    ordre.iter().filter_map(|id| par_id.remove(id)).collect()
}

/// Une liste de profil (`yt-dlp`, `feed/user`) en trouvailles : chaque vidéo avec son relevé.
pub fn trouvailles_liste(l: &Liste, p: Plateforme, compte: &str, abonnes: Option<i64>) -> Vec<Trouvaille> {
    let mut releves = HashMap::new();
    for r in &l.releves {
        releves.entry(r.video.as_str()).or_insert(r);
    }
    let prefixe = p.id_video("");
    l.videos.iter().map(|v| {
        let r = releves.get(v.id.as_str());
        Trouvaille {
            plateforme: p, brut: v.id.strip_prefix(&prefixe).unwrap_or(&v.id).to_string(), compte: compte.into(),
            legende: v.legende.clone(), publiee: v.publiee_le, duree_s: v.duree_s, vues: r.map(|r| r.vues),
            likes: r.and_then(|r| r.likes), commentaires: r.and_then(|r| r.commentaires), partages: r.and_then(|r| r.partages),
            enregistrements: r.and_then(|r| r.enregistrements), abonnes, image: l.images.get(&v.id).cloned(), media: None,
        }
    }).collect()
}

/// Un objet vidéo TikTok : `id`, `stats.playCount`, `author.uniqueId`.
pub fn tiktok(d: &Map<String, Value>) -> Option<Trouvaille> {
    let id = texte(d.get("id"))?;
    let auteur = d.get("author")?.as_object()?;
    let handle = auteur.get("uniqueId")?.as_str()?;
    let stats = d.get("stats").and_then(Value::as_object).or_else(|| d.get("statsV2").and_then(Value::as_object))?;
    if !stats.contains_key("playCount") {
        return None;
    }
    let vide = Map::new();
    let v2 = d.get("statsV2").and_then(Value::as_object).unwrap_or(&vide);
    let n = |cle: &str| nombre(stats.get(cle)).or_else(|| nombre(v2.get(cle)));
    let video = d.get("video").and_then(Value::as_object).unwrap_or(&vide);
    Some(Trouvaille {
        plateforme: Plateforme::Tiktok, brut: id, compte: handle.into(), legende: chaine(d.get("desc")),
        publiee: nombre(d.get("createTime")).and_then(date), duree_s: nombre(video.get("duration")).map(|s| s as f64),
        vues: n("playCount"), likes: n("diggCount"), commentaires: n("commentCount"), partages: n("shareCount"),
        enregistrements: n("collectCount"),
        abonnes: nombre(d.get("authorStats").and_then(|s| s.get("followerCount"))).or_else(|| nombre(auteur.get("followerCount"))),
        image: video.get("cover").and_then(Value::as_str).or_else(|| video.get("originCover").and_then(Value::as_str)).map(String::from),
        media: None,
    })
}

/// Un reel Instagram : `code` (ou `shortcode`), `taken_at` ou `play_count` (l'onglet Reels d'un profil ne donne pas la
/// date), vidéo (`media_type` 2 ou `product_type` clips).
pub fn instagram(d: &Map<String, Value>) -> Option<Trouvaille> {
    let code = d.get("code").and_then(Value::as_str).or_else(|| d.get("shortcode").and_then(Value::as_str))?;
    if !(d.contains_key("taken_at") || d.contains_key("taken_at_timestamp") || d.contains_key("play_count")) {
        return None;
    }
    let video = nombre(d.get("media_type")) == Some(2) || d.get("product_type").and_then(Value::as_str) == Some("clips")
        || d.get("is_video").and_then(Value::as_bool) == Some(true);
    if !video {
        return None;
    }
    let user = d.get("user").and_then(Value::as_object).or_else(|| d.get("owner").and_then(Value::as_object))?;
    let handle = user.get("username")?.as_str()?;
    let legende = d.get("caption").and_then(|c| c.get("text")).and_then(Value::as_str).or_else(|| {
        d.get("edge_media_to_caption")?.get("edges")?.as_array()?.first()?.get("node")?.get("text")?.as_str()
    });
    let image = d.get("image_versions2").and_then(|i| i.get("candidates")?.as_array()?.first()?.get("url")?.as_str())
        .or_else(|| d.get("display_url").and_then(Value::as_str));
    let media = d.get("video_versions").and_then(|v| v.as_array()?.first()?.get("url")?.as_str())
        .or_else(|| d.get("video_url").and_then(Value::as_str));
    let premier = |cles: &[&str]| cles.iter().find_map(|c| nombre(d.get(*c)));
    Some(Trouvaille {
        plateforme: Plateforme::Instagram, brut: code.into(), compte: handle.into(),
        legende: legende.filter(|l| !l.is_empty()).map(String::from),
        publiee: premier(&["taken_at", "taken_at_timestamp"]).and_then(date),
        duree_s: d.get("video_duration").and_then(Value::as_f64),
        vues: premier(&["play_count", "ig_play_count", "view_count", "video_view_count"]),
        likes: nombre(d.get("like_count")).or_else(|| nombre(d.get("edge_liked_by").and_then(|e| e.get("count")))),
        commentaires: nombre(d.get("comment_count")), partages: premier(&["reshare_count", "share_count"]),
        enregistrements: None, abonnes: nombre(user.get("follower_count")), image: image.map(String::from),
        media: media.map(String::from),
    })
}

// JSON

/// Les objets d'un corps : un JSON, ou un par ligne (Instagram envoie parfois plusieurs morceaux), sans le
/// préfixe anti-détournement `for (;;);`.
pub fn objets(corps: &str) -> Vec<Value> {
    let t = corps.trim();
    let t = t.strip_prefix("for (;;);").unwrap_or(t);
    if let Ok(o) = serde_json::from_str::<Value>(t) {
        return vec![o];
    }
    t.split('\n').filter(|l| !l.is_empty()).filter_map(|l| serde_json::from_str(l).ok()).collect()
}

pub fn parcourir(o: &Value, f: &mut impl FnMut(&Map<String, Value>)) {
    match o {
        Value::Object(d) => {
            f(d);
            for v in d.values() {
                parcourir(v, f);
            }
        }
        Value::Array(a) => {
            for v in a {
                parcourir(v, f);
            }
        }
        _ => {}
    }
}

/// Un compteur en nombre ou en texte (`statsV2` de TikTok).
pub fn nombre(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => s.parse().ok(),
        Value::Bool(b) => Some(i64::from(*b)),
        _ => None,
    }
}

fn texte(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) => (!s.is_empty()).then(|| s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn chaine(v: Option<&Value>) -> Option<String> { v?.as_str().filter(|s| !s.is_empty()).map(String::from) }

fn date(secondes: i64) -> Option<crate::jour::Date> { Utc.timestamp_opt(secondes, 0).single() }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modeles::{Releve, Video};

    /// Une réponse de recherche TikTok réduite : la vidéo est au fond de `data[].item`, `statsV2` en texte.
    const RECHERCHE_TIKTOK: &str = r#"
        {"data":[{"type":1,"item":{"id":"7300000000000000001","desc":"3 erreurs en espagnol","createTime":1799900000,
          "video":{"duration":21,"cover":"https://p16/cover.jpg"},
          "author":{"uniqueId":"profe.ana"},"authorStats":{"followerCount":12000},
          "stats":{"playCount":250000,"diggCount":9000,"commentCount":300,"shareCount":800,"collectCount":1200},
          "statsV2":{"playCount":"250431"}}}]}
        "#;

    /// Le fil d'un profil Instagram (`feed/user`), tel que le lit la source Instagram, devient des trouvailles au bon id.
    #[test]
    fn profil_instagram_en_trouvailles() {
        let at = Utc.timestamp_opt(1_800_000_000, 0).unwrap();
        let id = Plateforme::Instagram.id_video("DAbc");
        let l = Liste {
            videos: vec![Video {
                id: id.clone(), business: "".into(), compte: "profe.ana".into(), legende: None, publiee_le: date(1_799_900_000),
                duree_s: None, url: Some(Plateforme::Instagram.lien("DAbc", "profe.ana")), plateforme: Plateforme::Instagram,
            }],
            releves: vec![Releve { video: id.clone(), at, vues: 40_000, likes: Some(900), ..Default::default() }],
            ..Default::default()
        };
        let t = &trouvailles_liste(&l, Plateforme::Instagram, "profe.ana", Some(5_000))[0];
        assert_eq!(t.brut, "DAbc");
        assert_eq!(t.id(), id);
        assert_eq!(t.vues, Some(40_000));
        assert_eq!(t.likes, Some(900));
        assert_eq!(t.abonnes, Some(5_000));
    }

    #[test]
    fn tiktok_dans_la_recherche() {
        let t = &trouvailles(Plateforme::Tiktok, &[RECHERCHE_TIKTOK.into()])[0];
        assert_eq!(t.id(), Plateforme::Tiktok.id_video("7300000000000000001"));
        assert_eq!(t.compte, "profe.ana");
        assert_eq!(t.vues, Some(250_000));
        assert_eq!(t.enregistrements, Some(1_200));
        assert_eq!(t.abonnes, Some(12_000));
        assert_eq!(t.duree_s, Some(21.0));
        assert_eq!(t.image.as_deref(), Some("https://p16/cover.jpg"));
        assert_eq!(t.publiee, date(1_799_900_000));
    }

    /// Instagram : préfixe `for (;;);`, un reel avec vues, une photo ignorée, et le même reel vu deux fois.
    #[test]
    fn instagram_fusionne_et_ignore_les_photos() {
        let reel = |likes: i64, vues: &str| format!(r#"{{"code":"ABC","taken_at":1799900000,"media_type":2,"user":{{"username":"hola.es","follower_count":5000}},"like_count":{likes},"comment_count":40,"play_count":{vues},"caption":{{"text":"Ser o estar"}},"video_versions":[{{"url":"https://cdn/v.mp4"}}]}}"#);
        let corps1 = format!(r#"for (;;);{{"items":[{},{{"code":"PHOTO","taken_at":1,"media_type":1,"user":{{"username":"x"}}}}]}}"#, reel(900, "null"));
        let corps2 = format!(r#"{{"a":{{"b":[{}]}}}}"#, reel(1000, "80000"));
        let ts = trouvailles(Plateforme::Instagram, &[corps1, corps2]);
        assert_eq!(ts.len(), 1);
        assert_eq!(ts[0].vues, Some(80_000));
        assert_eq!(ts[0].likes, Some(1_000));
        assert_eq!(ts[0].media.as_deref(), Some("https://cdn/v.mp4"));
        assert_eq!(ts[0].legende.as_deref(), Some("Ser o estar"));
    }

    /// L'onglet Reels d'un profil (`clips_connection`) ne donne pas la date : le reel passe sur `play_count`.
    #[test]
    fn reels_d_un_profil_sans_date() {
        let corps = r#"{"data":{"fetch__XDTUserDict":{"clips_connection":{"edges":[{"node":{"media":{"code":"DOh","product_type":"clips","media_type":2,"play_count":47666,"like_count":900,"comment_count":40,"user":{"username":"espagnolpasapas"}}}}]}}}}"#;
        let t = &trouvailles(Plateforme::Instagram, &[corps.into()])[0];
        assert_eq!(t.compte, "espagnolpasapas");
        assert_eq!(t.vues, Some(47_666));
        assert_eq!(t.publiee, None);
    }
}
