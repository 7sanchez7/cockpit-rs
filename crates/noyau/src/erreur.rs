use std::fmt;

/// L'erreur de tout le noyau. Une source la rend, le collecteur l'écrit dans `collecte`, Santé l'affiche.
#[derive(Debug, Clone, PartialEq)]
pub enum Erreur {
    /// Il manque une clé, un identifiant ou un accès : la source attend qu'on la règle.
    NonConfigure(String),
    Http(u16, String),
    Format(String),
    Commande(String),
    Sql(String),
}

impl fmt::Display for Erreur {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Erreur::NonConfigure(m) => write!(f, "Non configuré : {m}"),
            Erreur::Http(c, x) => write!(f, "HTTP {c} : {x}"),
            Erreur::Format(m) => write!(f, "Réponse inattendue : {m}"),
            Erreur::Commande(m) => write!(f, "{m}"),
            Erreur::Sql(m) => write!(f, "SQLite : {m}"),
        }
    }
}

impl std::error::Error for Erreur {}

impl From<rusqlite::Error> for Erreur {
    fn from(e: rusqlite::Error) -> Self { Erreur::Sql(e.to_string()) }
}

impl From<serde_json::Error> for Erreur {
    fn from(e: serde_json::Error) -> Self { Erreur::Format(e.to_string()) }
}

impl From<reqwest::Error> for Erreur {
    fn from(e: reqwest::Error) -> Self {
        match e.status() {
            Some(s) => Erreur::Http(s.as_u16(), e.to_string()),
            None => Erreur::Commande(format!("réseau : {e}")),
        }
    }
}

impl From<std::io::Error> for Erreur {
    fn from(e: std::io::Error) -> Self { Erreur::Commande(e.to_string()) }
}

pub type Res<T> = Result<T, Erreur>;

/// Raccourci : `Err(format_err("…"))`.
pub fn format_err(m: impl Into<String>) -> Erreur { Erreur::Format(m.into()) }
