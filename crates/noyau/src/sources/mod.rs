//! Les sources, une par service (docs/sources/).
pub mod app_store_connect;
pub mod pays_fr;
pub mod play_console;
pub mod posthog;
pub mod revenuecat;

use crate::collecte::Source;
use std::sync::Arc;

/// Toutes les sources, dans l'ordre de Santé.
pub fn toutes() -> Vec<Arc<dyn Source>> {
    vec![
        Arc::new(posthog::PostHog),
        Arc::new(revenuecat::RevenueCat),
        Arc::new(app_store_connect::AppStoreConnect),
        Arc::new(play_console::PlayConsole),
    ]
}
