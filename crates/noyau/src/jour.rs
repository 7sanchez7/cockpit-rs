//! Jours à Paris (`yyyy-MM-dd`) et dates ISO, comme les écrit la version macOS.
use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Timelike, Utc};
use chrono_tz::Europe::Paris;

pub type Date = DateTime<Utc>;

pub fn maintenant() -> Date { Utc::now() }

/// La date la plus ancienne, pour « jamais ».
pub fn loin_passe() -> Date { Utc.with_ymd_and_hms(1, 1, 1, 0, 0, 0).unwrap() }

/// ISO 8601 sans fractions, en UTC : `2026-11-19T08:00:00Z`.
pub fn iso(d: &Date) -> String { d.format("%Y-%m-%dT%H:%M:%SZ").to_string() }

/// Lit une date ISO, avec ou sans fractions de seconde.
pub fn depuis_iso(s: &str) -> Option<Date> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
}

pub fn aujourdhui() -> String { cle(&maintenant()) }

pub fn cle(d: &Date) -> String { d.with_timezone(&Paris).format("%Y-%m-%d").to_string() }

/// Minuit à Paris ce jour-là.
pub fn date(cle: &str) -> Option<Date> {
    let n = NaiveDate::parse_from_str(cle, "%Y-%m-%d").ok()?;
    Paris.from_local_datetime(&n.and_hms_opt(0, 0, 0)?).earliest().map(|d| d.with_timezone(&Utc))
}

pub fn decale(cle: &str, jours: i64) -> String {
    match NaiveDate::parse_from_str(cle, "%Y-%m-%d") {
        Ok(n) => (n + Duration::days(jours)).format("%Y-%m-%d").to_string(),
        Err(_) => cle.to_string(),
    }
}

pub fn heure(d: &Date) -> i64 { d.with_timezone(&Paris).hour() as i64 }

/// Les `n` derniers jours finissant à `fin` inclus, du plus ancien au plus récent.
pub fn derniers(n: i64, fin: &str) -> Vec<String> { (0..n).rev().map(|i| decale(fin, -i)).collect() }

/// Le jour de la semaine, lundi = 0.
pub fn jour_semaine(cle: &str) -> Option<u32> {
    NaiveDate::parse_from_str(cle, "%Y-%m-%d").ok().map(|n| n.weekday().num_days_from_monday())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cles_et_iso() {
        let d = depuis_iso("2026-03-29T00:30:00Z").unwrap();
        assert_eq!(cle(&d), "2026-03-29");
        assert_eq!(heure(&d), 1);
        assert_eq!(iso(&d), "2026-03-29T00:30:00Z");
        assert_eq!(depuis_iso("2026-03-29T00:30:00.123Z").map(|d| iso(&d)), Some(iso(&d)));
        assert_eq!(decale("2026-03-01", -1), "2026-02-28");
        assert_eq!(derniers(3, "2026-01-02"), vec!["2025-12-31", "2026-01-01", "2026-01-02"]);
        assert_eq!(date("2026-07-01").map(|d| iso(&d)).as_deref(), Some("2026-06-30T22:00:00Z"));
    }
}
