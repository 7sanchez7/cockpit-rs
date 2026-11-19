//! Voir docs/sources/instagram.md.
use super::tiktok::{comptes_suivis, debut_legende, fusionner, publier, sans_echec};
use crate::collecte::http::{self, UA_SAFARI};
use crate::collecte::{miniatures, safari, ytdlp, Contexte, Hote, Http, Requete, Source};
use crate::erreur::{format_err, Erreur, Res};
use crate::jour::{self, Date};
use crate::modeles::{Compte, Plateforme, Releve, ReleveCompte, Video};
use async_trait::async_trait;
use chrono::DateTime;
use serde_json::{Map, Value};

pub struct Instagram;

const SITE: &str = "https://www.instagram.com";
/// L'id de l'app web d'Instagram, que l'API interne exige.
const APP_WEB: &str = "936619743392459";

/// La session refusée, avec ce qu'Instagram a renvoyé, sans jamais citer le corps.
fn refusee(raison: &str) -> Erreur {
    Erreur::NonConfigure(format!("session Instagram de Safari refusée ({raison}), se reconnecter sur instagram.com dans Safari"))
}

pub struct Profil {
    pub uid: String,
    pub releve: ReleveCompte,
    pub avatar: Option<String>,
}

#[async_trait]
impl Source for Instagram {
    fn id(&self) -> &'static str { "instagram" }
    fn nom(&self) -> &'static str { "Instagram" }
    fn cadence(&self) -> i64 { 3 * 3600 }
    /// Des appels refusés toutes les 10 minutes entretiennent le blocage d'Instagram.
    fn reprise(&self) -> i64 { 3 * 3600 }

    async fn collecte(&self, ctx: &Contexte) -> Res<usize> {
        let store = &ctx.store;
        let maintenant = jour::maintenant();
        let comptes = comptes_suivis(ctx, Plateforme::Instagram, "aucun compte Instagram suivi, en ajouter dans Vidéos")?;
        let cookies = ctx.hote.cookies("instagram.com", "sessionid")?;
        let cookie = safari::entete(&cookies);
        let csrf = cookies.get("csrftoken").map(String::as_str).unwrap_or("");
        let lire = |chemin: String, c: &Compte| {
            let r = Requete::get(format!("{SITE}{chemin}"))
                .entete("User-Agent", UA_SAFARI)
                .entete("Cookie", &cookie)
                .entete("X-IG-App-ID", APP_WEB)
                .entete("X-CSRFToken", csrf)
                .entete("Referer", &c.plateforme.profil(&c.handle))
                .entete("X-Requested-With", "XMLHttpRequest")
                .entete("X-ASBD-ID", "359341")
                .entete("Accept", "*/*")
                .entete("Accept-Language", "fr-FR,fr;q=0.9");
            async move { appel(&r, &ctx.http, &*ctx.hote).await }
        };

        let mut echecs = vec![];
        let mut liste = ytdlp::Liste::default();
        let mut profils = vec![];
        for c in &comptes {
            let lu: Res<()> = async {
                let p = profil(&lire(format!("/api/v1/users/web_profile_info/?username={}", c.handle), c).await?, c, &maintenant)?;
                profils.push(p.releve);
                if let Some(a) = p.avatar {
                    liste.images.insert(miniatures::avatar(&c.id()), a);
                }
                let l = fil(&lire(format!("/api/v1/feed/user/{}/?count=33", p.uid), c).await?, &c.business, &c.handle, &maintenant)?;
                liste.videos.extend(l.videos);
                liste.releves.extend(l.releves);
                fusionner(&mut liste.images, l.images);
                Ok(())
            }
            .await;
            match lu {
                Ok(()) => {}
                Err(e @ Erreur::NonConfigure(_)) => return Err(e),
                Err(e) => echecs.push(format!("@{} : {e}", c.handle)),
            }
        }
        echecs.extend(miniatures::telecharger(&liste.images, &ctx.http).await);

        store.lot(|| {
            publier(store, &liste.videos, "instagram", |v| format!("Nouvelle vidéo sur Instagram @{}{}", v.compte, debut_legende(&v.legende)))?;
            for r in &liste.releves {
                store.releve(r)?;
            }
            for p in &profils {
                store.releve_compte(p)?;
            }
            Ok(())
        })?;
        sans_echec(&echecs)?;
        Ok(liste.videos.len() + liste.releves.len() + profils.len())
    }
}

/// Un appel à l'API interne : une session refusée devient « reconnecte-toi », et l'erreur ne cite jamais le corps.
/// Une session périmée reçoit un 404 sans JSON, comme un 401 ou un 403.
async fn appel(requete: &Requete, http: &Http, hote: &dyn Hote) -> Res<Vec<u8>> {
    match http.donnees(requete).await {
        Err(Erreur::Http(code, debut)) if [401, 403, 404].contains(&code) => {
            // la page d'erreur dit si Instagram a reconnu la session
            let session = if debut.contains("not-logged-in") {
                format!(", session pas reconnue ; {}", hote.provenance("instagram.com", "sessionid"))
            } else {
                String::new()
            };
            Err(refusee(&format!("HTTP {code}{session}")))
        }
        Err(Erreur::Http(code, _)) => Err(Erreur::Http(code, "instagram".into())),
        r => r,
    }
}

/// `web_profile_info` : l'id du compte, ses abonnés, son nombre de publications et sa photo.
pub fn profil(reponse: &[u8], compte: &Compte, at: &Date) -> Res<Profil> {
    let o = objet(reponse)?;
    let u = o.get("data").and_then(|d| d.get("user")).and_then(Value::as_object);
    let Some((u, uid)) = u.and_then(|u| Some((u, u.get("id")?.as_str()?))) else {
        return Err(format_err(format!("Instagram @{} : profil introuvable", compte.handle)));
    };
    let compteur = |cle: &str| u.get(cle).and_then(|c| c.get("count")).and_then(Value::as_i64);
    Ok(Profil {
        uid: uid.into(),
        releve: ReleveCompte {
            compte: compte.id(),
            at: *at,
            abonnes: compteur("edge_followed_by"),
            videos: compteur("edge_owner_to_timeline_media"),
            ..Default::default()
        },
        avatar: u.get("profile_pic_url_hd").or(u.get("profile_pic_url")).and_then(Value::as_str).map(String::from),
    })
}

/// `feed/user` : les 33 dernières publications ; seules les vidéos (media_type 2) sont gardées.
pub fn fil(reponse: &[u8], business: &str, compte: &str, at: &Date) -> Res<ytdlp::Liste> {
    let o = objet(reponse)?;
    let Some(items) = o.get("items").and_then(Value::as_array) else {
        return Err(format_err(format!("Instagram @{compte} : pas de publications")));
    };
    let mut l = ytdlp::Liste::default();
    for i in items.iter().filter(|i| i.get("media_type").and_then(Value::as_i64) == Some(2)) {
        let Some(code) = i.get("code").and_then(Value::as_str) else { continue };
        let id = Plateforme::Instagram.id_video(code);
        l.videos.push(Video {
            id: id.clone(),
            business: business.into(),
            compte: compte.into(),
            legende: i.get("caption").and_then(|c| c.get("text")).and_then(Value::as_str).map(String::from),
            publiee_le: i.get("taken_at").and_then(Value::as_f64).and_then(|s| DateTime::from_timestamp_millis((s * 1000.0) as i64)),
            duree_s: i.get("video_duration").and_then(Value::as_f64),
            url: Some(Plateforme::Instagram.lien(code, compte)),
            plateforme: Plateforme::Instagram,
        });
        // ig_play_count : les lectures sur Instagram seul, sans celles du partage Facebook
        let entier = |k: &str| i.get(k).and_then(Value::as_i64);
        if let Some(vues) = i.get("ig_play_count").or(i.get("play_count")).or(i.get("view_count")).and_then(Value::as_i64) {
            l.releves.push(Releve {
                video: id.clone(),
                at: *at,
                vues,
                likes: entier("like_count"),
                commentaires: entier("comment_count"),
                ..Default::default()
            });
        }
        let image = i.get("image_versions2").and_then(|v| v.get("candidates")).and_then(|c| c.get(0)).and_then(|c| c.get("url")).and_then(Value::as_str);
        if let Some(image) = image {
            l.images.insert(id, image.into());
        }
    }
    Ok(l)
}

/// Une réponse JSON de l'API ; la page de connexion à la place veut dire une session expirée.
fn objet(reponse: &[u8]) -> Res<Map<String, Value>> {
    let o = http::objet(reponse).map_err(|_| refusee("réponse pas en JSON"))?;
    if o.get("require_login").and_then(Value::as_bool) == Some(true) {
        return Err(refusee("require_login"));
    }
    Ok(o)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn demande_connexion<T>(r: Res<T>) -> bool { matches!(r, Err(Erreur::NonConfigure(_))) }

    #[test]
    fn profil_et_fil_instagram() {
        let at = Utc.timestamp_opt(1794527444, 0).unwrap();
        let compte = Compte::nouveau("roleo", Plateforme::Instagram, "holaestony");
        let p = profil(br#"{"data": {"user": {"id": "42", "edge_followed_by": {"count": 1200}, "edge_owner_to_timeline_media": {"count": 87},
              "profile_pic_url": "https://x/p.jpg", "profile_pic_url_hd": "https://x/hd.jpg"}}, "status": "ok"}"#, &compte, &at).unwrap();
        assert_eq!(p.uid, "42");
        assert_eq!(p.releve, ReleveCompte { compte: "instagram:holaestony".into(), at, abonnes: Some(1200), videos: Some(87), likes: None });
        assert_eq!(p.avatar.as_deref(), Some("https://x/hd.jpg"));

        let f = fil(br#"{"items": [
              {"media_type": 2, "code": "ABC", "caption": {"text": "Hola"}, "taken_at": 1794500000, "video_duration": 21.5,
               "ig_play_count": 5000, "play_count": 9000, "like_count": 100, "comment_count": 5,
               "image_versions2": {"candidates": [{"url": "https://x/c.jpg"}]}},
              {"media_type": 1, "code": "PHOTO"},
              {"media_type": 2, "code": "DEF", "play_count": 300}]}"#, "roleo", "holaestony", &at).unwrap();
        assert_eq!(f.videos.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["instagram:ABC", "instagram:DEF"]);
        assert_eq!(f.videos[0], Video {
            id: "instagram:ABC".into(), business: "roleo".into(), compte: "holaestony".into(), legende: Some("Hola".into()),
            publiee_le: Some(Utc.timestamp_opt(1794500000, 0).unwrap()), duree_s: Some(21.5),
            url: Some("https://www.instagram.com/reel/ABC/".into()), plateforme: Plateforme::Instagram,
        });
        // ig_play_count passe avant play_count, qui compte aussi le partage Facebook
        assert_eq!(f.releves, vec![
            Releve { video: "instagram:ABC".into(), at, vues: 5000, likes: Some(100), commentaires: Some(5), ..Default::default() },
            Releve { video: "instagram:DEF".into(), at, vues: 300, ..Default::default() },
        ]);
        assert_eq!(f.images, std::collections::HashMap::from([("instagram:ABC".to_string(), "https://x/c.jpg".to_string())]));

        // page de connexion à la place du JSON : session à refaire
        assert!(demande_connexion(profil(b"<html>", &compte, &at)));
        assert!(demande_connexion(fil(br#"{"require_login": true}"#, "roleo", "x", &at)));
        assert!(fil(br#"{"status": "ok"}"#, "roleo", "x", &at).is_err());
    }
}
