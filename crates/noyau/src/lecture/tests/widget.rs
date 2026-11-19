use super::lectures;
use crate::essai::store;
use crate::lecture::format;
use crate::lecture::widget::ResumeWidget;
use crate::metrique;

#[test]
fn revenu_cumule_saute_les_jours_sans_releve() {
    let s = store();
    s.serie("roleo", metrique::REVENU, "2026-11-11", -1, 50.0).unwrap();
    s.serie("roleo", metrique::REVENU, "2026-11-13", -1, 100.0).unwrap();
    let cumul = lectures(&s).revenu_cumule(3, "2026-11-13").unwrap();
    assert_eq!(cumul.iter().map(|c| c.jour.as_str()).collect::<Vec<_>>(), ["2026-11-11", "2026-11-13"]);
    assert_eq!(cumul.iter().map(|c| c.euros).collect::<Vec<_>>(), [metrique::en_euros(50.0), metrique::en_euros(150.0)]);
}

#[test]
fn resume_du_jour() {
    let s = store();
    s.serie("roleo", metrique::REVENU, "2026-11-13", -1, 100.0).unwrap();
    s.serie("roleo", metrique::ABONNES, "2026-11-13", -1, 41.0).unwrap();
    s.serie("roleo", metrique::ABONNES, "2026-11-06", -1, 39.0).unwrap();
    s.serie("roleo", metrique::MRR_BRUT, "2026-11-13", -1, 600.0).unwrap();
    s.ajouter_tache("Répondre à l'avis", None, None).unwrap();
    let r = lectures(&s).resume_widget().unwrap();
    assert_eq!(r.revenu, format::euros(metrique::en_euros(100.0)));
    assert_eq!(r.hier, format!("hier {}", format::VIDE));
    assert_eq!(r.detail, format::VIDE);
    assert_eq!(r.maj, None);
    assert_eq!(r.kpis.iter().map(|k| k.libelle.as_str()).collect::<Vec<_>>(), ["Abonnés", "Essais", "Installs", "Actifs", "Vues TikTok"]);
    let abonnes = &r.kpis[0];
    assert!(abonnes.valeur == "41" && abonnes.delta.as_deref() == Some("+2") && abonnes.ecart == Some(2.0));
    assert_eq!(abonnes.note, Some(format!("MRR {}", format::euros(metrique::en_euros(600.0)))));
    assert_eq!(abonnes.serie.len(), 14);
    assert_eq!(r.a_traiter, 1);
}

#[test]
fn meme_resume_meme_json() {
    let s = store();
    s.serie("roleo", metrique::REVENU, "2026-11-13", -1, 100.0).unwrap();
    let l = lectures(&s);
    let (a, b) = (serde_json::to_string(&l.resume_widget().unwrap()).unwrap(), serde_json::to_string(&l.resume_widget().unwrap()).unwrap());
    assert_eq!(a, b);
    assert_eq!(serde_json::from_str::<ResumeWidget>(&a).unwrap(), l.resume_widget().unwrap());
}
