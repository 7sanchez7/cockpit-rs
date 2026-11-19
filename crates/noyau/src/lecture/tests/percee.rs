use super::{maintenant, video};
use crate::essai::store;
use crate::lecture::percee::{compte, levier, spearman, Mesure};
use crate::lecture::{LigneVideo, Lectures};
use crate::modeles::{Analyse, Plateforme};
use std::collections::HashMap;

/// Dix vidéos de 100 à 1 000 vues : le taux de likes monte avec les vues, celui des commentaires descend.
fn videos(likes: impl Fn(i64) -> i64) -> Vec<LigneVideo> {
    (1..=10).map(|i| LigneVideo {
        id: i.to_string(), business: "roleo".into(), compte: "holaestony".into(), plateforme: Plateforme::Tiktok,
        titre: i.to_string(), format: None, fiche: None, url: None, publiee_le: None, duree_s: None, verdict: false,
        vues: 100 * i, likes: Some(likes(i)), commentaires: Some((11 - i) * i), partages: None, enregistrements: None,
        completion: None, watch_moyen_s: None, abonnes: None, gain24h: None, releve_le: maintenant(),
    }).collect()
}

fn carre(i: i64) -> i64 { i * i }

#[test]
fn spearman_ex_aequo() {
    assert_eq!(spearman(&[(1.0, 1.0), (2.0, 2.0), (3.0, 3.0)]), 1.0);
    assert_eq!(spearman(&[(1.0, 3.0), (2.0, 2.0), (3.0, 1.0)]), -1.0);
    assert_eq!(spearman(&[(1.0, 2.0), (2.0, 2.0), (3.0, 2.0)]), 0.0);
    // ex aequo au rang moyen
    assert!((spearman(&[(1.0, 1.0), (2.0, 1.0), (3.0, 2.0)]) - 0.866).abs() < 0.001);
}

#[test]
fn le_levier_est_la_mesure_qui_porte_les_vues() {
    let v = videos(carre);
    let l = levier(&v[9], &v, &HashMap::new()).unwrap();
    assert_eq!(l.mesure, Mesure::Likes);
    assert_eq!(l.valeur, 0.1);
    assert_eq!(l.rang, 1.0);
    assert_eq!(l.poids, 1.0);
    assert!(l.percee);
}

#[test]
fn une_mesure_qui_suit_mal_les_vues_n_est_jamais_le_levier() {
    // la vidéo la moins vue commente le plus, mais les commentaires descendent quand les vues montent
    let v = videos(carre);
    assert_eq!(levier(&v[0], &v, &HashMap::new()), None);
}

#[test]
fn sous_la_mediane_le_meilleur_atout() {
    let v = videos(|i| if i == 3 { 60 } else { i * i });
    let l = levier(&v[2], &v, &HashMap::new()).unwrap();
    assert_eq!(l.mesure, Mesure::Likes);
    assert_eq!(l.rang, 1.0);
    assert!(!l.percee);
}

#[test]
fn les_comptes_ne_se_melangent_pas() {
    let mut v: Vec<LigneVideo> = videos(carre).into_iter().take(7).collect();
    v.extend(videos(carre).into_iter().map(|mut x| { x.id += "-autre"; x.compte = "tonyolingo".into(); x }));
    assert_eq!(compte(&v[6], &v).len(), 7);
    assert_eq!(levier(&v[6], &v, &HashMap::new()), None);
}

#[test]
fn accroche_lue_dans_la_courbe() {
    let s = store();
    for id in ["1", "2"] {
        let mut x = video(id, "holaestony", id, None);
        x.publiee_le = Some(maintenant());
        x.duree_s = Some(10.0);
        s.video(&x).unwrap();
    }
    s.analyse("1", &maintenant(), &Analyse { retention: vec![1.0, 0.8, 0.7, 0.58, 0.5], ..Default::default() }).unwrap();
    s.analyse("2", &maintenant(), &Analyse { retention: vec![1.0, 0.6], ..Default::default() }).unwrap();
    let l = Lectures { maintenant: maintenant(), ..Lectures::new(&s.db, s.businesses().unwrap()) };
    assert_eq!(l.accroches().unwrap(), HashMap::from([("1".to_string(), 0.58)]));
}
