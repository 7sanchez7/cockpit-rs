//! L'hôte de la collecte dans l'app : notifications du système et sessions des sites.
//! Les sessions viennent de Safari sur Mac, sinon de la fenêtre « Sessions » de l'app, dont les WebViews partagent
//! le même magasin de cookies. Aucune valeur de cookie n'est affichée, écrite ou mise dans une erreur.
use noyau::collecte::hote::Hote;
use noyau::collecte::http::{UA_NAVIGATEUR, UA_SAFARI};
use noyau::collecte::safari;
use noyau::{Erreur, Res};
use std::collections::HashMap;
use tauri::{AppHandle, Manager, Webview};
use tauri_plugin_notification::NotificationExt;

/// L'agent du moteur de la WebView : WebKit sur Mac, Chromium (WebView2) ailleurs.
pub const AGENT: &str = if cfg!(target_os = "macos") { UA_SAFARI } else { UA_NAVIGATEUR };

pub struct HoteApp {
    pub app: AppHandle,
}

impl HoteApp {
    /// Une WebView de l'app, qui donne accès au magasin de cookies commun.
    fn vue(&self) -> Option<Webview> { self.app.webviews().into_values().next() }

    fn cookies_app(&self, domaine: &str, requis: &str) -> Res<HashMap<String, String>> {
        let vue = self.vue().ok_or_else(|| Erreur::NonConfigure("aucune fenêtre ouverte pour lire les sessions".into()))?;
        let url = format!("https://www.{domaine}/").parse().map_err(|e| Erreur::Commande(format!("{domaine} : {e}")))?;
        let cookies = vue.cookies_for_url(url).map_err(|e| Erreur::Commande(format!("cookies de {domaine} : {e}")))?;
        let jar: HashMap<String, String> = cookies.iter().map(|c| (c.name().to_string(), c.value().to_string())).collect();
        if jar.get(requis).is_none_or(|v| v.is_empty()) {
            return Err(Erreur::NonConfigure(format!("pas de session : se connecter à {domaine} dans Réglages, Sessions")));
        }
        Ok(jar)
    }
}

impl Hote for HoteApp {
    fn notifier(&self, titre: &str, corps: &str, _id: &str) {
        if let Err(e) = self.app.notification().builder().title(titre).body(corps).show() {
            eprintln!("notification : {e}");
        }
    }

    /// Safari d'abord sur Mac (la session de tous les jours), puis la fenêtre Sessions de l'app.
    fn cookies(&self, domaine: &str, requis: &str) -> Res<HashMap<String, String>> {
        match safari::cookies(domaine, requis) {
            Ok(c) => Ok(c),
            Err(safari) => self.cookies_app(domaine, requis).map_err(|app| if cfg!(target_os = "macos") { safari } else { app }),
        }
    }

    fn provenance(&self, domaine: &str, requis: &str) -> String {
        let app = match self.cookies_app(domaine, requis) {
            Ok(c) => format!("app : {} cookies", c.len()),
            Err(_) => "app : pas de session".into(),
        };
        format!("{} ; {app}", safari::provenance(domaine, requis))
    }
}
