use super::*;
use crate::essai;
use crate::modeles::TypeFil;
use crate::p;
use crate::store::Store;
use chrono::Duration;

/// Jeudi 13 novembre 2026, 14 h 30 à Paris.
fn maintenant() -> Date { jour::date("2026-11-13").unwrap() + Duration::minutes(14 * 60 + 30) }

/// Une réponse HogQL figée, telle que `/query/` la rend.
fn lignes(results: &str, has_more: bool) -> Res<Lignes> {
    let json = format!(r#"{{"columns": [], "types": [], "limit": 100, "offset": 0, "hasMore": {has_more}, "results": {results}}}"#);
    decodage::lignes(&serde_json::from_str(&json).unwrap())
}

fn l(results: &str) -> Lignes { lignes(results, false).unwrap() }

fn valeur(store: &Store, metrique: &str, jour: &str, heure: i64) -> Option<f64> {
    store
        .db
        .query("select valeur from serie where business = 'roleo' and metrique = ? and jour = ? and heure = ?", p![metrique, jour, heure])
        .unwrap()
        .first()
        .and_then(|x| x.nombre("valeur"))
}

fn v(store: &Store, metrique: &str, jour: &str) -> Option<f64> { valeur(store, metrique, jour, -1) }

/// Une ligne de la requête par install ; `drapeaux` dans l'ordre paywall, essai, direct, rond, j1, j7, j14, j30.
fn install(jour: &str, loin: i64, drapeaux: [i64; 8], client: &str, valeurs: &[(Dimension, &str)]) -> String {
    let v: Vec<String> = Dimension::lues()
        .map(|d| {
            let x = if d == Dimension::Loin {
                loin.to_string()
            } else {
                valeurs.iter().find(|(c, _)| *c == d).map_or(String::new(), |(_, x)| x.to_string())
            };
            format!("\"{x}\"")
        })
        .collect();
    let d: Vec<String> = drapeaux.iter().map(i64::to_string).collect();
    format!("[\"{jour}\", {loin}, {}, \"{client}\", {}]", d.join(", "), v.join(", "))
}

fn lecture() -> LecturePostHog {
    use Dimension::{Age, Build, Interet, Pays, Source, Version};
    let installs = [
        install("2026-11-03", 17, [1, 1, 0, 1, 1, 1, 0, 0], "c1",
                &[(Age, "18_24"), (Source, "tiktok"), (Interet, "music, films"), (Pays, "FR"), (Version, "2.0.2"), (Build, "43")]),
        install("2026-11-03", 16, [1, 0, 1, 1, 0, 0, 0, 0], "c2",
                &[(Age, "25_34"), (Source, "tiktok"), (Interet, "music"), (Pays, "FR"), (Version, "2.0.2"), (Build, "43")]),
        install("2026-11-05", 17, [1, 1, 0, 0, 0, 0, 0, 0], "c3",
                &[(Age, "18_24"), (Source, "instagram"), (Pays, "ES"), (Version, "2.0.2"), (Build, "43")]),
        install("2026-11-12", 6, [0; 8], "", &[(Age, "18_24"), (Source, "tiktok"), (Version, "2.0.3"), (Build, "9")]),
        install("2026-11-12", 0, [0; 8], "", &[]),
    ];
    LecturePostHog {
        // le total, puis App Store, puis Play Store
        series: decodage::series(
            &l(r#"[["2026-11-06", 327, 160, 8, 4, 20, 10, 300, 150, 7, 4, 18, 9, 20, 8, 1, 0, 2, 1],
                   ["2026-11-12", 490, 490, 6, 6, 300, 300, 470, 470, 6, 6, 290, 290, 15, 15, 0, 0, 10, 10],
                   ["2026-11-13", 230, 230, 3, 3, 100, 100, 220, 220, 2, 2, 95, 95, 9, 9, 1, 1, 5, 5]]"#),
            &l(r#"[["2026-11-06", 115, 40, 110, 38, 5, 2], ["2026-11-13", 51, 51, 47, 47, 4, 4]]"#),
        )
        .unwrap(),
        actifs_heure: decodage::par_heure(&l(r#"[["2026-11-13", 0, 20, 19, 1], ["2026-11-13", 9, 31, 28, 3]]"#)).unwrap(),
        fil: decodage::fil(
            &l(r#"[["0193a1b2-0000-0000-0000-000000000001", 1794470458, "Yearly", "onboarding", "BE"],
                   ["0193a1b2-0000-0000-0000-000000000002", 1794454390, "Yearly", "round_locked", "ES"],
                   ["0193a1b2-0000-0000-0000-000000000003", 1794409202, "roleo_quarterly:trimestriel", null, null]]"#),
            "roleo",
        )
        .unwrap(),
        horaire: Some(Horaire {
            debut: "2026-11-01".into(),
            installs: decodage::installs(&l(&format!("[{}]", installs.join(", ")))).unwrap(),
            portes: decodage::portes(&l(r#"[["2026-11-12", "onboarding", 8, 2, 1], ["2026-11-12", "round_locked", 12, 1, 0]]"#)).unwrap(),
        }),
    }
}

fn ecrire_l(store: &Store, l: &LecturePostHog) -> usize {
    ecrire(l, "roleo", &["2026-11-12".into(), "2026-11-13".into()], &["2026-11-13".into()], &maintenant(), store).unwrap()
}

#[test]
fn reponse_tronquee_ou_sans_resultats() {
    assert!(lignes("[]", true).is_err());
    assert!(decodage::lignes(serde_json::json!({"error": "boom"}).as_object().unwrap()).is_err());
    assert!(decodage::par_jour(&l(r#"[["2026-11-13", 1]]"#), 4).is_err());
    assert!(decodage::installs(&l(r#"[["2026-11-13", 1]]"#)).is_err());
}

#[test]
fn series_et_leur_j7() {
    let s = essai::store();
    ecrire_l(&s, &lecture());
    assert_eq!(v(&s, metrique::ACTIFS, "2026-11-13"), Some(230.0));
    assert_eq!(v(&s, metrique::ACTIFS_J7, "2026-11-13"), Some(160.0));
    assert_eq!(v(&s, metrique::ESSAIS, "2026-11-13"), Some(3.0));
    assert_eq!(v(&s, metrique::ESSAIS_J7, "2026-11-13"), Some(4.0));
    assert_eq!(v(&s, metrique::INSTALLS, "2026-11-13"), Some(51.0));
    assert_eq!(v(&s, metrique::INSTALLS_J7, "2026-11-13"), Some(40.0));
    assert_eq!(v(&s, metrique::RONDS, "2026-11-13"), Some(100.0));
    assert_eq!(v(&s, metrique::RONDS_J7, "2026-11-13"), Some(10.0));
    // jour sans ligne : zéro, pas de trou
    assert_eq!(v(&s, metrique::INSTALLS, "2026-11-12"), Some(0.0));
    assert_eq!(v(&s, metrique::ACTIFS_J7, "2026-11-12"), Some(0.0));
}

#[test]
fn series_par_store() {
    let s = essai::store();
    ecrire_l(&s, &lecture());
    let (play, app) = (Boutique::PlayStore, Boutique::AppStore);
    assert_eq!(v(&s, &play.metrique(metrique::ACTIFS), "2026-11-13"), Some(9.0));
    assert_eq!(v(&s, &play.metrique(metrique::ACTIFS_J7), "2026-11-13"), Some(8.0));
    assert_eq!(v(&s, &play.metrique(metrique::ESSAIS), "2026-11-13"), Some(1.0));
    assert_eq!(v(&s, &play.metrique(metrique::INSTALLS), "2026-11-13"), Some(4.0));
    assert_eq!(v(&s, &play.metrique(metrique::INSTALLS_J7), "2026-11-13"), Some(2.0));
    assert_eq!(v(&s, &play.metrique(metrique::RONDS_J7), "2026-11-13"), Some(1.0));
    assert_eq!(v(&s, &app.metrique(metrique::INSTALLS), "2026-11-13"), Some(47.0));
    assert_eq!(v(&s, &app.metrique(metrique::RONDS), "2026-11-12"), Some(290.0));
    // un store sans ligne : zéro
    assert_eq!(v(&s, &play.metrique(metrique::INSTALLS), "2026-11-12"), Some(0.0));
    assert!(decodage::series(&l(r#"[["2026-11-13", 1, 1, 0, 0, 0, 0]]"#), &vec![]).is_err());
}

#[test]
fn requetes_par_segment() {
    let e = Evenements::default();
    let jours = requete_jours(&e, 0, 1);
    assert!(jours.contains("uniqExactIf(person_id, true)"));
    assert!(jours.contains("countIf(event = 'trial_started' and properties.$os = 'Android' and timestamp < fromUnixTimestamp(1))"));
    let installs = requete_installs(&e, 0, 1);
    assert!(installs.contains("countIf(os in ('iOS', 'iPadOS')), countIf(os in ('iOS', 'iPadOS') and f < fromUnixTimestamp(1))"));
    assert!(installs.contains("where os in ('iOS', 'iPadOS', 'Android')"));
}

#[test]
fn requetes_sur_les_evenements_du_business() {
    let e = Evenements { installation: "signup".into(), essai: "trial".into(), ..Default::default() };
    assert!(requete_installs(&e, 0, 1).contains("where event = 'signup'"));
    let par_install = requete_par_install(&e, 0);
    assert!(par_install.contains("minIf(timestamp, event = 'signup')") && par_install.contains("countIf(event = 'trial') > 0 as essai"));
    assert!(!par_install.contains("onboarding_started") && !par_install.contains("trial_started"));
}

#[test]
fn ronds_seulement_depuis_la_mesure() {
    let s = essai::store();
    let jours = ["2026-10-14".to_string(), "2026-10-21".into(), "2026-10-22".into()];
    ecrire(&LecturePostHog::default(), "roleo", &jours, &[], &maintenant(), &s).unwrap();
    assert_eq!(v(&s, metrique::RONDS, "2026-10-14"), None);
    assert_eq!(v(&s, metrique::RONDS, "2026-10-21"), Some(0.0));
    assert_eq!(v(&s, metrique::RONDS_J7, "2026-10-21"), None);
    assert_eq!(v(&s, metrique::RONDS_J7, "2026-10-22"), Some(0.0));
    assert_eq!(v(&s, metrique::ACTIFS, "2026-10-14"), Some(0.0));
}

#[test]
fn actifs_par_heure_jusqua_l_heure_en_cours() {
    let s = essai::store();
    ecrire_l(&s, &lecture());
    assert_eq!(valeur(&s, metrique::ACTIFS_HEURE, "2026-11-13", 0), Some(20.0));
    assert_eq!(valeur(&s, metrique::ACTIFS_HEURE, "2026-11-13", 9), Some(31.0));
    assert_eq!(valeur(&s, metrique::ACTIFS_HEURE, "2026-11-13", 14), Some(0.0));
    assert_eq!(valeur(&s, metrique::ACTIFS_HEURE, "2026-11-13", 15), None);
    assert_eq!(valeur(&s, &Boutique::PlayStore.metrique(metrique::ACTIFS_HEURE), "2026-11-13", 9), Some(3.0));
    assert_eq!(valeur(&s, &Boutique::AppStore.metrique(metrique::ACTIFS_HEURE), "2026-11-13", 14), Some(0.0));
}

#[test]
fn fil_des_essais() {
    let l = lecture();
    let textes: Vec<&str> = l.fil.iter().map(|e| e.texte.as_str()).collect();
    assert_eq!(textes, ["Essai annuel, Belgique", "Essai annuel, Espagne, rond verrouillé", "Essai trimestriel"]);
    assert_eq!(l.fil[0].id, "posthog:essai:0193a1b2-0000-0000-0000-000000000001");
    assert_eq!(Some(l.fil[0].at), jour::depuis_iso("2026-11-12T08:00:58Z"));
    assert!(l.fil.iter().all(|e| e.type_ == TypeFil::Essai));
    assert_eq!(decodage::texte_essai(Some("monthly"), Some("porte_inconnue"), Some("US")), "Essai mensuel, États-Unis, porte_inconnue");
}

#[test]
fn lignes_ecrites_et_fil_sans_doublon() {
    let s = essai::store();
    let l = lecture();
    // 2 jours × 8 séries × 3 segments, 15 heures × 3 segments, 3 essais, 5 installs, 2 portes
    assert_eq!(ecrire_l(&s, &l), 48 + 45 + 3 + 7);
    assert_eq!(ecrire_l(&s, &l), 48 + 45 + 7);
    assert_eq!(s.db.query("select 1 from install", p![]).unwrap().len(), 5);
}

#[test]
fn installs_decodes() {
    let i = lecture().horaire.unwrap().installs;
    assert_eq!(i.len(), 5);
    assert!(i[0].loin == 17 && i[0].paywall && i[0].essai && !i[0].direct && i[0].j7 && !i[0].j14);
    assert_eq!(i[0].client.as_deref(), Some("c1"));
    assert_eq!(i[0].valeurs[&Dimension::Interet], ["music", "films"]);
    assert_eq!(i[0].valeurs[&Dimension::Loin], ["17"]);
    assert_eq!(i[0].valeurs[&Dimension::Zone], ["europe"]);
    assert_eq!(i[4].client, None);
    assert_eq!(i[4].valeurs, [(Dimension::Loin, vec!["0".to_string()])].into_iter().collect());
}
