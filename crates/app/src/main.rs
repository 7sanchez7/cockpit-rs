//! Cockpit, l'app de bureau (Windows, Mac, Linux) : la fenêtre, le tray, la collecte en fond.
//! L'interface est dans `ui/` ; elle ne parle qu'aux commandes de `commandes.rs` et de `ecrans.rs`, jamais au réseau.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commandes;
mod ecrans;
mod hote;
mod tray;

use noyau::collecte::collecteur::Collecteur;
use noyau::store::Store;
use std::sync::Arc;
use std::time::Duration;
#[cfg(target_os = "macos")]
use tauri::RunEvent;
use tauri::{Emitter, Manager, WindowEvent};

/// Ce que partagent les commandes.
pub struct Etat {
    pub store: Arc<Store>,
    pub collecteur: Arc<Collecteur>,
}

/// L'événement envoyé à l'interface quand la base change ou qu'une source démarre ou finit.
const EVENEMENT: &str = "cockpit";

fn main() {
    let store = match Store::ouvrir() {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("Cockpit ne démarre pas : {e}");
            std::process::exit(1);
        }
    };
    let pages = hote::Pages::default();
    let veille = pages.clone();
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri::plugin::Builder::<tauri::Wry, ()>::new("veille")
                .invoke_handler(tauri::generate_handler![commandes::message])
                .setup(move |app, _| {
                    app.manage(veille);
                    Ok(())
                })
                .build(),
        )
        .setup(move |app| {
            let hote = Arc::new(hote::HoteApp { app: app.handle().clone(), pages });
            let collecteur = Collecteur::new(store.clone(), noyau::sources::toutes(), hote);
            app.manage(Etat { store: store.clone(), collecteur: collecteur.clone() });
            app.manage(ecrans::barre::EtatBarre::default());
            app.asset_protocol_scope().allow_directory(noyau::chemins::cache(), true)?;
            tray::creer(app.handle())?;
            let poignee = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                collecteur.demarrer();
                surveiller(poignee, store, collecteur).await;
            });
            Ok(())
        })
        .on_window_event(|fenetre, e| {
            // fermer la fenêtre garde l'app dans le tray, comme la barre de menus du Mac
            if let WindowEvent::CloseRequested { api, .. } = e {
                if fenetre.label() == "main" {
                    api.prevent_close();
                    if let Err(e) = fenetre.hide() {
                        eprintln!("fenêtre : {e}");
                    }
                }
            }
        })
        .invoke_handler(ecrans::commandes())
        .build(tauri::generate_context!())
        .unwrap_or_else(|e| {
            eprintln!("Cockpit ne démarre pas : {e}");
            std::process::exit(1);
        })
        .run(|_app, _e| {
            // le clic sur l'icône du Dock (Mac) ; ailleurs, c'est le tray qui remontre la fenêtre
            #[cfg(target_os = "macos")]
            if let RunEvent::Reopen { .. } = _e {
                tray::montrer(_app);
            }
        });
}

/// Prévient l'interface à chaque écriture de la base et à chaque source qui démarre ou finit ; met le tray à jour.
async fn surveiller(app: tauri::AppHandle, store: Arc<Store>, collecteur: Arc<Collecteur>) {
    let mut vu = (u64::MAX, Vec::new());
    loop {
        let mut en_cours: Vec<String> = collecteur.en_cours().into_iter().collect();
        en_cours.sort();
        let etat = (store.version(), en_cours);
        if etat != vu {
            if let Err(e) = app.emit(EVENEMENT, serde_json::json!({ "version": etat.0, "enCours": etat.1 })) {
                eprintln!("événement : {e}");
            }
            if etat.0 != vu.0 {
                tray::actualiser(&app);
            }
            vu = etat;
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
}
