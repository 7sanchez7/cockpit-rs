//! Le cœur du Cockpit : la base, la collecte, les sources et les lectures. L'app et la CLI n'en sont que des vues.
pub mod business;
pub mod chemins;
pub mod collecte;
pub mod db;
pub mod dimension;
pub mod erreur;
#[cfg(test)]
pub mod essai;
pub mod jour;
pub mod lecture;
pub mod metrique;
pub mod modeles;
pub mod modeles_veille;
pub mod schema;
pub mod sources;
pub mod store;
pub mod store_veille;

pub use erreur::{Erreur, Res};
