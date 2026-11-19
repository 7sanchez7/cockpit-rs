//! Voir docs/sources/facebook.md.
use super::tiktok::{comptes_suivis, debut_legende, fusionner, publier, sans_echec};
use crate::collecte::http::{self, UA_NAVIGATEUR};
use crate::collecte::{miniatures, Contexte, Http, Requete, Source};
use crate::erreur::{format_err, Erreur, Res};
use crate::jour::{self, Date};
use crate::modeles::{Analyse, Compte, Part, Plateforme, Releve, ReleveCompte, Video};
use async_trait::async_trait;
use chrono::DateTime;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

pub struct Facebook;

const SUITE: &str = "https://business.facebook.com";
/// La requête du tableau Contenu de Business Suite (Statistiques → Contenu).
const REQUETE_TABLE: &str = "BizWebInsightsContentOrganicTableQueryRendererQuery";
pub const DOC_TABLE: &str = "40040115705602765";

pub struct Jetons {
    pub dtsg: String,
    pub lsd: String,
    pub rev: String,
}

#[derive(Default)]
pub struct Table {
    pub videos: Vec<Video>,
    pub releves: Vec<Releve>,
    pub analyses: HashMap<String, Analyse>,
    /// Id de vidéo, ou clé d'avatar, → URL de l'image.
    pub images: HashMap<String, String>,
}

#[async_trait]
impl Source for Facebook {
    fn id(&self) -> &'static str { "facebook" }
    fn nom(&self) -> &'static str { "Facebook" }
    fn cadence(&self) -> i64 { 3 * 3600 }

    async fn collecte(&self, ctx: &Contexte) -> Res<usize> {
        let store = &ctx.store;
        let maintenant = jour::maintenant();
        let comptes = comptes_suivis(ctx, Plateforme::Facebook, "aucune Page Facebook suivie, en ajouter dans Vidéos")?;
        let cookies = ctx.hote.cookies("facebook.com", "c_user")?;
        let cookie = http::entete_cookie(&cookies);
        let utilisateur = cookies.get("c_user").map(String::as_str).unwrap_or("");

        let mut echecs = vec![];
        let mut table = Table::default();
        let mut profils = vec![];
        for c in &comptes {
            let lu: Res<()> = async {
                let page_id = page_id(&c.handle, &cookie, &ctx.http).await?;
                let accueil = page(&format!("{SUITE}/latest/home?asset_id={page_id}"), &cookie, &ctx.http).await?;
                if let Some(abonnes) = nombre(r#""page_total_likes_or_followers":{"count":"#, &accueil) {
                    profils.push(ReleveCompte { compte: c.id(), at: maintenant, abonnes: Some(abonnes), ..Default::default() });
                }
                let reponse = appel(&requete(&page_id, utilisateur, &jetons(&accueil)?, &cookie), &ctx.http).await?;
                let t = self::table(&objets(&reponse)?, &c.business, &c.handle, &maintenant)?;
                table.videos.extend(t.videos);
                table.releves.extend(t.releves);
                fusionner(&mut table.analyses, t.analyses);
                fusionner(&mut table.images, t.images);
                Ok(())
            }
            .await;
            if let Err(e) = lu {
                echecs.push(format!("{} : {e}", c.handle));
            }
        }
        echecs.extend(miniatures::telecharger(&table.images, &ctx.http).await);
        echecs.extend(miniatures::extraire_premieres(&table.videos).await);

        store.lot(|| {
            publier(store, &table.videos, "facebook", |v| format!("Nouvelle vidéo sur Facebook {}{}", v.compte, debut_legende(&v.legende)))?;
            for r in &table.releves {
                store.releve(r)?;
            }
            for (id, a) in table.analyses.iter().filter(|(_, a)| !a.est_vide()) {
                store.analyse(id, &maintenant, a)?;
            }
            for p in &profils {
                store.releve_compte(p)?;
            }
            Ok(())
        })?;
        sans_echec(&echecs)?;
        Ok(table.videos.len() + table.releves.len() + table.analyses.len() + profils.len())
    }
}

// requêtes

/// Un appel à Facebook dont l'erreur ne cite pas le corps : il peut porter des jetons de session.
async fn appel(requete: &Requete, http: &Http) -> Res<Vec<u8>> {
    http.donnees(requete).await.map_err(|e| match e {
        Erreur::Http(code, _) => Erreur::Http(code, "facebook".into()),
        e => e,
    })
}

/// Une page HTML ouverte comme par le navigateur, avec la session de Safari.
async fn page(url: &str, cookie: &str, http: &Http) -> Res<String> {
    let r = Requete::get(url)
        .entete("User-Agent", UA_NAVIGATEUR)
        .entete("Cookie", cookie)
        .entete("Accept-Language", "fr-FR,fr;q=0.9")
        .entete("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
        .entete("Sec-Fetch-Dest", "document")
        .entete("Sec-Fetch-Mode", "navigate")
        .entete("Sec-Fetch-Site", "none")
        .entete("Sec-Fetch-User", "?1")
        .entete("Upgrade-Insecure-Requests", "1");
    Ok(String::from_utf8_lossy(&appel(&r, http).await?).into_owned())
}

/// L'id de la Page : le handle s'il n'a que des chiffres, sinon lu sur sa page publique.
async fn page_id(handle: &str, cookie: &str, http: &Http) -> Res<String> {
    if handle.chars().all(char::is_numeric) {
        return Ok(handle.into());
    }
    let html = page(&format!("https://www.facebook.com/{handle}"), cookie, http).await?;
    texte(r#""delegate_page_id":""#, &html)
        .or_else(|| nombre("asset_id=", &html).map(|n| n.to_string()))
        .ok_or_else(|| format_err(format!("Page {handle} introuvable, ou session Facebook de Safari expirée")))
}

/// La requête du tableau Contenu : les publications des 90 derniers jours et leurs statistiques.
pub fn requete(page: &str, utilisateur: &str, jetons: &Jetons, cookie: &str) -> Requete {
    // clés dans l'ordre trié, comme l'app Swift les envoie
    let variables = json!({
        "__relay_internal__pv__shouldFetchAMPostPreviewrelayprovider": false,
        "__relay_internal__pv__shouldFetchAssociatedAdsInforelayprovider": false,
        "__relay_internal__pv__shouldFetchEPAPostVariantImagerelayprovider": false,
        "callerID": "BIZWEB_INSIGHTS_ORGANIC_CONTENT",
        "count": 100,
        "ids": [page],
        "initialFilters": [],
        "skip": false,
        "timeRange": {"type": "LAST_90D"},
    })
    .to_string();
    Requete::new(format!("{SUITE}/api/graphql/"), "POST")
        .entete("User-Agent", UA_NAVIGATEUR)
        .entete("Cookie", cookie)
        .entete("Accept", "*/*")
        .entete("Origin", SUITE)
        .entete("Referer", &format!("{SUITE}/latest/insights/content?asset_id={page}"))
        .entete("Sec-Fetch-Dest", "empty")
        .entete("Sec-Fetch-Mode", "cors")
        .entete("Sec-Fetch-Site", "same-origin")
        .entete("X-FB-Friendly-Name", REQUETE_TABLE)
        .entete("X-FB-LSD", &jetons.lsd)
        .entete("X-ASBD-ID", "359341")
        .formulaire(&[
            ("av", utilisateur), ("__user", utilisateur), ("__a", "1"), ("__comet_req", "11"), ("fb_dtsg", &jetons.dtsg),
            ("lsd", &jetons.lsd), ("__rev", &jetons.rev), ("fb_api_caller_class", "RelayModern"),
            ("fb_api_req_friendly_name", REQUETE_TABLE), ("variables", &variables), ("server_timestamps", "true"),
            ("doc_id", DOC_TABLE),
        ])
}

// décodage

/// Les jetons de la session, lus dans une page de Business Suite ; jamais journalisés.
pub fn jetons(html: &str) -> Res<Jetons> {
    let dtsg = texte(r#"["DTSGInitialData",[],{"token":""#, html).or_else(|| texte(r#""dtsg":{"token":""#, html));
    let lsd = texte(r#"["LSD",[],{"token":""#, html);
    let rev = nombre(r#""server_revision":"#, html).or_else(|| nombre(r#""__spin_r":"#, html));
    match (dtsg, lsd, rev) {
        (Some(dtsg), Some(lsd), Some(rev)) => Ok(Jetons { dtsg, lsd, rev: rev.to_string() }),
        _ => Err(Erreur::NonConfigure("session Facebook de Safari expirée, se reconnecter sur business.facebook.com dans Safari".into())),
    }
}

/// Le texte entre `marque` et le guillemet suivant.
pub fn texte(marque: &str, html: &str) -> Option<String> {
    let reste = &html[html.find(marque)? + marque.len()..];
    let fin = reste.find('"')?;
    (fin > 0).then(|| reste[..fin].to_string())
}

/// Le nombre écrit juste après `marque`.
pub fn nombre(marque: &str, html: &str) -> Option<i64> {
    let reste = &html[html.find(marque)? + marque.len()..];
    let fin = reste.find(|c: char| !c.is_ascii_digit()).unwrap_or(reste.len());
    reste[..fin].parse().ok()
}

/// Les objets JSON mis bout à bout d'une réponse graphql, après le préfixe `for (;;);` : un par morceau livré.
pub fn objets(reponse: &[u8]) -> Res<Vec<Map<String, Value>>> {
    let mut objets = vec![];
    let (mut profondeur, mut debut, mut chaine, mut echappe) = (0, 0, false, false);
    for (i, &c) in reponse.iter().enumerate() {
        if chaine {
            if echappe {
                echappe = false;
            } else if c == b'\\' {
                echappe = true;
            } else if c == b'"' {
                chaine = false;
            }
            continue;
        }
        if c == b'"' && profondeur > 0 {
            chaine = true;
        } else if c == b'{' {
            if profondeur == 0 {
                debut = i;
            }
            profondeur += 1;
        } else if c == b'}' && profondeur > 0 {
            profondeur -= 1;
            if profondeur > 0 {
                continue;
            }
            let objet = serde_json::from_slice(&reponse[debut..=i]).map_err(|_| format_err("graphql : morceau illisible"))?;
            objets.push(objet);
        }
    }
    Ok(objets)
}

/// Les vidéos du tableau Contenu, avec leurs compteurs et leur analyse.
pub fn table(objets: &[Map<String, Value>], business: &str, compte: &str, at: &Date) -> Res<Table> {
    let erreurs: Vec<&str> = objets.iter()
        .flat_map(|o| o.get("errors").and_then(Value::as_array).into_iter().flatten())
        .filter_map(|e| e.get("message").and_then(Value::as_str))
        .collect();
    let lignes: Vec<&Map<String, Value>> = objets.iter()
        .flat_map(|o| o.get("data").and_then(|d| d.get("tofu_unified_table")).and_then(|t| t.get("content"))
            .and_then(|c| c.get("edges")).and_then(Value::as_array).into_iter().flatten())
        .filter_map(|e| e.get("node").and_then(Value::as_object))
        .collect();
    if let (true, Some(e)) = (lignes.is_empty(), erreurs.first()) {
        return Err(format_err(format!("graphql : {e}")));
    }

    let mut t = Table::default();
    let vide = Map::new();
    for n in lignes {
        let Some(brut) = n.get("row_id").and_then(Value::as_str) else { continue };
        let entete = n.get("header");
        let champs = n.get("fields").and_then(Value::as_object).unwrap_or(&vide);
        let m = |cle: &str| -> Option<f64> {
            let r = champs.get(cle)?.get("renderer")?.as_object()?;
            let type_ = r.get("__typename").and_then(Value::as_str).unwrap_or("");
            if type_.to_lowercase().contains("error") {
                return None;
            }
            r.get("value_for_sort")?.as_f64()
        };
        let video = entete.and_then(|e| e.get("content_type")).and_then(Value::as_str).unwrap_or("").contains("VIDEO");
        if !video && m("video_play_time").is_none() {
            continue;
        }
        let info = entete.and_then(|e| e.get("entity")).and_then(|e| e.get("entity_info"));
        let lire = |cle: &str| info.and_then(|i| i.get(cle));
        let id = Plateforme::Facebook.id_video(brut);
        t.videos.push(Video {
            id: id.clone(),
            business: business.into(),
            compte: compte.into(),
            legende: lire("title").and_then(Value::as_str).map(String::from),
            publiee_le: lire("created_at").and_then(Value::as_f64).and_then(|s| DateTime::from_timestamp_millis((s * 1000.0) as i64)),
            duree_s: None,
            url: Some(Plateforme::Facebook.lien(brut, compte)),
            plateforme: Plateforme::Facebook,
        });
        if let Some(image) = lire("image_source").and_then(|i| i.get("uri")).and_then(Value::as_str) {
            t.images.insert(id.clone(), image.into());
        }
        let avatar = lire("owner").and_then(|o| o.get("entity_info")).and_then(|i| i.get("image_source")).and_then(|i| i.get("uri")).and_then(Value::as_str);
        if let Some(avatar) = avatar {
            t.images.insert(miniatures::avatar(&Compte::id_de(Plateforme::Facebook, compte)), avatar.into());
        }
        let Some(vues) = m("views").map(|v| v as i64) else { continue };
        let entier = |cle: &str| m(cle).map(|v| v as i64);
        t.releves.push(Releve {
            video: id.clone(),
            at: *at,
            vues,
            likes: entier("net_reactions"),
            commentaires: entier("net_comments"),
            partages: entier("shares"),
            enregistrements: entier("net_saves"),
            watch_moyen_s: m("video_average_play_time").map(|v| v / 1000.0),
            abonnes: entier("new_follows"),
            ..Default::default()
        });
        let jalons = [("3 s", "video_three_second_views"), ("1 min", "video_one_minute_views"), ("ThruPlay", "video_thruplays")];
        t.analyses.insert(id, Analyse {
            watch_total_s: m("video_play_time").map(|v| v / 1000.0),
            spectateurs: entier("viewers"),
            jalons: if vues > 0 {
                jalons.iter().filter_map(|(nom, cle)| Some(Part { nom: (*nom).into(), part: m(cle)? / vues as f64 })).collect()
            } else {
                vec![]
            },
            ..Default::default()
        });
    }
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn table_facebook() {
        let at = Utc.timestamp_opt(1794527444, 0).unwrap();
        let reponse = br#"for (;;);{"data": {"tofu_unified_table": {"content": {"edges": [
              {"node": {"row_id": "123",
                "header": {"content_type": "VIDEO", "entity": {"entity_info": {"title": "Reel {test}", "created_at": 1794500000,
                  "image_source": {"uri": "https://x/i.jpg"}, "owner": {"entity_info": {"image_source": {"uri": "https://x/a.jpg"}}}}}},
                "fields": {
                  "views": {"renderer": {"value_for_sort": 1000}}, "net_reactions": {"renderer": {"value_for_sort": 50}},
                  "net_comments": {"renderer": {"value_for_sort": 2}}, "shares": {"renderer": {"value_for_sort": 3}},
                  "net_saves": {"renderer": {"value_for_sort": 4}}, "new_follows": {"renderer": {"value_for_sort": 7}},
                  "video_average_play_time": {"renderer": {"value_for_sort": 5970}},
                  "video_play_time": {"renderer": {"value_for_sort": 5970000}}, "viewers": {"renderer": {"value_for_sort": 800}},
                  "video_three_second_views": {"renderer": {"value_for_sort": 400}},
                  "video_one_minute_views": {"renderer": {"value_for_sort": 10}},
                  "video_thruplays": {"renderer": {"__typename": "BizWebInsightsErrorRenderer", "value_for_sort": 9}}}}},
              {"node": {"row_id": "456", "header": {"content_type": "PHOTO"}, "fields": {"views": {"renderer": {"value_for_sort": 80}}}}}
            ]}}}}{"label": "suite", "data": {"texte": "a } b \" {"}}"#;
        let o = objets(reponse).unwrap();
        assert_eq!(o.len(), 2);
        let t = table(&o, "roleo", "holaestony", &at).unwrap();
        assert_eq!(t.videos, vec![Video {
            id: "facebook:123".into(), business: "roleo".into(), compte: "holaestony".into(), legende: Some("Reel {test}".into()),
            publiee_le: Some(Utc.timestamp_opt(1794500000, 0).unwrap()), duree_s: None,
            url: Some("https://www.facebook.com/123".into()), plateforme: Plateforme::Facebook,
        }]);
        assert_eq!(t.releves, vec![Releve {
            video: "facebook:123".into(), at, vues: 1000, likes: Some(50), commentaires: Some(2), partages: Some(3),
            enregistrements: Some(4), watch_moyen_s: Some(5.97), abonnes: Some(7), ..Default::default()
        }]);
        // la case en erreur (ThruPlay) est absente, pas à zéro
        assert_eq!(t.analyses, HashMap::from([("facebook:123".to_string(), Analyse {
            watch_total_s: Some(5970.0), spectateurs: Some(800),
            jalons: vec![Part { nom: "3 s".into(), part: 0.4 }, Part { nom: "1 min".into(), part: 0.01 }],
            ..Default::default()
        })]));
        assert_eq!(t.images, HashMap::from([
            ("facebook:123".to_string(), "https://x/i.jpg".to_string()),
            ("avatar:facebook:holaestony".to_string(), "https://x/a.jpg".to_string()),
        ]));

        assert!(objets(br#"for (;;);{"a": }"#).is_err());
        assert!(table(&objets(br#"{"errors": [{"message": "boom"}]}"#).unwrap(), "roleo", "x", &at).is_err());
    }

    #[test]
    fn jetons_facebook() {
        let html = r#"<script>["DTSGInitialData",[],{"token":"dtsg-factice"},1]["LSD",[],{"token":"lsd-factice"},2]
            {"server_revision":1029384,"x":1}{"page_total_likes_or_followers":{"count":10465}}</script>"#;
        let j = jetons(html).unwrap();
        assert!(j.dtsg == "dtsg-factice" && j.lsd == "lsd-factice" && j.rev == "1029384");
        assert_eq!(nombre(r#""page_total_likes_or_followers":{"count":"#, html), Some(10465));
        assert_eq!(texte(r#"["LSD",[],{"token":""#, html).as_deref(), Some("lsd-factice"));
        assert!(matches!(jetons("<html>connexion</html>"), Err(Erreur::NonConfigure(_))));

        let r = requete("99", "1", &j, "c_user=1");
        let corps = String::from_utf8(r.corps.clone().unwrap_or_default()).unwrap();
        assert_eq!(r.methode, "POST");
        assert!(corps.contains("fb_dtsg=dtsg-factice") && corps.contains(&format!("doc_id={DOC_TABLE}")));
        assert!(!corps.contains('{'));
    }
}
