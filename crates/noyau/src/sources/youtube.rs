//! Voir docs/sources/youtube.md.
use super::tiktok::{comptes_suivis, debut_legende, fusionner, publier, sans_echec};
use crate::collecte::{miniatures, ytdlp, Contexte, Source};
use crate::erreur::Res;
use crate::jour::{self, Date};
use crate::modeles::{Plateforme, ReleveCompte};
use crate::p;
use async_trait::async_trait;
use chrono::Duration;
use std::collections::HashMap;

pub struct YouTube;

/// La liste à plat n'a ni date ni likes : on ouvre les vidéos inconnues et les récentes, au plus tant par passage.
const OUVERTES_MAX: usize = 20;
const RECENTE_S: i64 = 7 * 86_400;

#[async_trait]
impl Source for YouTube {
    fn id(&self) -> &'static str { "youtube" }
    fn nom(&self) -> &'static str { "YouTube" }
    fn cadence(&self) -> i64 { 3600 }

    async fn collecte(&self, ctx: &Contexte) -> Res<usize> {
        let store = &ctx.store;
        let maintenant = jour::maintenant();
        let comptes = comptes_suivis(ctx, Plateforme::Youtube, "aucune chaîne YouTube suivie, en ajouter dans Vidéos")?;
        let mut publiees: HashMap<String, Option<Date>> = HashMap::new();
        for l in store.db.query("select id, publiee_le from video where plateforme = 'youtube'", p![])? {
            if let Some(id) = l.texte("id") {
                publiees.entry(id).or_insert(l.date("publiee_le"));
            }
        }

        let mut echecs = vec![];
        let mut liste = ytdlp::Liste::default();
        let mut profils = vec![];
        for c in &comptes {
            let lu: Res<()> = async {
                let sortie = ytdlp::liste(&format!("{}/shorts", c.plateforme.profil(&c.handle)), None).await?;
                let l = ytdlp::lire_liste(&sortie, Plateforme::Youtube, &c.business, &c.handle, &maintenant)?;
                let a_ouvrir: Vec<String> = l.videos.iter()
                    .filter(|v| match publiees.get(&v.id).copied().flatten() {
                        Some(publiee) => maintenant - publiee < Duration::seconds(RECENTE_S),
                        None => true,
                    })
                    .take(OUVERTES_MAX)
                    .filter_map(|v| v.url.clone())
                    .collect();
                profils.push(ReleveCompte {
                    compte: c.id(), at: maintenant, abonnes: l.abonnes, videos: Some(l.videos.len() as i64), ..Default::default()
                });
                liste.videos.extend(l.videos);
                liste.releves.extend(l.releves);
                fusionner(&mut liste.images, l.images);
                if let Some(a) = l.avatar {
                    liste.images.insert(miniatures::avatar(&c.id()), a);
                }
                if a_ouvrir.is_empty() {
                    return Ok(());
                }
                // après la liste : ses champs complets remplacent ceux de la liste à plat
                let d = ytdlp::lire_details(&ytdlp::details(&a_ouvrir).await?, Plateforme::Youtube, &c.business, &c.handle, &maintenant)?;
                liste.videos.extend(d.videos);
                liste.releves.extend(d.releves);
                Ok(())
            }
            .await;
            if let Err(e) = lu {
                echecs.push(format!("@{} : {e}", c.handle));
            }
        }
        echecs.extend(miniatures::telecharger(&liste.images, &ctx.http).await);
        echecs.extend(miniatures::extraire_premieres(&liste.videos).await);

        store.lot(|| {
            publier(store, &liste.videos, "youtube", |v| format!("Nouvelle vidéo sur YouTube @{}{}", v.compte, debut_legende(&v.legende)))?;
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

#[cfg(test)]
mod tests {
    use crate::collecte::ytdlp;
    use crate::modeles::{Plateforme, Releve, Video};
    use chrono::{TimeZone, Utc};

    #[test]
    fn liste_youtube() {
        let at = Utc.timestamp_opt(1794527444, 0).unwrap();
        let l = ytdlp::lire_liste(br#"{"id": "UC1", "channel_follower_count": 403,
             "thumbnails": [{"id": "banner_uncropped", "url": "https://x/b.jpg"}, {"id": "avatar_uncropped", "url": "https://x/av.jpg"}],
             "entries": [{"id": "abc", "title": "Le short", "view_count": 1200, "url": "https://www.youtube.com/shorts/abc"}]}"#,
            Plateforme::Youtube, "roleo", "holaestony", &at).unwrap();
        assert!(l.abonnes == Some(403) && l.avatar.as_deref() == Some("https://x/av.jpg"));
        assert_eq!(l.videos, vec![Video {
            id: "youtube:abc".into(), business: "roleo".into(), compte: "holaestony".into(), legende: Some("Le short".into()),
            publiee_le: None, duree_s: None, url: Some("https://www.youtube.com/shorts/abc".into()), plateforme: Plateforme::Youtube,
        }]);
        assert_eq!(l.releves, vec![Releve { video: "youtube:abc".into(), at, vues: 1200, ..Default::default() }]);
    }
}
