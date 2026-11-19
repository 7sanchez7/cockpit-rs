//! Les résultats de la palette de commandes qui viennent de la base : vidéos, apps suivies, tâches ouvertes.
use crate::commandes::{texte, Rep};
use crate::Etat;
use noyau::lecture::recherche::Trouve;
use tauri::State;

#[tauri::command]
pub async fn chercher(etat: State<'_, Etat>, business: Option<String>, requete: String) -> Rep<Vec<Trouve>> {
    super::lectures(&etat.store, business.as_deref())?.recherche(&requete).map_err(texte)
}
