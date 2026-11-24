//! Coût IA en euros : le texte d'après les tokens (la formule `textCostEUR` de Roleo), la voix d'après les caractères.
//! Voir docs/sources/couts.md avant de changer un prix.
use crate::erreur::{format_err, Res};
use crate::metrique::EUR_PAR_USD;
use std::collections::BTreeMap;

/// Claude Haiku 5.5, dollars par million de tokens.
pub const ENTREE_USD: f64 = 0.10;
pub const SORTIE_USD: f64 = 0.50;
/// Part d'entrée non cachée dans les tokens facturables, le reste est de la sortie (mélange observé par Roleo).
pub const PART_ENTREE: f64 = 0.8;

/// Dollars par million de caractères, par moteur de `aiLoad.m.<minute>.v` (`TTSProvider` de Roleo).
pub fn voix_usd(moteur: &str) -> Option<f64> {
    match moteur {
        "inworld" => Some(9.0),   // Inworld TTS-2 Flash, plan Builder
        "voxtral3" => Some(13.0), // Voxtral 3, Mistral (jusqu'au 22 nov. 2026)
        "kokoro" => Some(1.485),  // Kokoro, facture DeepInfra
        "voxtral" => Some(16.0),  // Voxtral mini
        "gemini" => Some(20.0),   // Gemini Flash TTS
        _ => None,
    }
}

/// Dictée Inworld STT, dollars par heure d'audio, plan Builder.
pub const DICTEE_USD_HEURE: f64 = 0.10;

pub fn euros_tokens(tokens: f64) -> f64 {
    if tokens <= 0.0 {
        return 0.0;
    }
    tokens * (PART_ENTREE * ENTREE_USD + (1.0 - PART_ENTREE) * SORTIE_USD) / 1_000_000.0 * EUR_PAR_USD
}

pub fn euros_caracteres(caracteres: f64, usd_par_million: f64) -> f64 { caracteres * usd_par_million / 1_000_000.0 * EUR_PAR_USD }

pub fn euros_dictee(ms: f64) -> f64 { ms / 3_600_000.0 * DICTEE_USD_HEURE * EUR_PAR_USD }

/// Le coût de caractères par moteur ; un moteur sans prix lève `Format` plutôt que de compter 0.
pub fn euros_voix(voix: &BTreeMap<String, i64>) -> Res<f64> {
    voix.iter().try_fold(0.0, |total, (moteur, n)| {
        let prix = voix_usd(moteur).ok_or_else(|| format_err(format!("moteur de voix sans prix : {moteur}")))?;
        Ok(total + euros_caracteres(*n as f64, prix))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cout() {
        assert!((euros_tokens(1_000_000.0) - 0.1656).abs() < 1e-12);
        assert!((euros_tokens(3_058_771.0) - 0.50653).abs() < 1e-5);
        assert_eq!(euros_tokens(0.0), 0.0);
        assert_eq!(euros_tokens(-5.0), 0.0);
        assert!((euros_caracteres(1_000_000.0, 13.0) - 11.96).abs() < 1e-12);
        let voix = euros_voix(&BTreeMap::from([
            ("voxtral3".to_string(), 1_000_000), ("kokoro".to_string(), 2_000_000),
            ("voxtral".to_string(), 500_000), ("gemini".to_string(), 250_000), ("inworld".to_string(), 1_000_000),
        ])).unwrap();
        assert!((voix - (13.0 + 2.0 * 1.485 + 0.5 * 16.0 + 0.25 * 20.0 + 9.0) * 0.92).abs() < 1e-9);
        assert!((euros_dictee(7_200_000.0) - 0.184).abs() < 1e-12);
        assert!(euros_voix(&BTreeMap::from([("inconnu".to_string(), 1)])).is_err());
    }
}
