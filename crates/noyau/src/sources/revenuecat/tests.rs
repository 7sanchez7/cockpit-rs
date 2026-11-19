//! Réponses figées de l'API v2 de RevenueCat et de Firestore (ids inventés).
use super::*;
use crate::essai;
use crate::p;

const PROJETS: &str = r#"
    {"items":[{"object":"project","id":"proj_autre","name":"App 2","created_at":1750000000000},
              {"object":"project","id":"proj_roleo","name":"Roleo","created_at":1750000000000}],"next_page":null}"#;

const OVERVIEW: &str = r##"
    {"object":"overview_metrics","currency":"USD","metrics":[
      {"object":"overview_metric","id":"active_trials","name":"Active Trials","unit":"#","period":"P0D","value":23},
      {"object":"overview_metric","id":"active_subscriptions","name":"Active Subscriptions","unit":"#","period":"P0D","value":100},
      {"object":"overview_metric","id":"mrr","name":"MRR","unit":"$","period":"P28D","value":947},
      {"object":"overview_metric","id":"revenue","name":"Revenue","unit":"$","period":"P28D","value":3268},
      {"object":"overview_metric","id":"new_customers","name":"New Customers","unit":"#","period":"P28D","value":2400}]}"##;

/// `actives_new` sur deux jours : minuit UTC du 11 et du 12 novembre 2026.
const ACTIVES_NEW: &str = r##"
    {"object":"chart_data","category":"subscription","display_name":"New Paid Subscriptions","resolution":"day",
     "measures":[{"display_name":"Trial Conversions","unit":"#","chartable":true,"decimal_precision":0},
                 {"display_name":"Intro Offers","unit":"#","chartable":true,"decimal_precision":0},
                 {"display_name":"Direct Subscriptions","unit":"#","chartable":true,"decimal_precision":0},
                 {"display_name":"Product Changes","unit":"#","chartable":true,"decimal_precision":0},
                 {"display_name":"Resubscriptions","unit":"#","chartable":true,"decimal_precision":0},
                 {"display_name":"Total Paid Subscriptions","unit":"#","chartable":true,"decimal_precision":0}],
     "values":[{"cohort":1794355200,"incomplete":false,"measure":0,"value":1},
               {"cohort":1794355200,"incomplete":false,"measure":2,"value":4},
               {"cohort":1794355200,"incomplete":false,"measure":3,"value":1},
               {"cohort":1794355200,"incomplete":false,"measure":5,"value":6},
               {"cohort":1794441600,"incomplete":true,"measure":2,"value":2},
               {"cohort":1794441600,"incomplete":true,"measure":3,"value":0},
               {"cohort":1794441600,"incomplete":true,"measure":5,"value":2}]}"##;

const MRR: &str = r#"
    {"resolution":"day","measures":[{"display_name":"MRR","unit":"$"}],
     "values":[{"cohort":1794355200,"incomplete":false,"measure":0,"value":947.95},
               {"cohort":1794441600,"incomplete":true,"measure":0,"value":947.69}]}"#;

fn serie(v: &[(&str, f64)]) -> Serie { v.iter().map(|(j, x)| (j.to_string(), *x)).collect() }

fn date(iso: &str) -> Date { jour::depuis_iso(iso).unwrap() }

#[test]
fn projet_par_nom() {
    assert_eq!(projet(PROJETS.as_bytes(), "Roleo").unwrap(), "proj_roleo");
    assert!(projet(PROJETS.as_bytes(), "App 3").is_err());
    let seul = r#"{"items":[{"id":"proj_seul","name":"Roleo iOS"}]}"#;
    assert_eq!(projet(seul.as_bytes(), "Roleo").unwrap(), "proj_seul");
}

#[test]
fn apercu_lu() {
    let a = apercu(OVERVIEW.as_bytes()).unwrap();
    assert_eq!(a, Apercu { mrr: 947.0, abonnes: 100.0, essais: 23.0, revenu28j: 3268.0 });
    assert!(apercu(br#"{"metrics":[{"id":"mrr","value":947}]}"#).is_err());
}

#[test]
fn graphique_a_la_journee_de_paris() {
    let g = graphique(MRR.as_bytes(), None).unwrap();
    assert_eq!(mesure("MRR", &g).unwrap(), serie(&[("2026-11-11", 947.95), ("2026-11-12", 947.69)]));
    assert!(mesure("Actives", &g).is_err());
}

#[test]
fn achats_sans_changements_de_produit() {
    let g = graphique(ACTIVES_NEW.as_bytes(), None).unwrap();
    assert_eq!(achats(&g).unwrap(), serie(&[("2026-11-11", 5.0), ("2026-11-12", 2.0)]));
}

#[test]
fn series_ecrites() {
    let store = essai::store();
    let a = apercu(OVERVIEW.as_bytes()).unwrap();
    let s = Series {
        mrr: mesure("MRR", &graphique(MRR.as_bytes(), None).unwrap()).unwrap(),
        abonnes: serie(&[("2026-11-11", 100.0)]),
        essais: serie(&[("2026-11-11", 20.0)]),
        achats: achats(&graphique(ACTIVES_NEW.as_bytes(), None).unwrap()).unwrap(),
        revenu: serie(&[("2026-11-11", 120.0), ("2026-11-12", 35.0)]),
    };
    let points = points_des_series(&a, &s, &serie(&[("2026-11-11", 3200.0), ("2026-11-12", 1.0)]), "2026-11-12");
    ecrire(&points, "roleo", &store).unwrap();
    let valeur = |m: &str, j: &str| {
        store
            .db
            .query("select valeur from serie where business = 'roleo' and metrique = ? and jour = ?", p![m, j])
            .unwrap()
            .first()
            .and_then(|l| l.nombre("valeur"))
    };
    assert_eq!(valeur(metrique::MRR_BRUT, "2026-11-11"), Some(947.95));
    // aujourd'hui vient de l'overview, pas du graphique incomplet
    assert_eq!(valeur(metrique::MRR_BRUT, "2026-11-12"), Some(947.0));
    assert_eq!(valeur(metrique::MRR_NET, "2026-11-12"), Some(947.0 * metrique::PART_NETTE));
    assert_eq!(valeur(metrique::ABONNES, "2026-11-12"), Some(100.0));
    assert_eq!(valeur(metrique::ESSAIS_ACTIFS, "2026-11-11"), Some(20.0));
    assert_eq!(valeur(metrique::REVENU_28J, "2026-11-12"), Some(3268.0));
    assert_eq!(valeur(metrique::REVENU_28J, "2026-11-11"), Some(3200.0));
    assert_eq!(valeur(metrique::ACHATS, "2026-11-12"), Some(2.0));
    // le revenu du jour vient du graphique, aujourd'hui compris
    assert_eq!(valeur(metrique::REVENU, "2026-11-11"), Some(120.0));
    assert_eq!(valeur(metrique::REVENU, "2026-11-12"), Some(35.0));
    let n = store.db.query("select count(*) as n from serie", p![]).unwrap()[0].entier("n");
    assert_eq!(n, Some(points.len() as i64));
}

/// Un graphique demandé avec `segment=store` sur le 11 et le 12 nov. 2026 : par segment, les valeurs de chaque mesure
/// jour par jour.
fn segmente(mesures: &[&str], segments: &[(&str, Vec<Vec<f64>>)]) -> Vec<u8> {
    let liste: Vec<String> =
        segments.iter().map(|(n, _)| format!(r#"{{"display_name":"{n}","is_total":{}}}"#, *n == "Total")).collect();
    let mut valeurs = Vec::new();
    for (s, (_, mesures_)) in segments.iter().enumerate() {
        for (m, jours) in mesures_.iter().enumerate() {
            for (j, v) in jours.iter().enumerate() {
                valeurs.push(format!(r#"{{"cohort":{},"measure":{m},"segment":{s},"value":{v}}}"#, 1794355200 + 86400 * j));
            }
        }
    }
    let m: Vec<String> = mesures.iter().map(|m| format!(r#"{{"display_name":"{m}"}}"#)).collect();
    format!(r#"{{"measures":[{}],"segments":[{}],"values":[{}]}}"#, m.join(","), liste.join(","), valeurs.join(",")).into_bytes()
}

#[test]
fn graphique_par_store() {
    let sans_play = segmente(&["MRR"], &[("Total", vec![vec![900.0, 950.0]]), ("App Store", vec![vec![900.0, 950.0]])]);
    let attendu = serie(&[("2026-11-11", 900.0), ("2026-11-12", 950.0)]);
    assert_eq!(mesure("MRR", &graphique(&sans_play, None).unwrap()).unwrap(), attendu);
    assert_eq!(mesure("MRR", &graphique(&sans_play, Some(Boutique::AppStore)).unwrap()).unwrap(), attendu);
    // Play sans segment : 0 chaque jour du total, pour que sa vue montre zéro et pas un trou
    assert_eq!(
        mesure("MRR", &graphique(&sans_play, Some(Boutique::PlayStore)).unwrap()).unwrap(),
        serie(&[("2026-11-11", 0.0), ("2026-11-12", 0.0)])
    );
    let avec_play = segmente(
        &["MRR"],
        &[("Total", vec![vec![900.0, 1000.0]]), ("App Store", vec![vec![900.0, 750.0]]), ("Play Store", vec![vec![0.0, 250.0]])],
    );
    assert_eq!(
        mesure("MRR", &graphique(&avec_play, Some(Boutique::PlayStore)).unwrap()).unwrap(),
        serie(&[("2026-11-11", 0.0), ("2026-11-12", 250.0)])
    );
    assert!(boutique("Google Play") == Some(Boutique::PlayStore) && boutique("Stripe").is_none());
    assert!(boutique("iOS") == Some(Boutique::AppStore) && boutique("Android") == Some(Boutique::PlayStore));
    assert!(boutique("Unknown").is_none());
}

#[test]
fn stores_somment_au_total() {
    let a = apercu(OVERVIEW.as_bytes()).unwrap();
    let un = |m: &str, total: [f64; 2], app: [f64; 2]| {
        let play = vec![total[0] - app[0], total[1] - app[1]];
        segmente(&[m], &[("Total", vec![total.to_vec()]), ("App Store", vec![app.to_vec()]), ("Play Store", vec![play])])
    };
    let g = Graphiques {
        mrr: un("MRR", [900.0, 1000.0], [900.0, 750.0]),
        abonnes: un("Actives", [90.0, 100.0], [90.0, 80.0]),
        essais: un("Active Trials", [20.0, 25.0], [20.0, 25.0]),
        achats: segmente(
            &["Product Changes", "Total Paid Subscriptions"],
            &[
                ("Total", vec![vec![0.0, 0.0], vec![4.0, 3.0]]),
                ("App Store", vec![vec![0.0, 0.0], vec![4.0, 2.0]]),
                ("Play Store", vec![vec![0.0, 0.0], vec![0.0, 1.0]]),
            ],
        ),
        revenu: un("Revenue", [100.0, 60.0], [100.0, 30.0]),
        nouveaux: segmente(
            &["New Customers"],
            &[
                ("Total", vec![vec![101.0, 47.0]]),
                ("iOS", vec![vec![100.0, 38.0]]),
                ("Android", vec![vec![0.0, 9.0]]),
                ("Unknown", vec![vec![1.0, 0.0]]),
            ],
        ),
    };
    let lus = |b: Option<Boutique>| -> HashMap<String, f64> {
        points(&g, &a, b, "2026-11-11", "2026-11-12").unwrap().into_iter().map(|p| (format!("{} {}", p.metrique, p.jour), p.valeur)).collect()
    };
    let (total, ios, play) = (lus(None), lus(Some(Boutique::AppStore)), lus(Some(Boutique::PlayStore)));
    assert_eq!(play["mrr_brut.play_store 2026-11-11"], 0.0);
    assert!(play["achats.play_store 2026-11-12"] == 1.0 && ios["achats.app_store 2026-11-12"] == 2.0);
    assert_eq!(play["revenu.play_store 2026-11-12"], 30.0);
    // aujourd'hui, l'overview au prorata du graphique : 947 × 250/1000 côté Play
    assert_eq!(play["mrr_brut.play_store 2026-11-12"], 947.0 * 0.25);
    for m in [metrique::MRR_BRUT, metrique::ABONNES, metrique::ESSAIS_ACTIFS, metrique::REVENU_28J] {
        let somme = ios.get(&format!("{} 2026-11-12", Boutique::AppStore.metrique(m))).copied().unwrap_or(-1.0)
            + play.get(&format!("{} 2026-11-12", Boutique::PlayStore.metrique(m))).copied().unwrap_or(-1.0);
        assert!((somme - total.get(&format!("{m} 2026-11-12")).copied().unwrap_or(0.0)).abs() < 1e-9, "{m}");
    }
    assert!(total["mrr_brut 2026-11-12"] == 947.0 && total["mrr_brut 2026-11-11"] == 900.0);
    // nouveaux clients par première plateforme, aujourd'hui compris ; « Unknown » ne va à aucun store
    assert!(total["nouveaux_clients 2026-11-11"] == 101.0 && ios["nouveaux_clients.app_store 2026-11-11"] == 100.0);
    assert!(play["nouveaux_clients.play_store 2026-11-12"] == 9.0 && ios["nouveaux_clients.app_store 2026-11-12"] == 38.0);
}

#[test]
fn revenu_glissant_sur_vingt_huit_jours() {
    let revenu: Serie = jour::derniers(30, "2026-11-12").into_iter().enumerate().map(|(i, j)| (j, (i + 1) as f64)).collect();
    let r = revenus28j(&revenu, "2026-10-13", "2026-11-12");
    // le 9 nov. manque d'un jour, le 10 somme 1…28, le 11 2…29 ; aujourd'hui vient de l'overview
    assert!(!r.contains_key("2026-10-13") && !r.contains_key("2026-11-09"));
    assert_eq!(r["2026-11-10"], (1..=28).sum::<i32>() as f64);
    assert_eq!(r["2026-11-11"], (2..=29).sum::<i32>() as f64);
    assert!(!r.contains_key("2026-11-12"));
}

#[test]
fn requete_comptes_valide() {
    let maintenant = date("2026-11-13T00:00:00Z");
    let q = requete_comptes(&maintenant);
    assert_eq!(q["structuredQuery"]["from"][0]["collectionId"], "users");
    assert_eq!(q["structuredQuery"]["where"]["fieldFilter"]["value"]["timestampValue"], "2026-11-13T00:00:00Z");
}

#[test]
fn comptes_payes_relus_recemment() {
    let reponse = r#"
        [{"document":{"name":"projects/p/databases/(default)/documents/users/uid-paye",
                      "fields":{"premiumTrial":{"booleanValue":false},
                                "entitlementCheckedAt":{"timestampValue":"2026-11-12T20:47:30.123456Z"}}},
          "readTime":"2026-11-13T00:00:00Z"},
         {"document":{"name":"projects/p/databases/(default)/documents/users/uid-essai",
                      "fields":{"premiumTrial":{"booleanValue":true},
                                "entitlementCheckedAt":{"timestampValue":"2026-11-12T20:00:00.5Z"}}}},
         {"document":{"name":"projects/p/databases/(default)/documents/users/uid-ancien",
                      "fields":{"entitlementCheckedAt":{"timestampValue":"2026-11-01T10:00:00Z"}}}},
         {"document":{"name":"projects/p/databases/(default)/documents/users/uid-vide","fields":{}}}]"#;
    let depuis = date("2026-11-12T00:00:00Z");
    assert_eq!(comptes_payes(reponse.as_bytes(), &depuis).unwrap(), ["uid-paye"]);
    assert!(comptes_payes(br#"[{"readTime":"2026-11-13T00:00:00Z"}]"#, &depuis).unwrap().is_empty());
}

#[test]
fn abonnements_de_production_recents() {
    let reponse = r#"
        {"items":[{"object":"subscription","id":"sub_recent","starts_at":1794132000000,"environment":"production","country":"FR","status":"active"},
                  {"object":"subscription","id":"sub_sandbox","starts_at":1794132000000,"environment":"sandbox","country":"FR"},
                  {"object":"subscription","id":"sub_ancien","starts_at":1792490400000,"environment":"production","country":null}],
         "next_page":null}"#;
    let a = abonnements(reponse.as_bytes(), &date("2026-11-01T00:00:00Z")).unwrap();
    let attendu = Abonnement { id: "sub_recent".into(), starts_at: 1794132000000.0, environment: "production".into(), country: Some("FR".into()) };
    assert_eq!(a, [attendu]);
}

#[test]
fn achat_apres_essai() {
    let store = essai::store();
    let transactions = r#"
        {"items":[{"object":"transaction","id":"t2","purchased_at":1794492042000,"product_store_identifier":"Yearly",
                   "revenue_in_local_currency":{"currency":"EUR","gross":69.99,"proceeds":59.49,"tax":11.67,"commission":8.75}},
                  {"object":"transaction","id":"t1","purchased_at":1793872800000,"product_store_identifier":"Yearly",
                   "revenue_in_local_currency":{"currency":"EUR","gross":0.0,"proceeds":0.0,"tax":0.0,"commission":0.0}}]}"#;
    let a = Abonnement { id: "sub1".into(), starts_at: 1793872800000.0, environment: "production".into(), country: Some("FR".into()) };
    let e = achat("revenuecat:achat:sub1", &a, transactions.as_bytes(), "roleo").unwrap().unwrap();
    assert_eq!(e.type_, TypeFil::Achat);
    assert_eq!(e.at.timestamp(), 1794492042);
    assert!(e.texte.starts_with("Annuel après essai, 69,99"));
    assert!(e.texte.ends_with("€, France"));
    assert!(store.fil(&e).unwrap());
    let l = store.db.query("select texte from fil where id = 'revenuecat:achat:sub1' and type = 'ACHAT'", p![]).unwrap();
    assert_eq!(l.len(), 1);
}

#[test]
fn achat_direct_et_essai_en_cours() {
    let direct = r#"
        {"items":[{"id":"t1","purchased_at":1794516438000,"product_store_identifier":"roleo_quarterly:trimestriel",
                   "revenue_in_local_currency":{"currency":"CAD","gross":29.99}}]}"#;
    let a = Abonnement { id: "sub2".into(), starts_at: 1794516438000.0, environment: "production".into(), country: None };
    let e = achat("revenuecat:achat:sub2", &a, direct.as_bytes(), "roleo").unwrap().unwrap();
    assert!(e.texte.starts_with("Trimestriel, 29,99"));
    assert!(!e.texte.contains("essai"));
    let essai = r#"{"items":[{"id":"t1","purchased_at":1794516438000,"product_store_identifier":"Yearly","revenue_in_local_currency":{"currency":"EUR","gross":0}}]}"#;
    assert!(achat("revenuecat:achat:sub3", &a, essai.as_bytes(), "roleo").unwrap().is_none());
}

#[test]
fn prix_a_la_francaise() {
    assert_eq!(prix(69.99, "EUR"), "69,99\u{A0}€");
    assert_eq!(prix(1069.5, "USD"), "1\u{202F}069,50\u{A0}$US");
    assert_eq!(prix(1200.0, "JPY"), "1\u{202F}200\u{A0}JPY");
}

#[test]
fn paiement_du_client() {
    let etat = |items: &str| paiement(format!(r#"{{"items":[{items}]}}"#).as_bytes()).unwrap();
    let paye = r#"{"status":"active","environment":"production","total_revenue_in_usd":{"gross":69.99}}"#;
    let essai = r#"{"status":"trialing","environment":"production","total_revenue_in_usd":{"gross":0}}"#;
    let fini = r#"{"status":"expired","environment":"production","total_revenue_in_usd":{"gross":0}}"#;
    let sandbox = r#"{"status":"active","environment":"sandbox","total_revenue_in_usd":{"gross":9.99}}"#;
    assert_eq!(etat(&format!("{paye},{essai}")), EtatPaiement::Paye);
    assert_eq!(etat(&format!("{essai},{fini}")), EtatPaiement::Essai);
    assert_eq!(etat(fini), EtatPaiement::Non);
    assert_eq!(etat(sandbox), EtatPaiement::Inconnu);
    assert_eq!(etat(""), EtatPaiement::Inconnu);
}

#[test]
fn formules() {
    assert_eq!(formule("monthly"), "Mensuel");
    assert_eq!(formule("Yearly"), "Annuel");
    assert_eq!(formule("roleo_annual_downsell"), "Annuel");
    assert_eq!(formule("roleo_weekly:hebdo"), "Hebdo");
    assert_eq!(formule("roleo_quarterly"), "Trimestriel");
    assert_eq!(formule("lifetime"), "lifetime");
}

#[test]
fn client_encode() {
    assert_eq!(encoder_client("$RCAnonymousID:ab-c_1"), "%24RCAnonymousID%3Aab-c_1");
}

#[test]
fn erreur_nomme_le_business() {
    let e = nomme(Erreur::Http(401, "clé refusée".into()), &essai::roleo());
    assert_eq!(e.to_string(), "HTTP 401 : Roleo : clé refusée");
}
