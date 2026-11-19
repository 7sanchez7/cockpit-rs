use super::{d, lectures, maintenant, video};
use crate::business::Business;
use crate::essai::store;
use crate::jour;
use crate::lecture::fenetre::{Cumul, Fenetre, Periode};
use crate::lecture::Lectures;
use crate::metrique::{self, Boutique};
use crate::modeles::{Appli, EtatPassage, Evenement, Liste, Passage, Releve, Ton, TypeFil};
use chrono::Duration;

#[test]
fn valeur_et_jours_sans_releve() {
    let s = store();
    s.serie("roleo", metrique::ACTIFS, "2026-11-13", -1, 386.0).unwrap();
    s.serie("roleo", metrique::ACTIFS, "2026-11-11", -1, 300.0).unwrap();
    let l = lectures(&s);
    assert_eq!(l.valeur(metrique::ACTIFS, "2026-11-13", false).unwrap(), Some(386.0));
    assert_eq!(l.valeur(metrique::INSTALLS, "2026-11-13", false).unwrap(), None);
    assert_eq!(l.jours(metrique::ACTIFS, 3, "2026-11-13", false).unwrap(), vec![Some(300.0), None, Some(386.0)]);
}

#[test]
fn periode_couverte_en_partie() {
    let s = store();
    for j in ["2026-11-10", "2026-11-11", "2026-11-12"] { s.serie("roleo", metrique::VUES_TIKTOK, j, -1, 100.0).unwrap(); }
    for j in jour::derniers(14, "2026-11-12") { s.serie("roleo", metrique::MRR_BRUT, &j, -1, 500.0).unwrap(); }
    let l = lectures(&s);
    let f = Fenetre { periode: Periode::Semaine, maintenant: maintenant() };
    let vues = f.mesure(&l, metrique::VUES_TIKTOK, None, Cumul::Somme).unwrap();
    assert!(vues.valeur == Some(300.0) && vues.avant.is_none() && vues.couverture().as_deref() == Some("sur 3 j"));
    let mrr = f.mesure(&l, metrique::MRR_BRUT, None, Cumul::Dernier).unwrap();
    assert!(mrr.valeur == Some(500.0) && mrr.jours.is_none());
    let euros = vues.map(metrique::en_euros);
    assert!(euros.valeur == Some(276.0) && euros.serie.last() == Some(&Some(92.0)) && euros.couverture().as_deref() == Some("sur 3 j"));
}

#[test]
fn heures_sur_vingt_quatre() {
    let s = store();
    s.serie("roleo", metrique::ACTIFS_HEURE, "2026-11-13", 9, 32.0).unwrap();
    let h = lectures(&s).heures(metrique::ACTIFS_HEURE, "2026-11-13", false).unwrap();
    assert_eq!(h.len(), 24);
    assert_eq!(h[9], Some(32.0));
    assert_eq!(h[10], None);
}

#[test]
fn filtre_sur_le_business() {
    let s = store();
    let app2: Business = serde_json::from_str(r#"{"id": "app2", "nom": "App 2", "lettre": "A", "teinte": "ciel"}"#).unwrap();
    s.enregistrer(&app2).unwrap();
    s.serie("roleo", metrique::INSTALLS, "2026-11-13", -1, 10.0).unwrap();
    s.serie("app2", metrique::INSTALLS, "2026-11-13", -1, 5.0).unwrap();
    let tous = s.businesses().unwrap();
    assert_eq!(Lectures::new(&s.db, tous.clone()).valeur(metrique::INSTALLS, "2026-11-13", false).unwrap(), Some(15.0));
    let seul: Vec<Business> = tous.into_iter().filter(|b| b.id == "app2").collect();
    assert_eq!(Lectures::new(&s.db, seul).valeur(metrique::INSTALLS, "2026-11-13", false).unwrap(), Some(5.0));
}

#[test]
fn vue_par_store() {
    let s = store();
    s.serie("roleo", metrique::ACTIFS, "2026-11-13", -1, 158.0).unwrap();
    s.serie("roleo", &Boutique::AppStore.metrique(metrique::ACTIFS), "2026-11-13", -1, 152.0).unwrap();
    s.serie("roleo", &Boutique::PlayStore.metrique(metrique::ACTIFS), "2026-11-13", -1, 6.0).unwrap();
    s.serie("roleo", metrique::COUT_IA, "2026-11-13", -1, 7.0).unwrap();
    let mut l = lectures(&s);
    assert_eq!(l.valeur(metrique::ACTIFS, "2026-11-13", false).unwrap(), Some(158.0));
    l.boutique = Some(Boutique::PlayStore);
    assert_eq!(l.valeur(metrique::ACTIFS, "2026-11-13", false).unwrap(), Some(6.0));
    assert_eq!(l.jours(metrique::ACTIFS, 1, "2026-11-13", false).unwrap(), vec![Some(6.0)]);
    // une métrique sans découpe reste le total
    assert_eq!(l.valeur(metrique::COUT_IA, "2026-11-13", false).unwrap(), Some(7.0));
    l.boutique = Some(Boutique::AppStore);
    assert_eq!(l.valeur(metrique::ACTIFS, "2026-11-13", false).unwrap(), Some(152.0));
}

#[test]
fn fil_borne_sur_les_jours_de_paris() {
    let s = store();
    for (id, at) in [("veille", "2026-11-11T22:30:00Z"), ("jour", "2026-11-11T23:30:00Z"), ("lendemain", "2026-11-12T23:30:00Z")] {
        s.fil(&Evenement { id: id.into(), business: "roleo".into(), at: d(at), type_: TypeFil::Achat, texte: id.into(), lien: None }).unwrap();
    }
    let l = lectures(&s);
    let (du, au) = (jour::date("2026-11-12").unwrap(), jour::date("2026-11-13").unwrap());
    let ids = |e: Vec<Evenement>| e.into_iter().map(|e| e.id).collect::<Vec<_>>();
    assert_eq!(ids(l.fil(Some(&du), Some(&au), 200).unwrap()), ["jour"]);
    assert_eq!(ids(l.fil(None, None, 200).unwrap()), ["lendemain", "jour", "veille"]);
}

#[test]
fn video_gain_sur_24h_et_seuil() {
    let s = store();
    s.video(&video("1", "holaestony", "4 insultes\n#espagnol", Some("2026-11-01T10:00:00Z"))).unwrap();
    s.releve(&Releve { video: "1".into(), at: d("2026-11-12T13:00:00Z"), vues: 1000, partages: Some(5), enregistrements: Some(10), ..Default::default() }).unwrap();
    s.releve(&Releve { video: "1".into(), at: d("2026-11-13T13:30:00Z"), vues: 1400, partages: Some(8), enregistrements: Some(21), ..Default::default() }).unwrap();
    let v = lectures(&s).videos(1000).unwrap().remove(0);
    assert_eq!(v.titre, "4 insultes");
    assert_eq!(v.vues, 1400);
    assert_eq!(v.gain24h, Some(400));
    assert!(v.seuil_atteint());
}

#[test]
fn video_recente_gagne_toutes_ses_vues() {
    let s = store();
    s.video(&video("2", "holaestony", "Neuve", Some("2026-11-13T08:00:00Z"))).unwrap();
    s.releve(&Releve { video: "2".into(), at: d("2026-11-13T13:00:00Z"), vues: 6420, partages: Some(20), enregistrements: Some(78), ..Default::default() }).unwrap();
    let v = lectures(&s).videos(1000).unwrap().remove(0);
    assert_eq!(v.gain24h, Some(6420));
    assert!(!v.seuil_atteint());
}

#[test]
fn tache_en_retard_passe_en_alerte_et_devant() {
    let s = store();
    s.ajouter_tache("Plus tard", None, Some("2026-11-20")).unwrap();
    s.ajouter_tache("En retard", Some("roleo"), Some("2026-11-10")).unwrap();
    let l = lectures(&s);
    let t = l.taches().unwrap();
    assert_eq!(t.iter().map(|t| t.texte.as_str()).collect::<Vec<_>>(), ["En retard", "Plus tard"]);
    assert_eq!(t[0].ton, Ton::Alerte);
    assert_eq!(t[0].pastille("2026-11-13").as_deref(), Some("retard"));
    assert_eq!(t[1].pastille("2026-11-19").as_deref(), Some("demain"));
    assert_eq!(l.nb_a_traiter().unwrap(), 2);
}

#[test]
fn rang_et_gain_depuis_la_veille() {
    let s = store();
    s.app(&Appli { id: "6770304361".into(), nom: "Roleo".into(), business: Some("roleo".into()), ..Default::default() }).unwrap();
    s.rang("6770304361", "fr", 6017, Liste::Gratuit, &d("2026-11-12T12:00:00Z"), 163).unwrap();
    s.rang("6770304361", "fr", 6017, Liste::Gratuit, &maintenant(), 152).unwrap();
    let r = lectures(&s).rang_principal().unwrap().unwrap();
    assert_eq!(r.ligne.rangs.get(&r.cle), Some(&152));
    assert_eq!(r.ligne.gain(&r.cle), Some(11));
}

#[test]
fn dernier_passage_par_source() {
    let s = store();
    let veille = maintenant() - Duration::days(1);
    s.journal(&Passage { source: "posthog".into(), debut: veille, fin: veille, etat: EtatPassage::Erreur, erreur: Some("x".into()), lignes: 0 }).unwrap();
    s.journal(&Passage { source: "posthog".into(), debut: maintenant(), fin: maintenant(), etat: EtatPassage::Ok, erreur: None, lignes: 12 }).unwrap();
    assert_eq!(lectures(&s).derniers_passages().unwrap()["posthog"].etat, EtatPassage::Ok);
}
