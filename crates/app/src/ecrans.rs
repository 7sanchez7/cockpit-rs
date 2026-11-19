//! Les commandes des écrans : chacune rend la lecture toute faite d'un écran, que l'interface ne fait qu'afficher.
use crate::commandes;
use noyau::store::Store;

/// Ce que montre l'icône du tray : le nombre de choses à traiter et les chiffres du jour.
pub struct ResumeTray {
    pub a_traiter: usize,
    pub bulle: String,
}

pub fn resume_tray(_store: &Store) -> ResumeTray { ResumeTray { a_traiter: 0, bulle: "Cockpit".into() } }

pub fn commandes() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        commandes::etat,
        commandes::reglage,
        commandes::regler,
        commandes::ecrire_reglage,
        commandes::pause,
        commandes::relancer,
        commandes::session,
        commandes::lien,
    ]
}
