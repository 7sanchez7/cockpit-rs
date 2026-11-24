//! Ce que la collecte demande à l'app qui la porte : les notifications et les sessions des sites.
//! L'app Tauri l'implémente ; la CLI se contente de `SansInterface`.
use crate::erreur::Res;
use std::collections::HashMap;

pub trait Hote: Send + Sync {
    /// Une notification du système. Sans interface, elle ne part pas.
    fn notifier(&self, titre: &str, corps: &str, id: &str);

    /// Les cookies de session d'un site (`tiktok.com`), par nom. Sur Mac, ceux de Safari ; l'app ajoute ceux de sa
    /// propre fenêtre de connexion (Windows). Rend `NonConfigure` sans session portant le cookie `requis`.
    fn cookies(&self, domaine: &str, requis: &str) -> Res<HashMap<String, String>> { super::safari::cookies(domaine, requis) }

    /// D'où vient la session, sans aucune valeur : pour les messages d'erreur.
    fn provenance(&self, domaine: &str, requis: &str) -> String { super::safari::provenance(domaine, requis) }
}

/// L'hôte de la CLI et des tests : pas de notification.
pub struct SansInterface;

impl Hote for SansInterface {
    fn notifier(&self, _: &str, _: &str, _: &str) {}
}
