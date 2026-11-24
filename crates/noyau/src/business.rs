//! Un business suivi par le cockpit et sa configuration (en JSON dans `business.config`).
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Business {
    pub id: String,
    pub nom: String,
    pub lettre: String,
    pub teinte: String,
    pub config: Config,
}

impl<'de> Deserialize<'de> for Business {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Brut { id: String, nom: String, lettre: Option<String>, teinte: Option<String>, config: Option<Config> }
        let b = Brut::deserialize(d)?;
        let lettre = b.lettre.unwrap_or_else(|| b.nom.chars().take(1).collect::<String>().to_uppercase());
        Ok(Business { id: b.id, nom: b.nom, lettre, teinte: b.teinte.unwrap_or_else(|| "menthe".into()), config: b.config.unwrap_or_default() })
    }
}

/// Mêmes clés JSON que la version macOS ; une clé absente prend sa valeur par défaut.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    #[serde(rename = "appStoreId", skip_serializing_if = "Option::is_none")]
    pub app_store_id: Option<String>,
    #[serde(rename = "ascIssuer", skip_serializing_if = "Option::is_none")]
    pub asc_issuer: Option<String>,
    #[serde(rename = "ascVendor", skip_serializing_if = "Option::is_none")]
    pub asc_vendor: Option<String>,
    #[serde(rename = "playPackage", skip_serializing_if = "Option::is_none")]
    pub play_package: Option<String>,
    #[serde(rename = "playBucket", skip_serializing_if = "Option::is_none")]
    pub play_bucket: Option<String>,
    #[serde(rename = "firebaseProject", skip_serializing_if = "Option::is_none")]
    pub firebase_project: Option<String>,
    #[serde(rename = "posthogHost", skip_serializing_if = "Option::is_none")]
    pub posthog_host: Option<String>,
    #[serde(rename = "contenuDir", skip_serializing_if = "Option::is_none")]
    pub contenu_dir: Option<String>,
    pub pays: Vec<String>,
    pub genre: i64,
    #[serde(rename = "objectifMensuelEUR")]
    pub objectif_mensuel_eur: f64,
    pub evenements: Evenements,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            app_store_id: None, asc_issuer: None, asc_vendor: None, play_package: None, play_bucket: None,
            firebase_project: None, posthog_host: None, contenu_dir: None,
            pays: vec![], genre: 6017, objectif_mensuel_eur: 10_000.0, evenements: Evenements::default(),
        }
    }
}

/// Les noms des événements PostHog que lit la source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Evenements {
    pub installation: String,
    pub etape: String,
    pub paywall: String,
    pub essai: String,
    pub achat: String,
    /// Rond commencé, puis rond fini.
    pub commence: String,
    pub usage: String,
    /// Les événements gardés un par un, avec leurs propriétés (table `evenement`).
    pub detail: Vec<String>,
}

impl Default for Evenements {
    fn default() -> Self {
        Evenements {
            installation: "onboarding_started".into(), etape: "onboarding_step_completed".into(),
            paywall: "paywall_viewed".into(), essai: "trial_started".into(), achat: "purchase_completed".into(),
            commence: "path_round_started".into(), usage: "path_round_done".into(),
            detail: [
                "onboarding_started", "onboarding_step_completed", "onboarding_completed", "onboarding_failed",
                "paywall_viewed", "paywall_cta_tapped", "trial_started", "purchase_completed", "purchase_failed",
                "path_round_started", "path_reply", "path_reply_failed", "path_help_used", "path_round_paused",
                "path_round_left", "path_round_done", "path_bilan_viewed", "path_bilan_answered", "path_bilan_closed",
                "app_opened", "notif_opened", "notif_prompt_answered", "rating_sheet_answered",
            ]
            .map(String::from)
            .into(),
        }
    }
}

/// Le fichier `graine.json` : les businesses et comptes de cette machine, hors du dépôt.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Graine {
    pub businesses: Vec<Business>,
    pub comptes: Vec<crate::modeles::Compte>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defauts() {
        let b: Business = serde_json::from_str(r#"{"id":"roleo","nom":"roleo","config":{"pays":["fr"],"evenements":{"essai":"x"}}}"#).unwrap();
        assert_eq!(b.lettre, "R");
        assert_eq!(b.teinte, "menthe");
        assert_eq!(b.config.genre, 6017);
        assert_eq!(b.config.evenements.essai, "x");
        assert_eq!(b.config.evenements.achat, "purchase_completed");
        assert!(b.config.evenements.detail.iter().any(|x| x == "path_reply"));
        let s = serde_json::to_string(&b.config).unwrap();
        assert!(s.contains("objectifMensuelEUR") && !s.contains("appStoreId"));
    }
}
