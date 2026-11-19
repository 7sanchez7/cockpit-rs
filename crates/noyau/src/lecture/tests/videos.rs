use super::{d, lectures, maintenant, video};
use crate::business::Business;
use crate::collecte::miniatures;
use crate::essai::store;
use crate::lecture::Lectures;
use crate::modeles::{Plateforme, Releve, ReleveCompte};
use crate::store::Store;
use std::collections::HashSet;

fn publie(s: &Store, id: &str, compte: &str, publiee: &str) { s.video(&video(id, compte, id, Some(publiee))).unwrap(); }

#[test]
fn comptes_avec_abonnes_et_gain() {
    let s = store();
    publie(&s, "1", "holaestony", "2026-11-10T10:00:00Z");
    publie(&s, "2", "holaestony", "2026-11-11T10:00:00Z");
    publie(&s, "3", "tonyolingo", "2026-11-12T10:00:00Z");
    for (at, n) in [("2026-11-05T10:00:00Z", 1000), ("2026-11-13T10:00:00Z", 1250)] {
        s.releve_compte(&ReleveCompte { compte: "tiktok:holaestony".into(), at: d(at), abonnes: Some(n), videos: None, likes: None }).unwrap();
    }
    let c = lectures(&s).comptes().unwrap();
    assert_eq!(c.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
               ["facebook:holaestony", "instagram:holaestony", "tiktok:holaestony", "tiktok:tonyolingo", "youtube:holaestony"]);
    let tiktok = c.iter().find(|c| c.id == "tiktok:holaestony").unwrap();
    assert_eq!(tiktok.videos, 2);
    assert_eq!(tiktok.abonnes, Some(1250));
    assert_eq!(tiktok.gain7j, Some(250));
    assert_eq!(tiktok.serie.last(), Some(&Some(1250.0)));
    assert_eq!(c.iter().find(|c| c.id == "tiktok:tonyolingo").unwrap().abonnes, None);
}

#[test]
fn compte_retire_cache_ses_videos() {
    let s = store();
    publie(&s, "1", "holaestony", "2026-11-10T10:00:00Z");
    publie(&s, "2", "tonyolingo", "2026-11-11T10:00:00Z");
    s.releve(&Releve { video: "1".into(), at: maintenant(), vues: 100, ..Default::default() }).unwrap();
    s.releve(&Releve { video: "2".into(), at: maintenant(), vues: 50, ..Default::default() }).unwrap();
    let ids = |s: &Store| lectures(s).videos(1000).unwrap().into_iter().map(|v| v.id).collect::<Vec<_>>();
    assert_eq!(ids(&s).into_iter().collect::<HashSet<_>>(), HashSet::from(["1".to_string(), "2".to_string()]));
    s.retirer_compte("tiktok:tonyolingo").unwrap();
    assert_eq!(ids(&s), ["1"]);
    assert!(!lectures(&s).comptes().unwrap().iter().any(|c| c.handle == "tonyolingo"));
}

#[test]
fn engagement_et_conversion() {
    let s = store();
    publie(&s, "1", "holaestony", "2026-11-10T10:00:00Z");
    s.releve(&Releve {
        video: "1".into(), at: maintenant(), vues: 2000, likes: Some(100), commentaires: Some(10), partages: Some(30),
        enregistrements: Some(20), abonnes: Some(8), ..Default::default()
    }).unwrap();
    let v = lectures(&s).videos(1000).unwrap().remove(0);
    assert_eq!(v.engagement(), Some(0.08));
    assert_eq!(v.conversion(), Some(4.0));
}

#[test]
fn publications_par_jour_de_paris() {
    let s = store();
    // 23 h 30 UTC le 11 = 0 h 30 le 12 à Paris
    publie(&s, "1", "holaestony", "2026-11-11T23:30:00Z");
    publie(&s, "2", "holaestony", "2026-11-12T09:00:00Z");
    publie(&s, "3", "tonyolingo", "2026-11-13T09:00:00Z");
    publie(&s, "4", "holaestony", "2026-11-01T09:00:00Z");
    assert_eq!(lectures(&s).publications(3, "2026-11-13").unwrap(), vec![0.0, 2.0, 1.0]);
}

#[test]
fn publications_filtrees_sur_le_business() {
    let s = store();
    let app2: Business = serde_json::from_str(r#"{"id": "app2", "nom": "App 2", "lettre": "A", "teinte": "ciel"}"#).unwrap();
    s.enregistrer(&app2).unwrap();
    publie(&s, "1", "holaestony", "2026-11-13T09:00:00Z");
    let seul: Vec<Business> = s.businesses().unwrap().into_iter().filter(|b| b.id == "app2").collect();
    let l = Lectures { maintenant: maintenant(), ..Lectures::new(&s.db, seul) };
    assert_eq!(l.publications(1, "2026-11-13").unwrap(), vec![0.0]);
    assert!(l.comptes().unwrap().is_empty());
}

#[test]
fn images_a_precharger() {
    let s = store();
    publie(&s, "1", "holaestony", "2026-11-10T10:00:00Z");
    publie(&s, "2", "holaestony", "2026-11-12T10:00:00Z");
    let mut yt = video("youtube:abc", "holaestony", "yt", Some("2026-11-11T10:00:00Z"));
    yt.plateforme = Plateforme::Youtube;
    s.video(&yt).unwrap();
    let images = lectures(&s).images(2).unwrap();
    assert_eq!(images.first(), Some(&vec![miniatures::icone("6770304361")]));
    assert!(images.contains(&vec![miniatures::avatar("tiktok:holaestony")]));
    // les plus récentes d'abord, la première image avant la miniature
    assert_eq!(&images[images.len() - 2..], [vec!["2".to_string()], vec![miniatures::premiere("youtube:abc"), "youtube:abc".to_string()]]);
}
