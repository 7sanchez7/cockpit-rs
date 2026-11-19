//! Les sources, une par service (docs/sources/).
pub mod app_store_connect;
pub mod classements;
pub mod contenu;
pub mod facebook;
pub mod firebase;
pub mod instagram;
pub mod notes;
pub mod pays_fr;
pub mod play_console;
pub mod posthog;
pub mod revenuecat;
pub mod tiktok;
pub mod veille;
pub mod youtube;

use crate::collecte::Source;
use std::sync::Arc;

/// Toutes les sources, dans l'ordre de Santé.
pub fn toutes() -> Vec<Arc<dyn Source>> {
    vec![
        Arc::new(posthog::PostHog),
        Arc::new(revenuecat::RevenueCat),
        Arc::new(app_store_connect::AppStoreConnect),
        Arc::new(play_console::PlayConsole),
        Arc::new(tiktok::TikTok),
        Arc::new(instagram::Instagram),
        Arc::new(youtube::YouTube),
        Arc::new(facebook::Facebook),
        Arc::new(contenu::Contenu),
        Arc::new(firebase::Firebase),
        Arc::new(classements::Classements),
        Arc::new(notes::Notes),
        Arc::new(veille::Veille),
    ]
}
