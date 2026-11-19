//! Les sources, une par service (docs/sources/).
use crate::collecte::Source;
use std::sync::Arc;

pub mod contenu;
pub mod facebook;
pub mod instagram;
pub mod tiktok;
pub mod youtube;

/// Toutes les sources, dans l'ordre de Santé.
pub fn toutes() -> Vec<Arc<dyn Source>> {
    vec![
        Arc::new(tiktok::TikTok),
        Arc::new(instagram::Instagram),
        Arc::new(youtube::YouTube),
        Arc::new(facebook::Facebook),
        Arc::new(contenu::Contenu),
    ]
}
