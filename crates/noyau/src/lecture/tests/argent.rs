use super::{lectures, maintenant};
use crate::essai::store;
use crate::jour;
use crate::lecture::argent::{plus_grosse_perte, retour, serie, Jn, Objectif, TauxRetour};
use crate::metrique;
use crate::modeles::Cohorte;

fn cohorte(jour: &str, installs: i64, j1: Option<i64>) -> Cohorte {
    Cohorte { jour: jour.into(), installs, j1, ..Default::default() }
}

#[test]
fn objectif_en_euros_et_arrivee() {
    let s = store();
    s.serie("roleo", metrique::MRR_BRUT, "2026-10-14", -1, 250.0).unwrap();
    s.serie("roleo", metrique::MRR_BRUT, "2026-11-13", -1, 700.0).unwrap();
    let o = lectures(&s).objectif("2026-11-13").unwrap().unwrap();
    assert_eq!(o.cible_eur, 10_000.0);
    assert!((o.mrr_eur - 644.0).abs() < 1e-9);
    assert!((o.reste_eur() - 9_356.0).abs() < 1e-9);
    assert!((o.part() - 0.0644).abs() < 1e-9);
    assert!((o.rythme_par_jour_eur.unwrap_or(0.0) - 13.8).abs() < 1e-9);
    let arrivee = o.arrivee(&maintenant()).unwrap();
    let jours = (arrivee - maintenant()).num_milliseconds() as f64 / 86_400_000.0;
    assert!((jours - 9_356.0 / 13.8).abs() < 1e-6);
}

#[test]
fn pas_d_arrivee_sans_hausse_ni_trop_loin() {
    let s = store();
    s.serie("roleo", metrique::MRR_BRUT, "2026-11-13", -1, 700.0).unwrap();
    let l = lectures(&s);
    assert_eq!(l.objectif("2026-11-13").unwrap().unwrap().arrivee(&maintenant()), None);
    let lente = Objectif { cible_eur: 10_000.0, mrr_eur: 600.0, rythme_par_jour_eur: Some(1.0) };
    assert_eq!(lente.arrivee(&maintenant()), None);
    let baisse = Objectif { cible_eur: 10_000.0, mrr_eur: 600.0, rythme_par_jour_eur: Some(-2.0) };
    assert_eq!(baisse.arrivee(&maintenant()), None);
    assert_eq!(l.objectif("2026-11-12").unwrap(), None);
}

#[test]
fn somme_sur_n_jours() {
    let s = store();
    s.serie("roleo", metrique::ACHATS, "2026-11-13", -1, 3.0).unwrap();
    s.serie("roleo", metrique::ACHATS, "2026-11-10", -1, 2.0).unwrap();
    s.serie("roleo", metrique::ACHATS, "2026-11-05", -1, 9.0).unwrap();
    let l = lectures(&s);
    assert_eq!(l.somme(metrique::ACHATS, 7, "2026-11-13").unwrap(), Some(5.0));
    assert_eq!(l.somme(metrique::ACHATS, 3, "2026-11-04").unwrap(), None);
}

#[test]
fn retour_pondere_et_comparaison() {
    // 14 cohortes complètes, les 7 récentes à 10 sur 100, les 7 d'avant à 5 sur 50, puis 2 ouvertes
    let mut c = vec![cohorte("2026-11-13", 40, None), cohorte("2026-11-12", 80, None)];
    c.extend((0..7).map(|i| cohorte(&jour::decale("2026-11-11", -i), 100, Some(10))));
    c.extend((0..7).map(|i| cohorte(&jour::decale("2026-11-04", -i), 50, Some(5 + if i == 0 { 7 } else { 0 }))));
    let r = retour(&c, Jn::J1, 7).unwrap();
    assert_eq!(r.taux, 0.1);
    assert_eq!(r.cohortes, 7);
    assert_eq!(r.avant, Some(42.0 / 350.0));
}

#[test]
fn retour_sans_assez_de_cohortes() {
    let c = [cohorte("2026-10-12", 73, Some(1)), cohorte("2026-10-11", 108, Some(1)), cohorte("2026-10-10", 0, Some(0))];
    assert_eq!(retour(&c, Jn::J1, 7), Some(TauxRetour { taux: 2.0 / 181.0, cohortes: 2, avant: None }));
    assert_eq!(retour(&[cohorte("2026-11-13", 40, None)], Jn::J1, 7), None);
    assert_eq!(serie(&c, Jn::J1), vec![1.0 / 108.0, 1.0 / 73.0]);
}

#[test]
fn plus_grosse_perte_du_tunnel() {
    assert_eq!(plus_grosse_perte(&[("Installs", 950), ("Accueil", 915), ("Paywall vu", 863), ("Essai touché", 362)]), Some(3));
    assert_eq!(plus_grosse_perte(&[("Installs", 10), ("Accueil", 10)]), None);
    assert_eq!(plus_grosse_perte(&[]), None);
}
