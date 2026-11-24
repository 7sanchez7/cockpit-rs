use super::gcloud::GCloud;
use super::hote::Hote;
use super::http::Http;
use super::secrets::Secrets;
use crate::business::Business;
use crate::erreur::Res;
use crate::store::Store;
use async_trait::async_trait;
use std::sync::Arc;

/// Un service interrogé à sa cadence. Une source lit, n'écrit que par `Store`, ne modifie rien chez le service ;
/// seule exception, les actions de l'écran SAV (`sources::firebase::sav`), qui écrivent dans Firestore et envoient la
/// notif. Le contrat complet est dans docs/collecte.md, chaque source a sa fiche dans docs/sources/.
#[async_trait]
pub trait Source: Send + Sync {
    /// Clé de la source dans la table `collecte` et préfixe de ses lignes du fil (`posthog`).
    fn id(&self) -> &'static str;
    /// Nom affiché (`PostHog`).
    fn nom(&self) -> &'static str;
    /// Intervalle entre deux passages réussis, en secondes ; `None` : la source ne passe que sur demande.
    fn cadence(&self) -> Option<i64>;
    /// Délai avant de retenter après un échec.
    fn reprise(&self) -> i64 { super::collecteur::REPRISE_APRES_ECHEC }
    /// Lit la source pour chaque business qui l'utilise et rend le nombre de lignes écrites.
    /// Rend `Erreur::NonConfigure` si aucun business n'est configuré pour elle.
    async fn collecte(&self, ctx: &Contexte) -> Res<usize>;
}

/// Ce qu'une source reçoit pour travailler.
#[derive(Clone)]
pub struct Contexte {
    pub store: Arc<Store>,
    pub secrets: Arc<Secrets>,
    pub http: Http,
    pub gcloud: Arc<GCloud>,
    pub businesses: Vec<Business>,
    pub hote: Arc<dyn Hote>,
}
