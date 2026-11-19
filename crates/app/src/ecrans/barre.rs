//! La fenêtre de la barre de menus (Mac) ou de la barre des tâches (Windows), ouverte par un clic gauche sur l'icône :
//! les chiffres du jour, ce qu'il reste à traiter, les 5 derniers événements. Portée de BarreMenus.swift.
use crate::commandes::{texte, Rep};
use crate::Etat;
use noyau::jour;
use noyau::lecture::format;
use noyau::metrique;
use noyau::modeles::Evenement;
use parking_lot::Mutex;
use serde::Serialize;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Rect, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};

const LIBELLE: &str = "barre";
/// La largeur de la fenêtre (Theme.Taille.barreMenus) et sa hauteur avant que la page ne se mesure.
const LARGEUR: f64 = 340.0;
const HAUTEUR: f64 = 420.0;
/// L'écart entre l'icône et la fenêtre.
const ECART: f64 = 6.0;
/// Un clic sur l'icône juste après que la fenêtre a perdu le focus la referme au lieu de la rouvrir.
const REBOND: Duration = Duration::from_millis(300);

/// Où est l'icône (pour placer la fenêtre) et quand la fenêtre a été cachée par la perte du focus.
#[derive(Default)]
struct Ancrage {
    icone: Option<Rect>,
    cachee: Option<Instant>,
}

#[derive(Default)]
pub struct EtatBarre(Mutex<Ancrage>);

/// Le clic gauche sur l'icône : montre la fenêtre sous l'icône (au-dessus sur une barre des tâches en bas), ou la cache.
pub fn basculer(app: &AppHandle, icone: Rect) {
    let etat = app.state::<EtatBarre>();
    let rebond = {
        let mut a = etat.0.lock();
        a.icone = Some(icone);
        a.cachee.take().is_some_and(|t| t.elapsed() < REBOND)
    };
    let resultat = match app.get_webview_window(LIBELLE) {
        Some(f) if f.is_visible().unwrap_or(false) => f.hide(),
        Some(_) if rebond => Ok(()),
        Some(f) => placer(app).and_then(|_| f.show()).and_then(|_| f.set_focus()),
        None => creer(app).and_then(|_| placer(app)),
    };
    if let Err(e) = resultat {
        eprintln!("barre : {e}");
    }
}

fn creer(app: &AppHandle) -> tauri::Result<()> {
    let f = WebviewWindowBuilder::new(app, LIBELLE, WebviewUrl::App("barre.html".into()))
        .title("Cockpit")
        .inner_size(LARGEUR, HAUTEUR)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(true)
        .background_color(tauri::window::Color(5, 5, 6, 255))
        .build()?;
    let poignee = app.clone();
    f.on_window_event(move |e| {
        if let WindowEvent::Focused(false) = e {
            poignee.state::<EtatBarre>().0.lock().cachee = Some(Instant::now());
            if let Some(f) = poignee.get_webview_window(LIBELLE) {
                if let Err(e) = f.hide() {
                    eprintln!("barre : {e}");
                }
            }
        }
    });
    f.set_focus()
}

/// Centre la fenêtre sur l'icône, collée au bord de l'écran où est la barre, sans déborder de l'écran.
fn placer(app: &AppHandle) -> tauri::Result<()> {
    let Some(f) = app.get_webview_window(LIBELLE) else { return Ok(()) };
    let Some(icone) = app.state::<EtatBarre>().0.lock().icone else { return Ok(()) };
    let echelle = f.scale_factor()?;
    let pos = icone.position.to_physical::<f64>(echelle);
    let taille = icone.size.to_physical::<f64>(echelle);
    // monitor_from_point compte en points sur Mac et en pixels sur Windows : on cherche l'écran à la main, en pixels
    let contient = |m: &tauri::Monitor| {
        let (p, t) = (m.position(), m.size());
        (p.x as f64..p.x as f64 + t.width as f64).contains(&pos.x) && (p.y as f64..p.y as f64 + t.height as f64).contains(&pos.y)
    };
    let Some(ecran) = app.available_monitors()?.into_iter().find(contient) else { return Ok(()) };
    let zone = ecran.work_area();
    let (zx, zy) = (zone.position.x as f64, zone.position.y as f64);
    let (zl, zh) = (zone.size.width as f64, zone.size.height as f64);
    let fenetre = f.outer_size()?;
    let (l, h) = (fenetre.width as f64, fenetre.height as f64);
    let ecart = ECART * echelle;
    let x = (pos.x + taille.width / 2.0 - l / 2.0).clamp(zx + ecart, (zx + zl - l - ecart).max(zx));
    let en_haut = pos.y + taille.height / 2.0 < zy + zh / 2.0;
    let y = if en_haut { (pos.y + taille.height).max(zy) + ecart } else { pos.y.min(zy + zh) - h - ecart };
    f.set_position(PhysicalPosition::new(x, y))
}

#[derive(Serialize)]
pub struct Chiffre {
    libelle: &'static str,
    valeur: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Barre {
    /// « collecte il y a 3 min » ou « aucune collecte ».
    collecte: String,
    chiffres: Vec<Chiffre>,
    a_traiter: i64,
    fil: Vec<Evenement>,
}

/// La lecture de la fenêtre, pour le business choisi dans la fenêtre principale.
#[tauri::command]
pub async fn barre(etat: State<'_, Etat>) -> Rep<Barre> {
    let business = etat.store.reglage("business").map_err(texte)?;
    let l = super::lectures(&etat.store, business.as_deref())?;
    let jour = jour::cle(&l.maintenant);
    let valeur = |m: &str, f: fn(f64) -> String| -> Rep<String> {
        Ok(l.valeur(m, &jour, false).map_err(texte)?.map_or_else(|| format::VIDE.to_string(), f))
    };
    let chiffres = vec![
        Chiffre { libelle: "MRR", valeur: valeur(metrique::MRR_BRUT, |v| format::euros(metrique::en_euros(v)))? },
        Chiffre { libelle: "Actifs", valeur: valeur(metrique::ACTIFS, format::entier)? },
        Chiffre { libelle: "Installs", valeur: valeur(metrique::INSTALLS, format::entier)? },
        Chiffre { libelle: "Essais", valeur: valeur(metrique::ESSAIS, format::entier)? },
    ];
    let derniere = l.derniers_passages().map_err(texte)?.into_values().map(|p| p.fin).max();
    Ok(Barre {
        collecte: derniere.map_or_else(|| "aucune collecte".into(), |d| format!("collecte {}", format::il_ya(&d, &l.maintenant))),
        chiffres,
        a_traiter: l.nb_a_traiter().map_err(texte)?,
        fil: l.fil(None, None, 5).map_err(texte)?,
    })
}

/// Ramène la fenêtre principale devant, sur `ecran` s'il est donné, et cache la barre.
#[tauri::command]
pub async fn barre_ouvrir(app: AppHandle, ecran: Option<String>) -> Rep<()> {
    crate::tray::montrer(&app);
    if let Some(ecran) = ecran {
        app.emit("ecran", ecran).map_err(texte)?;
    }
    barre_cacher(app).await
}

#[tauri::command]
pub async fn barre_cacher(app: AppHandle) -> Rep<()> {
    match app.get_webview_window(LIBELLE) {
        Some(f) => f.hide().map_err(texte),
        None => Ok(()),
    }
}

/// La page s'est mesurée : la fenêtre prend sa hauteur et se recolle à l'icône.
#[tauri::command]
pub async fn barre_taille(app: AppHandle, hauteur: f64) -> Rep<()> {
    let Some(f) = app.get_webview_window(LIBELLE) else { return Ok(()) };
    f.set_size(LogicalSize::new(LARGEUR, hauteur.ceil())).map_err(texte)?;
    placer(&app).map_err(texte)
}

#[tauri::command]
pub fn quitter(app: AppHandle) { app.exit(0) }
