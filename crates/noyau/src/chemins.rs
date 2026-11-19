//! Où le cockpit range ses fichiers. Sur Mac, les mêmes dossiers que la version SwiftUI (la base existante est reprise
//! telle quelle) ; sous Windows, `%APPDATA%\Cockpit` et `%LOCALAPPDATA%\Cockpit`. `COCKPIT_DIR` remplace le dossier des
//! données, pour essayer l'app sur une copie de la base.
use std::path::PathBuf;

/// `~/Library/Application Support/Cockpit`, `%APPDATA%\Cockpit`.
pub fn donnees() -> PathBuf {
    if let Some(d) = std::env::var_os("COCKPIT_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(d);
    }
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("Cockpit")
}

/// Les programmes que le cockpit télécharge lui-même (yt-dlp nightly) ; cherchés avant le PATH.
pub fn outils() -> PathBuf { donnees().join("bin") }

pub fn base() -> PathBuf { donnees().join("cockpit.sqlite") }

/// Les businesses et comptes de cette machine, hors du dépôt (outils/graine.exemple.json).
pub fn graine() -> PathBuf { donnees().join("graine.json") }

/// `~/Library/Caches/Cockpit`, `%LOCALAPPDATA%\Cockpit`.
pub fn cache() -> PathBuf {
    if std::env::var_os("COCKPIT_DIR").is_some_and(|d| !d.is_empty()) {
        return donnees().join("cache");
    }
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("Cockpit")
}

/// Les identifiants par défaut de gcloud (`gcloud auth application-default login`).
pub fn adc() -> PathBuf {
    if let Some(f) = std::env::var_os("GOOGLE_APPLICATION_CREDENTIALS").filter(|f| !f.is_empty()) {
        return PathBuf::from(f);
    }
    if cfg!(windows) {
        dirs::config_dir().unwrap_or_default().join("gcloud").join("application_default_credentials.json")
    } else {
        dirs::home_dir().unwrap_or_default().join(".config/gcloud/application_default_credentials.json")
    }
}
