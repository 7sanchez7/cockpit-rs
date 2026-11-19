//! L'icône de la barre des tâches (ou de la barre de menus) : le nombre de choses à traiter, les chiffres du jour en
//! info-bulle, un menu (clic droit) pour ouvrir, relancer, mettre en pause, quitter, et la fenêtre de la barre au clic
//! gauche (`ecrans::barre`). Elle remplace le widget du bureau du Mac.
use crate::Etat;
use noyau::collecte::collecteur::REGLAGE_PAUSE;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

const ID: &str = "cockpit";

/// La case « Collecte en pause », gardée pour la cocher d'après la base.
struct CasePause(CheckMenuItem<Wry>);

pub fn creer(app: &AppHandle) -> tauri::Result<()> {
    let pause = CheckMenuItem::with_id(app, "pause", "Collecte en pause", true, false, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "ouvrir", "Ouvrir Cockpit", true, None::<&str>)?,
            &MenuItem::with_id(app, "sav", "SAV", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "relancer", "Tout relancer", true, None::<&str>)?,
            &pause,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quitter", "Quitter", true, None::<&str>)?,
        ],
    )?;
    TrayIconBuilder::with_id(ID)
        .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
        .icon_as_template(true)
        .tooltip("Cockpit")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "ouvrir" => montrer(app),
            "sav" => {
                montrer(app);
                if let Err(e) = app.emit("ecran", "aTraiter") {
                    eprintln!("tray : {e}");
                }
            }
            "relancer" => {
                let c = app.state::<Etat>().collecteur.clone();
                tauri::async_runtime::spawn(async move { c.relancer(None) });
            }
            "pause" => {
                let etat = app.state::<Etat>();
                let oui = etat.store.reglage(REGLAGE_PAUSE).ok().flatten().as_deref() != Some("1");
                match etat.store.regler(REGLAGE_PAUSE, oui.then_some("1")) {
                    Ok(()) => etat.store.a_change(),
                    Err(e) => eprintln!("pause : {e}"),
                }
            }
            "quitter" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|icone, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } = e {
                crate::ecrans::barre::basculer(icone.app_handle(), rect);
            }
        })
        .build(app)?;
    app.manage(CasePause(pause));
    actualiser(app);
    Ok(())
}

/// Montre la fenêtre principale, cachée par sa fermeture.
pub fn montrer(app: &AppHandle) {
    if let Some(f) = app.get_webview_window("main") {
        if let Err(e) = f.show().and_then(|_| f.unminimize()).and_then(|_| f.set_focus()) {
            eprintln!("fenêtre : {e}");
        }
    }
}

/// Le titre (Mac), l'info-bulle et la case de pause d'après la base.
pub fn actualiser(app: &AppHandle) {
    let Some(icone) = app.tray_by_id(ID) else { return };
    let etat = app.state::<Etat>();
    let resume = crate::ecrans::resume_tray(&etat.store);
    let titre = (resume.a_traiter > 0).then(|| resume.a_traiter.to_string());
    let pause = etat.store.reglage(REGLAGE_PAUSE).ok().flatten().as_deref() == Some("1");
    let resultat = icone
        .set_title(titre.as_deref())
        .and_then(|_| icone.set_tooltip(Some(resume.bulle.as_str())))
        .and_then(|_| app.state::<CasePause>().0.set_checked(pause));
    if let Err(e) = resultat {
        eprintln!("tray : {e}");
    }
}
