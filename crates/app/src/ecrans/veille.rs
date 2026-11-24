//! Veille : le site de Viral Bank dans l'onglet, une WebView enfant de la fenêtre posée sur la place de l'écran
//! (docs/ecrans.md § Veille). La connexion et les cookies sont ceux du site ; un nouvel onglet part dans le navigateur.
use crate::commandes::{texte, Rep};
use crate::hote::AGENT;
use tauri::webview::{NewWindowResponse, WebviewBuilder};
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl};
use tauri_plugin_opener::OpenerExt;

const LIBELLE: &str = "viralbank";
const SITE: &str = "https://viralbank.duckdns.org/";

/// Pose la vue sur le rectangle de l'écran (en points de la fenêtre) et la montre ; la crée au premier appel.
#[tauri::command]
pub async fn montrer_veille(app: AppHandle, x: f64, y: f64, largeur: f64, hauteur: f64) -> Rep<()> {
    let (position, taille) = (LogicalPosition::new(x, y), LogicalSize::new(largeur, hauteur));
    if let Some(vue) = app.get_webview(LIBELLE) {
        vue.set_position(position).map_err(texte)?;
        vue.set_size(taille).map_err(texte)?;
        return vue.show().map_err(texte);
    }
    let fenetre = app.get_window("main").ok_or("fenêtre principale introuvable")?;
    let navigateur = app.clone();
    // les liens vers TikTok ou Instagram s'ouvrent en `target="_blank"` : dans le navigateur, pas dans l'onglet
    let vue = WebviewBuilder::new(LIBELLE, WebviewUrl::External(SITE.parse().map_err(texte)?))
        .user_agent(AGENT)
        .on_new_window(move |url, _| {
            if matches!(url.scheme(), "http" | "https") {
                if let Err(e) = navigateur.opener().open_url(url.as_str(), None::<&str>) {
                    eprintln!("lien : {e}");
                }
            }
            NewWindowResponse::Deny
        });
    fenetre.add_child(vue, position, taille).map_err(texte)?;
    Ok(())
}

/// Cache la vue en quittant l'écran ; elle garde sa page pour le retour.
#[tauri::command]
pub fn cacher_veille(app: AppHandle) -> Rep<()> {
    app.get_webview(LIBELLE).map_or(Ok(()), |vue| vue.hide().map_err(texte))
}
