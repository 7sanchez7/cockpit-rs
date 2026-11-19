//! Les sources, une par service (docs/sources/).
use crate::collecte::Source;
use std::sync::Arc;

/// Toutes les sources, dans l'ordre de Santé.
pub fn toutes() -> Vec<Arc<dyn Source>> { vec![] }
