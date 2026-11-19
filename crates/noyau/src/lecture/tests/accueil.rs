use super::{d, lectures, video};
use crate::essai::store;
use crate::jour;
use crate::lecture::accueil::{semaine, AxeScore, Score};
use crate::metrique;
use crate::modeles::LignePlan;
use std::collections::HashMap;

#[test]
fn plan_dans_l_ordre_de_la_note() {
    let s = store();
    let ligne = |section: &str, texte: &str, coche, fait| LignePlan { section: section.into(), texte: texte.into(), coche, fait };
    let lignes = vec![ligne("Cette semaine", "Sortir le build 51", true, true), ligne("Cette semaine", "3 vidéos par jour", true, false),
                      ligne("Idées", "Carte de leçon", false, false)];
    s.plan(&lignes).unwrap();
    assert_eq!(lectures(&s).plan().unwrap(), lignes);
    s.plan(&lignes[..1]).unwrap();
    assert_eq!(lectures(&s).plan().unwrap(), lignes[..1]);
}

#[test]
fn reseau_compte_les_videos_du_jour_a_paris() {
    let s = store();
    // minuit à Paris le 13 novembre = 23 h UTC le 12
    for (id, compte, publiee) in [("1", "holaestony", "2026-11-12T22:30:00Z"), ("2", "holaestony", "2026-11-13T09:00:00Z"),
                                  ("3", "holaestony", "2026-11-13T12:00:00Z"), ("4", "tonyolingo", "2026-11-11T10:00:00Z")] {
        s.video(&video(id, compte, id, Some(publiee))).unwrap();
    }
    let r = lectures(&s).reseau().unwrap();
    let hola = r.iter().find(|c| c.id() == "tiktok:holaestony").unwrap();
    assert_eq!(hola.aujourdhui, 2);
    assert_eq!(hola.derniere, Some(d("2026-11-13T12:00:00Z")));
    let tony = r.iter().find(|c| c.id() == "tiktok:tonyolingo").unwrap();
    assert_eq!(tony.aujourdhui, 0);
    assert_eq!(tony.derniere, Some(d("2026-11-11T10:00:00Z")));
    assert_eq!(r.iter().find(|c| c.id() == "instagram:holaestony").unwrap().derniere, None);
}

#[test]
fn semaine_du_lundi_au_dimanche() {
    assert_eq!(semaine("2026-11-13"), (0..7).map(|i| jour::decale("2026-11-09", i)).collect::<Vec<_>>());
    assert_eq!(semaine("2026-11-13").last().map(String::as_str), Some("2026-11-15"));
    assert_eq!(semaine("2026-11-15")[0], "2026-11-09");
    assert_eq!(semaine("2026-11-16")[0], "2026-11-16");
}

#[test]
fn score_semaine_contre_semaine() {
    let s = store();
    // semaine d'avant : 6 au 12 nov. moins 7 jours ; semaine courante : 6 au 12
    for j in 0..7 {
        let (avant, courant) = (jour::decale("2026-10-30", j), jour::decale("2026-11-06", j));
        for (m, a, c) in [(metrique::INSTALLS, 10.0, 20.0), (metrique::ESSAIS, 2.0, 2.0), (metrique::REVENU, 100.0, 80.0)] {
            s.serie("roleo", m, &avant, -1, a).unwrap();
            s.serie("roleo", m, &courant, -1, c).unwrap();
        }
    }
    let sc = lectures(&s).score("2026-11-12").unwrap();
    let notes: HashMap<&str, Option<i64>> = sc.axes.iter().map(|a| (a.nom.as_str(), a.note)).collect();
    assert_eq!(notes["Installs"], Some(100));
    assert_eq!(notes["Conversion"], Some(50));
    assert_eq!(notes["Revenu"], Some(80));
    assert_eq!(notes["Vidéos"], None);
    assert_eq!(notes["Vues"], None);
    assert_eq!(sc.global(), Some(77));
    assert_eq!(sc.faible().map(|a| a.nom.as_str()), Some("Conversion"));
}

#[test]
fn pas_de_point_faible_quand_tout_va_bien() {
    let s = Score { axes: vec![AxeScore { nom: "Vues".into(), note: Some(100) }, AxeScore { nom: "Revenu".into(), note: None }] };
    assert_eq!(s.global(), Some(100));
    assert_eq!(s.faible(), None);
    assert_eq!(Score::note(Some(5.0), Some(0.0)), None);
    assert_eq!(Score::note(Some(30.0), Some(10.0)), Some(100));
}
