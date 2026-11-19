//! Les sources, une par service (docs/sources/).
pub mod classements;
pub mod firebase;
pub mod notes;
pub mod veille;

use crate::collecte::Source;
use std::sync::Arc;

/// Toutes les sources, dans l'ordre de Santé.
pub fn toutes() -> Vec<Arc<dyn Source>> { vec![Arc::new(firebase::Firebase), Arc::new(classements::Classements), Arc::new(notes::Notes), Arc::new(veille::Veille)] }
