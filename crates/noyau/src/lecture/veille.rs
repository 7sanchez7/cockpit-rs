//! La veille des business affichés (docs/sources/veille.md) : l'écran Veille et la CLI.
use super::Lectures;
use crate::erreur::Res;
use crate::modeles_veille::{MotVeille, VideoVeille};
use crate::p;
use chrono::Duration;
use serde::Serialize;
use std::collections::HashMap;

/// Le plafond des analyses Claude de la veille, en dollars.
pub const PLAFOND_VEILLE_USD: f64 = 50.0;

/// Un type de hook et le nombre de vidéos analysées qui l'ont.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeHook {
    #[serde(rename = "type")]
    pub type_: String,
    pub videos: i64,
}

impl Lectures<'_> {
    pub fn mots_veille(&self) -> Res<Vec<MotVeille>> {
        let sql = format!("select * from veille_mot where business in {} order by ajoute", self.dans());
        Ok(self.q(&sql, self.ids())?.iter().filter_map(|l| Some(MotVeille {
            business: l.texte("business")?,
            mot: l.texte("mot")?,
            vues_min: l.entier("vues_min").unwrap_or(20_000),
            actif: l.booleen("actif"),
            cherche: l.date("cherche"),
        })).collect())
    }

    /// Les vidéos de la veille des `jours` derniers jours, les plus récemment trouvées d'abord.
    pub fn videos_veille(&self, jours: i64) -> Res<Vec<VideoVeille>> {
        let sql = format!("{} where v.business in {} and v.trouvee >= ? order by v.trouvee desc", VideoVeille::SQL, self.dans());
        let depuis = self.maintenant - Duration::seconds(jours * 86_400);
        Ok(self.q(&sql, self.autour(p![], p![depuis]))?.iter().filter_map(VideoVeille::depuis_ligne).collect())
    }

    /// Les types de hook des vidéos analysées sur 30 jours, du plus fréquent au moins fréquent.
    pub fn hooks_veille(&self) -> Res<Vec<TypeHook>> {
        let mut n: HashMap<String, i64> = HashMap::new();
        for v in self.videos_veille(30)? {
            if let Some(a) = v.analyse { *n.entry(a.hook_type).or_default() += 1; }
        }
        let mut r: Vec<TypeHook> = n.into_iter().map(|(type_, videos)| TypeHook { type_, videos }).collect();
        r.sort_by(|a, b| b.videos.cmp(&a.videos).then_with(|| a.type_.cmp(&b.type_)));
        Ok(r)
    }
}
