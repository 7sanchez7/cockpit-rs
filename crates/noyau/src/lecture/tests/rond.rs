use super::maintenant;
use crate::essai::store;
use crate::lecture::data::Filtre;
use crate::lecture::rond::{JourRonds, ParJour, PremiersRonds};
use crate::business::Business;
use crate::lecture::Lectures;
use crate::modeles::{EvenementApp, Install};
use crate::sources::posthog::CLE_EVENEMENTS_DEPUIS;
use crate::store::Store;

fn roleo(s: &Store) -> Lectures<'_> {
    let seul: Vec<Business> = s.businesses().unwrap().into_iter().filter(|b| b.id == "roleo").collect();
    Lectures { maintenant: maintenant(), ..Lectures::new(&s.db, seul) }
}

fn evenement(uuid: &str, at: &str, nom: &str, props: &str) -> EvenementApp {
    EvenementApp {
        uuid: uuid.into(), personne: "p".into(), at: at.into(), jour: at[..10].into(), nom: nom.into(),
        plateforme: "ios".into(), build: "54".into(), props: props.into(), recu_ms: 0,
    }
}

#[test]
fn premiers_ronds_par_jour_d_install() {
    let s = store();
    let install = |jour: &str, commence: bool, rond: bool, ronds: i64, delai: Option<i64>, essai: bool| Install {
        jour: jour.into(), commence, rond, ronds, jours_ronds: ronds.min(2), premier_rond: delai, essai,
        ..Default::default()
    };
    s.installs("roleo", "2026-10-01", &[
        install("2026-10-10", true, true, 1, Some(10), false),
        install("2026-11-10", true, true, 3, Some(60), true),
        install("2026-11-10", true, false, 0, Some(600), false),
        install("2026-11-10", false, false, 0, None, false),
        install("2026-11-12", true, true, 1, Some(120), false),
    ]).unwrap();
    let l = roleo(&s);
    let jours = l.premiers_ronds(&[], "2026-11-10", "2026-11-12").unwrap();
    assert_eq!(jours.iter().map(|j| j.jour.as_str()).collect::<Vec<_>>(), ["2026-11-10", "2026-11-11", "2026-11-12"]);
    assert_eq!(jours[1], PremiersRonds { jour: "2026-11-11".into(), ..Default::default() });
    let t = PremiersRonds::somme(&jours);
    assert_eq!((t.installs, t.commence, t.fini, t.ronds, t.jours), (4, 3, 2, 4, 3));
    assert_eq!(t.jours_par_fidele(), Some(1.5));
    assert_eq!(t.delai(), Some(120.0));
    let essais = PremiersRonds::somme(&l.premiers_ronds(&[Filtre::Essai], "2026-11-10", "2026-11-12").unwrap());
    assert_eq!((essais.installs, essais.fini, essais.delais), (1, 1, vec![60.0]));
}

#[test]
fn ronds_par_jour_lit_les_proprietes() {
    let s = store();
    s.evenements_app("roleo", &[
        evenement("a", "2026-11-12T09:00:00Z", "path_round_started", "{}"),
        evenement("b", "2026-11-12T09:01:00Z", "path_round_started", "{}"),
        evenement("c", "2026-11-12T09:02:00Z", "path_round_left", r#"{"last_verdict":"right"}"#),
        // sortie à l'ouverture du rond, sans réponse : ne compte pas
        evenement("c2", "2026-11-12T09:02:30Z", "path_round_left", r#"{"elapsed_s":2}"#),
        evenement("d", "2026-11-12T09:03:00Z", "path_reply", r#"{"tutor_ms":1200}"#),
        evenement("e", "2026-11-12T09:04:00Z", "path_reply", r#"{"tutor_ms":800}"#),
        evenement("f", "2026-11-12T09:05:00Z", "path_round_done",
            r#"{"replies":6,"rights":4,"helps":1,"voice_replies":3,"said":3,"forms":4,"elapsed_s":150}"#),
        // sans said ni forms : compte dans les réponses, pas dans les formes
        evenement("g", "2026-11-12T10:00:00Z", "path_round_done", r#"{"replies":2,"rights":2,"helps":0,"voice_replies":0,"elapsed_s":90}"#),
        evenement("h", "2026-11-12T10:01:00Z", "path_bilan_closed", r#"{"how":"finish","answered":2,"missed":1}"#),
        evenement("i", "2026-11-12T10:02:00Z", "path_bilan_closed", r#"{"how":"closed"}"#),
        evenement("j", "2026-11-12T10:03:00Z", "rating_sheet_answered", r#"{"answer":"yes"}"#),
        evenement("k", "2026-11-12T10:04:00Z", "app_opened", "{}"),
        evenement("l", "2026-11-12T10:05:00Z", "notif_opened", "{}"),
        evenement("m", "2026-11-12T10:06:00Z", "paywall_viewed", "{}"),
        evenement("n", "2026-11-13T08:00:00Z", "path_round_started", "{}"),
    ]).unwrap();
    let l = roleo(&s);
    let jours = l.ronds_par_jour("2026-11-11", "2026-11-13", None).unwrap();
    assert_eq!(jours.len(), 3);
    assert_eq!(jours[0], JourRonds { jour: "2026-11-11".into(), ..Default::default() });
    let j = &jours[1];
    assert_eq!((j.commences, j.finis, j.quittes, j.reponses, j.justes, j.voix), (2, 2, 1, 8, 6, 3));
    assert_eq!((j.dites, j.formes), (3, 4));
    assert_eq!((j.bilans_fermes, j.bilans_finis, j.quiz, j.quiz_rates), (2, 1, 2, 1));
    assert_eq!((j.ouvertures, j.notifs, j.avis, j.avis_oui), (1, 1, 1, 1));
    assert_eq!(j.duree(), Some(120.0));
    assert_eq!(j.tuteur(), Some(1.0));
    assert_eq!(j.quittes(), Some(0.5));
    assert_eq!(j.quiz_juste(), Some(0.5));
    assert_eq!(j.bilan_fini(), Some(0.5));
    // arrêté à 10 h UTC : les événements d'après ne comptent pas
    let tot = &l.ronds_par_jour("2026-11-12", "2026-11-12", Some("2026-11-12T09:30:00Z")).unwrap()[0];
    assert_eq!((tot.finis, tot.ouvertures, tot.durees.clone()), (1, 0, vec![150.0]));
    assert_eq!(JourRonds::somme(&jours).commences, 3);
}

#[test]
fn evenements_depuis_attend_chaque_business() {
    let s = store();
    let app2: Business = serde_json::from_str(r#"{"id": "app2", "nom": "App 2", "lettre": "A", "teinte": "ciel"}"#).unwrap();
    s.enregistrer(&app2).unwrap();
    let tous = Lectures { maintenant: maintenant(), ..Lectures::new(&s.db, s.businesses().unwrap()) };
    assert_eq!(roleo(&s).evenements_depuis().unwrap(), None);
    s.regler(&format!("{CLE_EVENEMENTS_DEPUIS}roleo"), Some("2026-08-08")).unwrap();
    assert_eq!(roleo(&s).evenements_depuis().unwrap().as_deref(), Some("2026-08-08"));
    assert_eq!(tous.evenements_depuis().unwrap(), None);
    s.regler(&format!("{CLE_EVENEMENTS_DEPUIS}app2"), Some("2026-09-01")).unwrap();
    assert_eq!(tous.evenements_depuis().unwrap().as_deref(), Some("2026-09-01"));
}
