//! Les commandes des écrans : chacune rend la lecture toute faite d'un écran, que l'interface ne fait qu'afficher.
//! Un fichier par écran ; chaque commande prend `business: Option<String>` (None = tous) quand l'écran en dépend.
pub mod accueil;
pub mod apps;
pub mod barre;
pub mod data;
pub mod palette;
pub mod reglages;
pub mod sante;
pub mod sav;
pub mod veille;
pub mod videos;

use crate::commandes::{self, texte, Rep};
use noyau::lecture::Lectures;
use noyau::metrique::Boutique;
use noyau::store::Store;

/// Les lectures des business affichés (tous, ou celui choisi), sur le store choisi (réglage `boutique`).
pub fn lectures<'a>(store: &'a Store, business: Option<&str>) -> Rep<Lectures<'a>> {
    let tous = store.businesses().map_err(texte)?;
    let affiches = match business.and_then(|id| tous.iter().find(|b| b.id == id)) {
        Some(b) => vec![b.clone()],
        None => tous,
    };
    let mut l = Lectures::new(&store.db, affiches);
    l.boutique = store.reglage("boutique").map_err(texte)?.as_deref().and_then(Boutique::depuis);
    Ok(l)
}

/// Ce que montre l'icône du tray : le nombre de choses à traiter et les chiffres du jour.
pub struct ResumeTray {
    pub a_traiter: usize,
    pub bulle: String,
}

pub fn resume_tray(store: &Store) -> ResumeTray {
    let resume = lectures(store, None).and_then(|l| l.resume_widget().map_err(texte));
    match resume {
        Ok(r) => ResumeTray {
            a_traiter: r.a_traiter.max(0) as usize,
            bulle: format!("Cockpit, aujourd'hui {} ({}), {}", r.revenu, r.hier, r.detail),
        },
        Err(e) => ResumeTray { a_traiter: 0, bulle: format!("Cockpit : {e}") },
    }
}

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
        reglages::reglages_business,
        reglages::enregistrer_business,
        reglages::supprimer_business,
        reglages::reglages_cles,
        reglages::ranger_cle,
        reglages::effacer_cle,
        reglages::reglages_notifs,
        reglages::reglages_collecte,
        reglages::montrer_base,
        reglages::reglages_sessions,
        barre::barre,
        barre::barre_ouvrir,
        barre::barre_cacher,
        barre::barre_taille,
        barre::quitter,
        palette::chercher,
    ]
}
