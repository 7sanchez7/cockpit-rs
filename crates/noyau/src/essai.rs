//! La graine des tests : Roleo et ses comptes vidéo (mêmes valeurs que CockpitTests/Graine+Tests.swift).
use crate::business::{Business, Graine};
use crate::store::Store;
use std::path::PathBuf;

pub const GRAINE: &str = r#"{
  "businesses": [
    {"id": "roleo", "nom": "Roleo", "lettre": "R", "teinte": "menthe", "config": {
      "appStoreId": "6770304361", "ascIssuer": "00000000-0000-0000-0000-000000000000", "playPackage": "app.roleo.roleo",
      "firebaseProject": "roleo-73820", "posthogHost": "https://eu.posthog.com",
      "contenuDir": "~/Documents/code/app/Roleo/contenu/bank", "pays": ["fr", "es", "be", "us"], "genre": 6017}}
  ],
  "comptes": [
    {"business": "roleo", "plateforme": "tiktok", "handle": "holaestony"},
    {"business": "roleo", "plateforme": "tiktok", "handle": "tonyolingo"},
    {"business": "roleo", "plateforme": "instagram", "handle": "holaestony"},
    {"business": "roleo", "plateforme": "youtube", "handle": "holaestony"},
    {"business": "roleo", "plateforme": "facebook", "handle": "holaestony"}
  ]
}"#;

pub fn graine() -> Graine { serde_json::from_str(GRAINE).unwrap() }

pub fn roleo() -> Business { graine().businesses.remove(0) }

/// Écrit `contenu` dans un fichier temporaire neuf.
pub fn fichier(contenu: &str) -> PathBuf {
    let f = std::env::temp_dir().join(format!("graine-{}.json", uuid::Uuid::new_v4()));
    std::fs::write(&f, contenu).unwrap();
    f
}

/// Une base en mémoire semée avec la graine des tests.
pub fn store() -> Store { Store::memoire_avec(&fichier(GRAINE)).unwrap() }

#[test]
fn graine_semee() {
    let s = store();
    assert_eq!(s.businesses().unwrap()[0].id, "roleo");
    assert_eq!(s.comptes().unwrap().len(), 5);
}
