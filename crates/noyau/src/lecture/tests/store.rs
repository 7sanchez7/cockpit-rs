//! Les parties de StoreTests que les tests de store.rs ne couvrent pas encore.
use crate::business::Graine;
use crate::essai::{fichier, store, GRAINE};
use crate::jour;
use crate::modeles::{Compte, EtatPaiement, EtatPassage, Evenement, Install, Passage, Plateforme, Tache, TypeFil};
use crate::p;
use crate::store::Store;
use chrono::{Duration, TimeZone, Utc};
use std::fs::{File, FileTimes};
use std::time::SystemTime;

#[test]
fn graine_relue_seulement_quand_elle_change() {
    let g = fichier(GRAINE);
    let chemin = std::env::temp_dir().join(format!("graine-{}.sqlite", uuid::Uuid::new_v4()));
    Store::ouvrir_a(&chemin, Some(&g)).unwrap().activer_compte("tiktok:tonyolingo", false).unwrap();
    Store::ouvrir_a(&chemin, Some(&g)).unwrap().supprimer_business("roleo").unwrap();
    // même fichier : rien ne revient
    assert!(Store::ouvrir_a(&chemin, Some(&g)).unwrap().businesses().unwrap().is_empty());
    // fichier modifié : le business revient, le compte en pause y reste
    std::fs::write(&g, GRAINE).unwrap();
    let plus_tard = SystemTime::now() + std::time::Duration::from_secs(60);
    File::options().write(true).open(&g).unwrap().set_times(FileTimes::new().set_modified(plus_tard)).unwrap();
    let s = Store::ouvrir_a(&chemin, Some(&g)).unwrap();
    assert_eq!(s.businesses().unwrap().iter().map(|b| b.id.as_str()).collect::<Vec<_>>(), ["roleo"]);
    assert!(!s.comptes().unwrap().iter().find(|c| c.id() == "tiktok:tonyolingo").unwrap().actif);
}

#[test]
fn graine_absente_ou_illisible() {
    let absente = std::env::temp_dir().join("absente.json");
    assert!(Store::memoire_avec(&absente).unwrap().businesses().unwrap().is_empty());
    assert!(Store::memoire_avec(&fichier(r#"{"businesses": [{"nom": "X"}]}"#)).is_err());
}

#[test]
fn graine_minimale() {
    let s = Store::memoire_avec(&fichier(r#"{"businesses": [{"id": "app", "nom": "mon app"}],
        "comptes": [{"business": "app", "plateforme": "tiktok", "handle": "@Mon.App"}]}"#)).unwrap();
    let b = &s.businesses().unwrap()[0];
    assert!(b.lettre == "M" && b.teinte == "menthe" && b.config.evenements.essai == "trial_started");
    assert_eq!(s.comptes().unwrap().iter().map(Compte::id).collect::<Vec<_>>(), ["tiktok:mon.app"]);
}

#[test]
fn comptes_suivis_editables() {
    let s = store();
    s.ajouter(&Compte::nouveau("roleo", Plateforme::Facebook, "roleo.app")).unwrap();
    s.activer_compte("tiktok:tonyolingo", false).unwrap();
    s.retirer_compte("youtube:holaestony").unwrap();
    let c = s.comptes().unwrap();
    let ids: Vec<String> = c.iter().map(Compte::id).collect();
    assert!(ids.contains(&"facebook:roleo.app".to_string()));
    assert!(!ids.contains(&"youtube:holaestony".to_string()));
    assert!(!c.iter().find(|c| c.id() == "tiktok:tonyolingo").unwrap().actif);
    // réajouter un compte en pause le relance
    s.ajouter(&Compte::nouveau("roleo", Plateforme::Tiktok, "tonyolingo")).unwrap();
    assert!(s.comptes().unwrap().iter().find(|c| c.id() == "tiktok:tonyolingo").unwrap().actif);
}

#[test]
fn serie_remplace_la_valeur_du_jour() {
    let s = store();
    s.serie("roleo", "actifs", "2026-11-13", -1, 10.0).unwrap();
    s.serie("roleo", "actifs", "2026-11-13", -1, 12.0).unwrap();
    let l = s.db.query("select valeur from serie where metrique = 'actifs'", p![]).unwrap();
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].nombre("valeur"), Some(12.0));
}

#[test]
fn historique_incomplet_si_hier_ou_le_plus_vieux_manque() {
    let s = store();
    let incomplet = || s.historique_incomplet("roleo", "actifs", 30, "2026-11-13").unwrap();
    assert!(incomplet());
    s.serie("roleo", "actifs", "2026-11-12", -1, 1.0).unwrap();
    assert!(incomplet());
    s.serie("roleo", "actifs", "2026-10-15", -1, 1.0).unwrap();
    assert!(!incomplet());
}

#[test]
fn fil_ne_renotifie_pas() {
    let s = store();
    let e = Evenement { id: "posthog:essai:1".into(), business: "roleo".into(), at: jour::maintenant(), type_: TypeFil::Essai,
                        texte: "Essai annuel".into(), lien: None };
    assert!(s.fil(&e).unwrap());
    assert!(!s.fil(&e).unwrap());
    assert!(s.prendre_nouveaux("revenuecat").is_empty());
    assert_eq!(s.prendre_nouveaux("posthog").iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["posthog:essai:1"]);
    assert!(s.prendre_nouveaux("posthog").is_empty());
}

#[test]
fn taches_synchronisees() {
    let s = store();
    let t = |cle: &str, texte: &str| Tache { cle: cle.into(), texte: texte.into(), ..Default::default() };
    s.synchroniser_taches("asc", "roleo", &[t("avis:1", "a"), t("avis:2", "b")]).unwrap();
    s.synchroniser_taches("asc", "roleo", &[t("avis:2", "b")]).unwrap();
    let ouvertes: Vec<String> = s.db.query("select id from a_traiter where fait_le is null", p![]).unwrap()
        .iter().filter_map(|l| l.texte("id")).collect();
    assert_eq!(ouvertes, ["asc:avis:2"]);
}

#[test]
fn clients_a_verifier() {
    let s = store();
    let relu = Utc.timestamp_opt(1_794_000_000, 0).unwrap();
    let install = |jour: &str, client: Option<&str>| Install {
        jour: jour.into(), essai: client.is_some(), client: client.map(str::to_string), ..Default::default()
    };
    s.installs("roleo", "2026-10-01", &[
        install("2026-10-05", Some("ancien")), install("2026-11-10", Some("jamais")), install("2026-11-11", Some("paye")),
        install("2026-11-12", Some("essai-vieux")), install("2026-11-13", Some("essai-frais")), install("2026-11-13", None),
    ]).unwrap();
    for (client, etat, lu) in [("ancien", EtatPaiement::Essai, -60), ("paye", EtatPaiement::Paye, -60),
                               ("essai-vieux", EtatPaiement::Essai, -60), ("essai-frais", EtatPaiement::Essai, 60)] {
        s.paiement("roleo", client, etat, &(relu + Duration::seconds(lu))).unwrap();
    }
    assert_eq!(s.clients_a_verifier("roleo", "2026-11-01", &relu, 10).unwrap(), ["essai-vieux", "jamais"]);
    assert_eq!(s.clients_a_verifier("roleo", "2026-11-01", &relu, 1).unwrap(), ["essai-vieux"]);
}

#[test]
fn journal() {
    let s = store();
    let debut = jour::maintenant();
    s.journal(&Passage { source: "posthog".into(), debut, fin: debut, etat: EtatPassage::Erreur, erreur: Some("HTTP 500".into()), lignes: 0 }).unwrap();
    assert_eq!(s.dernier_succes("posthog").unwrap(), None);
    s.journal(&Passage { source: "posthog".into(), debut: debut + Duration::seconds(1), fin: debut + Duration::seconds(2),
                         etat: EtatPassage::Ok, erreur: None, lignes: 3 }).unwrap();
    assert_eq!(s.dernier_passage("posthog").unwrap().map(|p| p.lignes), Some(3));
    assert!(s.dernier_succes("posthog").unwrap().is_some());
}

#[test]
fn graine_des_tests_lisible() {
    let g: Graine = serde_json::from_str(GRAINE).unwrap();
    assert_eq!(g.comptes.len(), store().comptes().unwrap().len());
}
