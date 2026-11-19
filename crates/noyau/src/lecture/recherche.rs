//! La recherche de la palette : les vidéos, les apps suivies et les tâches ouvertes dont le titre ou le détail
//! contient le texte tapé, sans tenir compte des accents ni de la casse.
use super::{format, Lectures};
use crate::erreur::Res;
use serde::Serialize;

/// Au plus autant de résultats, comme la palette.
pub const MAX_TROUVES: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Genre { Video, App, Tache }

#[derive(Debug, Clone, Serialize)]
pub struct Trouve {
    pub genre: Genre,
    pub id: String,
    pub titre: String,
    pub detail: String,
}

/// En minuscules et sans accents : `Éducation` devient `education`.
pub fn plier(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ñ' => 'n',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ý' | 'ÿ' => 'y',
            c => c,
        })
        .collect()
}

impl Lectures<'_> {
    /// Vidéos, apps puis tâches qui contiennent `texte` ; rien pour un texte vide.
    pub fn recherche(&self, texte: &str) -> Res<Vec<Trouve>> {
        let q = plier(texte.trim());
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let videos = self.videos(1000)?.into_iter().map(|v| Trouve {
            genre: Genre::Video,
            titre: if v.titre.is_empty() { v.id.clone() } else { v.titre },
            detail: format!("@{}, {} vues", v.compte, format::entier(v.vues as f64)),
            id: v.id,
        });
        let apps = self.apps()?.into_iter().map(|a| Trouve {
            genre: Genre::App,
            detail: a.app.editeur.unwrap_or_else(|| "App Store".into()),
            titre: a.app.nom,
            id: a.app.id,
        });
        let taches = self.taches()?.into_iter().map(|t| Trouve { genre: Genre::Tache, id: t.id, titre: t.texte, detail: "SAV".into() });
        Ok(videos
            .chain(apps)
            .chain(taches)
            .filter(|t| plier(&t.titre).contains(&q) || plier(&t.detail).contains(&q))
            .take(MAX_TROUVES)
            .collect())
    }
}
