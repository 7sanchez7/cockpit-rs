//! La collecte : chaque source interrogée à sa cadence, ses passages journalisés (docs/collecte.md).
pub mod collecteur;
pub mod commande;
pub mod gcloud;
pub mod hote;
pub mod http;
pub mod jwt;
pub mod miniatures;
pub mod safari;
pub mod secrets;
pub mod source;
pub mod tikhub;
pub mod ytdlp;

pub use collecteur::Collecteur;
pub use hote::{Hote, Navigateur, SansInterface};
pub use http::{Http, Requete};
pub use source::{Contexte, Source};
