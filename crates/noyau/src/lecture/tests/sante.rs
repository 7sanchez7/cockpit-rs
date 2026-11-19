use super::{d, lectures};
use crate::essai::store;
use crate::modeles::{EtatPassage, Passage};

#[test]
fn dernier_succes_ignore_les_echecs() {
    let s = store();
    let (ok, panne) = (d("2026-11-13T10:00:00Z"), d("2026-11-13T11:00:00Z"));
    let passage = |source: &str, at, etat, erreur: Option<&str>, lignes| Passage {
        source: source.into(), debut: at, fin: at, etat, erreur: erreur.map(str::to_string), lignes,
    };
    s.journal(&passage("tiktok", ok, EtatPassage::Ok, None, 3)).unwrap();
    s.journal(&passage("tiktok", panne, EtatPassage::Erreur, Some("HTTP 500"), 0)).unwrap();
    s.journal(&passage("asc", panne, EtatPassage::NonConfigure, Some("pas de clé"), 0)).unwrap();
    let succes = lectures(&s).derniers_succes().unwrap();
    assert_eq!(succes.get("tiktok"), Some(&ok));
    assert_eq!(succes.get("asc"), None);
}
