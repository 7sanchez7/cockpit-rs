use super::d;
use crate::lecture::format::*;
use chrono::Duration;

const ESPACE: char = '\u{202F}';

#[test]
fn nombres() {
    assert_eq!(entier(48_200.0), format!("48{ESPACE}200"));
    assert_eq!(compact(337_500.0), "338 k");
    assert_eq!(compact(25_400.0), "25,4 k");
    assert_eq!(compact(48_000.0), "48 k");
    assert_eq!(compact(2_000_000.0), "2 M");
    assert_eq!(compact(6420.0), format!("6{ESPACE}420"));
    assert_eq!(delta(-96.0, false), "\u{2212}96");
    assert_eq!(delta(0.0, false), "+0");
    assert_eq!(points(Some(0.123), Some(0.1)).as_deref(), Some("+2,3 pt"));
    assert_eq!(points(Some(0.05), Some(0.1)).as_deref(), Some("\u{2212}5,0 pt"));
    assert_eq!(points(Some(0.1), Some(0.1)).as_deref(), Some("= 0 pt"));
    assert_eq!(points(Some(0.1), None), None);
    assert_eq!(pourcent(0.0099, 2), "0,99 %");
    assert_eq!(variation(386.0, 354.0).as_deref(), Some("+9 %"));
    assert_eq!(variation(147.0, 153.0).as_deref(), Some("\u{2212}4 %"));
    assert_eq!(variation(3.0, 0.0), None);
}

#[test]
fn argent() {
    assert_eq!(euros(7.1), "7,10 €");
    assert_eq!(rang(152), "152e");
    assert_eq!(rang(1), "1er");
}

#[test]
fn dates() {
    let x = d("2026-11-12T13:52:00Z");
    assert_eq!(heure(&x), "14:52");
    assert_eq!(date_longue(&x), "Jeudi 12 novembre");
    assert_eq!(age(&x, &d("2026-11-13T09:00:00Z")), "J+1");
    assert_eq!(age(&x, &d("2026-12-01T09:00:00Z")), "19 j");
    assert_eq!(il_ya(&x, &(x + Duration::seconds(190))), "il y a 3 min");
}
