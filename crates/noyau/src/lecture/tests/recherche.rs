use super::{d, lectures, video};
use crate::essai::store;
use crate::lecture::recherche::{plier, Genre};
use crate::modeles::{Appli, Releve};

#[test]
fn plier_sans_accents_ni_casse() {
    assert_eq!(plier("Éducation Ñandú Ça"), "education nandu ca");
}

#[test]
fn videos_apps_et_taches_trouvees() {
    let s = store();
    s.video(&video("1", "holaestony", "Les 4 insultes\n#espagnol", Some("2026-11-01T10:00:00Z"))).unwrap();
    s.releve(&Releve { video: "1".into(), at: d("2026-11-13T13:00:00Z"), vues: 48_200, ..Default::default() }).unwrap();
    s.app(&Appli { id: "42".into(), nom: "Insulte Pro".into(), ..Default::default() }).unwrap();
    s.suivre("42", true).unwrap();
    s.ajouter_tache("Répondre à l'insulté", None, None).unwrap();
    let l = lectures(&s);
    let r = l.recherche(" INSULTÉ ").unwrap();
    assert_eq!(r.iter().map(|t| t.genre).collect::<Vec<_>>(), [Genre::Video, Genre::App, Genre::Tache]);
    assert_eq!(r[0].detail, "@holaestony, 48\u{202F}200 vues");
    assert_eq!(r[1].detail, "App Store");
    assert_eq!(r[2].detail, "SAV");
    assert!(l.recherche("  ").unwrap().is_empty());
    assert_eq!(l.recherche("holaestony").unwrap().len(), 1);
}
