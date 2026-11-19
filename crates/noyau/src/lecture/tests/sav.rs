//! Les tests de SAVTests qui passent par la base et les lectures.
use super::{d, lectures};
use crate::essai::store;
use crate::modeles::{CleFil, EtatFil, FilSAV, MessageSAV, ReleveSAV, RetourSAV, Tache};
use crate::p;
use crate::store::Store;
use std::collections::{BTreeMap, HashSet};

fn u1() -> CleFil { CleFil { business: "roleo".into(), uid: "u1".into() } }

fn cle(uid: &str) -> CleFil { CleFil { business: "roleo".into(), uid: uid.into() } }

fn fil(uid: &str, at: &str, ouvert: bool, equipe: bool) -> FilSAV {
    FilSAV {
        business: "roleo".into(), uid: uid.into(), nom: None, email: None, dernier_at: d(at), dernier_equipe: equipe,
        apercu: "salut".into(), ouvert, non_vu: false,
    }
}

fn retour(id: &str, traite: bool, uid: Option<&str>) -> RetourSAV {
    RetourSAV {
        business: "roleo".into(), id: id.into(), uid: uid.map(str::to_string), email: None, source: Some("profile".into()),
        categorie: Some("idea".into()), etoiles: None, message: "une idée".into(), citation: None, conversation: None, traite,
        traite_le: None, plateforme: None, build: None, at: d("2026-11-14T10:00:00Z"),
    }
}

fn message(id: &str, equipe: bool, at: &str) -> MessageSAV { MessageSAV { id: id.into(), equipe, texte: id.into(), at: d(at) } }

fn tache(cle: &str, texte: &str) -> Tache { Tache { cle: cle.into(), texte: texte.into(), ..Tache::default() } }

fn ouvertes(s: &Store) -> HashSet<String> {
    s.db.query("select id from a_traiter where fait_le is null", p![]).unwrap().iter().filter_map(|l| l.texte("id")).collect()
}

fn ids(m: &[MessageSAV]) -> Vec<&str> { m.iter().map(|m| m.id.as_str()).collect() }

#[test]
fn synchronisation() {
    let s = store();
    let r = ReleveSAV {
        fils: vec![fil("u1", "2026-11-14T10:00:00Z", true, false), fil("u2", "2026-11-14T11:00:00Z", false, true)],
        retours: vec![retour("r1", false, Some("u1")), retour("r2", true, None)],
        messages: BTreeMap::from([("u1".into(), vec![message("m1", false, "2026-11-14T09:00:00Z"), message("m2", false, "2026-11-14T10:00:00Z")])]),
        emails: BTreeMap::from([("u1".into(), "lina@exemple.fr".into()), ("u2".into(), String::new())]),
    };
    assert_eq!(s.synchroniser_sav("roleo", &r).unwrap(), 8);
    assert_eq!(s.fils_lus("roleo").unwrap(), [("u1".to_string(), "2026-11-14T10:00:00Z".to_string())].into());
    assert_eq!(s.comptes_sav("roleo").unwrap(), ["u1".to_string()].into());
    let l = lectures(&s);
    let nb = l.nb_sav().unwrap();
    assert_eq!((nb.fils, nb.retours), (1, 1));
    assert_eq!(l.retours_sav(Some(false), 50).unwrap()[0].email.as_deref(), Some("lina@exemple.fr"));
    assert_eq!(l.email_sav(&cle("u2")).unwrap().as_deref(), Some(""));
    assert_eq!(l.email_sav(&cle("u9")).unwrap(), None);

    // u1 et r1 traités sur le dashboard : absents du relevé suivant ; u2 relu
    let suite = ReleveSAV {
        fils: vec![fil("u2", "2026-11-14T12:00:00Z", true, false)],
        messages: BTreeMap::from([("u2".into(), vec![message("m3", false, "2026-11-14T12:00:00Z")])]),
        ..ReleveSAV::default()
    };
    s.synchroniser_sav("roleo", &suite).unwrap();
    assert!(!l.fil_sav(&u1()).unwrap().unwrap().ouvert);
    assert!(l.retours_sav(Some(false), 50).unwrap().is_empty());
    assert_eq!(ids(&l.messages_sav(&u1()).unwrap()), ["m1", "m2"]);
    assert_eq!(ids(&l.messages_sav(&cle("u2")).unwrap()), ["m3"]);

    // un fil relu remplace ses messages
    let relu = ReleveSAV {
        messages: BTreeMap::from([("u1".into(), vec![message("m4", false, "2026-11-14T13:00:00Z")])]),
        ..ReleveSAV::default()
    };
    s.synchroniser_sav("roleo", &relu).unwrap();
    assert_eq!(ids(&l.messages_sav(&u1()).unwrap()), ["m4"]);
}

#[test]
fn fils_ranges() {
    let s = store();
    let r = ReleveSAV {
        fils: vec![
            fil("u1", "2026-11-14T08:00:00Z", true, false),
            fil("u2", "2026-11-14T12:00:00Z", false, false),
            fil("u3", "2026-11-14T10:00:00Z", false, false),
        ],
        emails: BTreeMap::from([("u2".into(), "a@exemple.fr".into())]),
        ..ReleveSAV::default()
    };
    s.synchroniser_sav("roleo", &r).unwrap();
    let fils = lectures(&s).fils_sav(50).unwrap();
    assert_eq!(fils.iter().map(|f| f.uid.as_str()).collect::<Vec<_>>(), ["u1", "u2", "u3"]);
    assert_eq!(fils[1].email.as_deref(), Some("a@exemple.fr"));
    assert_eq!(fils[2].email, None);
}

#[test]
fn reponse_ferme_le_fil_et_ses_taches() {
    let s = store();
    s.synchroniser_taches("firebase", "roleo", &[tache("msg:u1:2026-11-14T10:00:00Z", "Bonjour"), tache("msg:u10:2026-11-14T10:00:00Z", "Autre")])
        .unwrap();
    let r = ReleveSAV { fils: vec![fil("u1", "2026-11-14T10:00:00Z", true, false)], ..ReleveSAV::default() };
    s.synchroniser_sav("roleo", &r).unwrap();
    s.repondre_sav(&u1(), &message("m9", true, "2026-11-14T11:00:00Z"), "Re").unwrap();
    let l = lectures(&s);
    let f = l.fil_sav(&u1()).unwrap().unwrap();
    assert_eq!(f.etat(), EtatFil::PasLu);
    assert_eq!(f.apercu, "Re");
    assert_eq!(f.dernier_at, d("2026-11-14T11:00:00Z"));
    assert_eq!(ids(&l.messages_sav(&u1()).unwrap()), ["m9"]);
    assert_eq!(ouvertes(&s), ["firebase:msg:u10:2026-11-14T10:00:00Z".to_string()].into());

    // un compte qui n'avait jamais écrit : le fil naît de la réponse
    s.repondre_sav(&cle("u5"), &message("m10", true, "2026-11-14T12:00:00Z"), "Bienvenue").unwrap();
    assert_eq!(l.fil_sav(&cle("u5")).unwrap().unwrap().etat(), EtatFil::PasLu);
}

#[test]
fn fermer_sans_repondre() {
    let s = store();
    s.synchroniser_taches("firebase", "roleo", &[tache("msg:u1:2026-11-14T10:00:00Z", "x")]).unwrap();
    let r = ReleveSAV { fils: vec![fil("u1", "2026-11-14T10:00:00Z", true, false)], ..ReleveSAV::default() };
    s.synchroniser_sav("roleo", &r).unwrap();
    s.fermer_fil_sav(&u1()).unwrap();
    assert_eq!(lectures(&s).fil_sav(&u1()).unwrap().unwrap().etat(), EtatFil::Traite);
    assert!(ouvertes(&s).is_empty());
}

#[test]
fn traiter_et_rouvrir_un_retour() {
    let s = store();
    s.synchroniser_taches("firebase", "roleo", &[tache("retour:r1", "Idée")]).unwrap();
    let r = ReleveSAV { retours: vec![retour("r1", false, None)], ..ReleveSAV::default() };
    s.synchroniser_sav("roleo", &r).unwrap();
    let le = d("2026-11-14T12:00:00Z");
    let l = lectures(&s);
    s.traiter_retour_sav("roleo", "r1", true, &le).unwrap();
    assert_eq!(l.retours_sav(Some(true), 50).unwrap()[0].traite_le, Some(le));
    assert!(ouvertes(&s).is_empty());
    s.traiter_retour_sav("roleo", "r1", false, &le).unwrap();
    assert_eq!(l.retours_sav(Some(false), 50).unwrap()[0].traite_le, None);
    assert_eq!(ouvertes(&s), ["firebase:retour:r1".to_string()].into());
}

#[test]
fn un_retour_par_sa_cle() {
    let s = store();
    let r = ReleveSAV {
        retours: vec![retour("r1", false, Some("u1")), retour("r2", true, None)],
        emails: BTreeMap::from([("u1".into(), "lina@exemple.fr".into())]),
        ..ReleveSAV::default()
    };
    s.synchroniser_sav("roleo", &r).unwrap();
    let l = lectures(&s);
    let r1 = l.retour_sav("roleo", "r1").unwrap().unwrap();
    assert_eq!((r1.email.as_deref(), r1.traite), (Some("lina@exemple.fr"), false));
    assert!(l.retour_sav("roleo", "r2").unwrap().unwrap().traite);
    assert_eq!(l.retour_sav("roleo", "r9").unwrap(), None);
}
