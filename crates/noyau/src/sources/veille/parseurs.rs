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
    let objets: Vec<Value> = corps.iter().flat_map(|c| objets(c)).collect();
    trouvailles_objets(p, &objets)
}

/// Les vidéos d'objets déjà décodés (une réponse TikHub), chacune une fois, lectures fusionnées.
pub fn trouvailles_objets(p: Plateforme, objets: &[Value]) -> Vec<Trouvaille> {
    let mut par_id: HashMap<String, Trouvaille> = HashMap::new();
    let mut ordre: Vec<String> = vec![];
    for o in objets {
        parcourir(o, &mut |d| {
            let Some(t) = (if p == Plateforme::Tiktok { tiktok(d).or_else(|| tiktok_app(d)) } else { instagram(d) }) else { return };
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

/// Un objet vidéo de l'API mobile de TikTok (TikHub App V3) : `aweme_id`, `statistics.play_count`,
/// `author.unique_id`, la durée en millisecondes.
pub fn tiktok_app(d: &Map<String, Value>) -> Option<Trouvaille> {
    let id = texte(d.get("aweme_id"))?;
    let auteur = d.get("author")?.as_object()?;
    let handle = auteur.get("unique_id").and_then(Value::as_str).or_else(|| auteur.get("uniqueId").and_then(Value::as_str))?;
    let stats = d.get("statistics").and_then(Value::as_object).or_else(|| d.get("stats").and_then(Value::as_object))?;
    if !["play_count", "playCount"].iter().any(|c| stats.contains_key(*c)) {
        return None;
    }
    let n = |cle: &str, autre: &str| nombre(stats.get(cle)).or_else(|| nombre(stats.get(autre)));
    let video = d.get("video").and_then(Value::as_object);
    Some(Trouvaille {
        plateforme: Plateforme::Tiktok, brut: id, compte: handle.into(), legende: chaine(d.get("desc")),
        publiee: nombre(d.get("create_time")).and_then(date),
        duree_s: video.and_then(|v| nombre(v.get("duration"))).map(|ms| ms as f64 / 1000.0),
        vues: n("play_count", "playCount"), likes: n("digg_count", "diggCount"),
        commentaires: n("comment_count", "commentCount"), partages: n("share_count", "shareCount"),
        enregistrements: n("collect_count", "collectCount"),
        abonnes: nombre(auteur.get("follower_count")).or_else(|| nombre(auteur.get("followerCount"))),
        image: video.and_then(|v| url_de(v.get("cover"))).or_else(|| video.and_then(|v| url_de(v.get("origin_cover")))),
        media: video.and_then(|v| url_de(v.get("play_addr"))).or_else(|| video.and_then(|v| url_de(v.get("download_addr")))),
    })
}

/// La première URL d'un `{url_list: [...]}` (ou la chaîne elle-même).
fn url_de(v: Option<&Value>) -> Option<String> {
    let v = v?;
    v.get("url_list").and_then(Value::as_array).and_then(|l| l.iter().find_map(|u| u.as_str().map(String::from)))
        .or_else(|| v.as_str().map(String::from))
}

/// Un reel Instagram : `code` (ou `shortcode`), `taken_at`, `play_count` ou `video_view_count` (l'onglet Reels d'un
/// profil et l'embed ne donnent pas la date : elle se lit dans l'id), vidéo (`media_type` 2, `product_type` clips ou
/// `is_video`).
pub fn instagram(d: &Map<String, Value>) -> Option<Trouvaille> {
    let code = d.get("code").and_then(Value::as_str).or_else(|| d.get("shortcode").and_then(Value::as_str))?;
    if !["taken_at", "taken_at_timestamp", "play_count", "video_view_count"].iter().any(|c| d.contains_key(*c)) {
        return None;
    }
    let video = nombre(d.get("media_type")) == Some(2) || d.get("product_type").and_then(Value::as_str) == Some("clips")
        || d.get("is_video").and_then(Value::as_bool) == Some(true);
    if !video {
        return None;
    }
    let user = d.get("user").and_then(Value::as_object).or_else(|| d.get("owner").and_then(Value::as_object))?;
    let handle = user.get("username")?.as_str()?;
    let legende = d.get("caption").and_then(|c| c.get("text")).and_then(Value::as_str)
        .or_else(|| d.get("caption").and_then(Value::as_str))
        .or_else(|| {
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
        publiee: premier(&["taken_at", "taken_at_timestamp"]).and_then(date).or_else(|| date_de_l_id(d.get("id"))),
        duree_s: d.get("video_duration").and_then(Value::as_f64),
        vues: premier(&["play_count", "ig_play_count", "view_count", "video_view_count"]),
        likes: nombre(d.get("like_count")).or_else(|| compte(d.get("edge_liked_by"))),
        commentaires: nombre(d.get("comment_count")).or_else(|| compte(d.get("edge_media_to_comment"))),
        partages: premier(&["reshare_count", "share_count"]),
        enregistrements: None, abonnes: nombre(user.get("follower_count")).or_else(|| compte(user.get("edge_followed_by"))),
        image: image.map(String::from), media: media.map(String::from),
    })
}

/// Le `count` d'un objet `{"count": n}` (les anciens champs `edge_*`).
fn compte(v: Option<&Value>) -> Option<i64> { nombre(v?.get("count")) }

/// La date d'un id de média Instagram (`4033…` ou `4033…_<compte>`) : ses bits de poids fort sont les millisecondes
/// depuis le 24 août 2011.
fn date_de_l_id(id: Option<&Value>) -> Option<crate::jour::Date> {
    let id: i64 = match id? {
        Value::String(s) => s.split('_').next()?.parse().ok()?,
        v => nombre(Some(v))?,
    };
    date(((id >> 23) + 1_314_220_021_721) / 1000)
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

    /// La recherche TikHub (App V3) : la vidéo est dans `data.data[].aweme_info`, les champs en `snake_case`,
    /// la durée en millisecondes.
    const RECHERCHE_TIKHUB: &str = r#"
        {"code":200,"data":{"data":[{"type":1,"aweme_info":{"aweme_id":"7647571066872712479","desc":"3 erreurs en espagnol",
          "create_time":1799900000,
          "statistics":{"play_count":250000,"digg_count":9000,"comment_count":300,"share_count":800,"collect_count":1200},
          "author":{"unique_id":"profe.ana","nickname":"Ana","follower_count":12000},
          "video":{"duration":21000,"cover":{"url_list":["https://p16/cover.jpg"]},"play_addr":{"url_list":["https://cdn/v.mp4"]}}}}]}}
        "#;

    #[test]
    fn tiktok_cote_app() {
        let t = &trouvailles(Plateforme::Tiktok, &[RECHERCHE_TIKHUB.into()])[0];
        assert_eq!(t.id(), Plateforme::Tiktok.id_video("7647571066872712479"));
        assert_eq!(t.compte, "profe.ana");
        assert_eq!((t.vues, t.likes, t.commentaires, t.partages, t.enregistrements), (Some(250_000), Some(9_000), Some(300), Some(800), Some(1_200)));
        assert_eq!(t.abonnes, Some(12_000));
        assert_eq!(t.duree_s, Some(21.0));
        assert_eq!(t.image.as_deref(), Some("https://p16/cover.jpg"));
        assert_eq!(t.media.as_deref(), Some("https://cdn/v.mp4"));
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

    /// Le reel d'un embed (`gql_data.shortcode_media`) : ni date ni partages, les compteurs en `edge_*`, la date dans l'id.
    #[test]
    fn reel_d_un_embed() {
        let corps = r#"{"gql_data":{"shortcode_media":{"id":"4033719412274227586","shortcode":"Df6pm8UM-mC","is_video":true,
            "video_view_count":22,"video_duration":64.505,"product_type":"clips",
            "owner":{"id":"64975230875","username":"appendreespaglo","edge_followed_by":{"count":260}},
            "edge_liked_by":{"count":1},"edge_media_to_comment":{"count":0},"display_url":"https://cdn/i.jpg",
            "video_url":"https://cdn/v.mp4","edge_media_to_caption":{"edges":[{"node":{"text":"Apprends l'espagnol"}}]}}}}"#;
        let t = &trouvailles(Plateforme::Instagram, &[corps.into()])[0];
        assert_eq!((t.brut.as_str(), t.compte.as_str()), ("Df6pm8UM-mC", "appendreespaglo"));
        assert_eq!((t.vues, t.likes, t.commentaires, t.abonnes), (Some(22), Some(1), Some(0), Some(260)));
        assert_eq!(t.duree_s, Some(64.505));
        assert_eq!(t.publiee, date(1_795_076_847));
        assert_eq!(t.legende.as_deref(), Some("Apprends l'espagnol"));
        assert_eq!(t.media.as_deref(), Some("https://cdn/v.mp4"));
    }

    /// L'id d'un fil (`média_compte`) donne la même date, et un id illisible n'en donne pas.
    #[test]
    fn date_de_l_id_du_fil() {
        let id = |v: &str| date_de_l_id(Some(&Value::String(v.into())));
        assert_eq!(id("4033719412274227586_64975230875"), date(1_795_076_847));
        assert_eq!(id("pas-un-id"), None);
        assert_eq!(date_de_l_id(None), None);
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
