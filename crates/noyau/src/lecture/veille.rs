//! La veille des business affichés (docs/sources/veille.md) : l'écran Veille et la CLI.
use super::Lectures;
use crate::db::Valeur;
use crate::erreur::Res;
use crate::jour::Date;
use crate::modeles_veille::{DemandeVeille, MotVeille, VideoVeille};
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

/// Une conversation avec Claude : ses demandes partagent le même `fil`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FilVeille {
    pub id: String,
    pub business: String,
    /// Sa première demande.
    pub titre: String,
    pub debut: Date,
    /// Le début de sa dernière demande.
    pub dernier: Date,
    /// L'état de sa dernière demande.
    pub etat: String,
    pub demandes: i64,
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

    /// Les vidéos de la veille des `jours` derniers jours, les plus récemment trouvées d'abord, avec toutes celles qu'une
    /// demande à Claude a gardées : elles restent dans la bibliothèque.
    pub fn videos_veille(&self, jours: i64) -> Res<Vec<VideoVeille>> {
        let sql = format!("{} where v.business in {} and (v.trouvee >= ? or retenue) order by v.trouvee desc", VideoVeille::SQL, self.dans());
        let depuis = self.maintenant - Duration::seconds(jours * 86_400);
        Ok(self.q(&sql, self.autour(p![], p![depuis]))?.iter().filter_map(VideoVeille::depuis_ligne).collect())
    }

    /// Toute la bank des business affichés : chaque vidéo trouvée, les plus récemment trouvées d'abord.
    pub fn bank_veille(&self) -> Res<Vec<VideoVeille>> {
        let sql = format!("{} where v.business in {} order by v.trouvee desc", VideoVeille::SQL, self.dans());
        Ok(self.q(&sql, self.ids())?.iter().filter_map(VideoVeille::depuis_ligne).collect())
    }

    /// Les demandes à Claude des business affichés, de la première à la dernière.
    fn demandes_veille(&self, suite: &str, params: Vec<Valeur>) -> Res<Vec<DemandeVeille>> {
        let sql = format!("select * from veille_demande where business in {} {suite} order by debut, rowid", self.dans());
        Ok(self.q(&sql, self.autour(vec![], params))?.iter().filter_map(DemandeVeille::depuis_ligne).collect())
    }

    /// La dernière demande à Claude des business affichés.
    pub fn demande_veille(&self) -> Res<Option<DemandeVeille>> { Ok(self.demandes_veille("", vec![])?.pop()) }

    /// Les conversations avec Claude, la plus récemment relancée d'abord.
    pub fn fils_veille(&self) -> Res<Vec<FilVeille>> {
        // de la dernière demande à la première : un fil apparaît à sa dernière demande et finit sur sa première
        let mut fils: Vec<FilVeille> = vec![];
        for d in self.demandes_veille("", vec![])?.into_iter().rev() {
            match fils.iter_mut().find(|f| f.id == d.fil) {
                Some(f) => {
                    (f.titre, f.debut) = (d.prompt, d.debut);
                    f.demandes += 1;
                }
                None => fils.push(FilVeille { id: d.fil, business: d.business, titre: d.prompt, debut: d.debut, dernier: d.debut, etat: d.etat, demandes: 1 }),
            }
        }
        Ok(fils)
    }

    /// Les demandes d'une conversation, dans l'ordre.
    pub fn fil_veille(&self, fil: &str) -> Res<Vec<DemandeVeille>> {
        self.demandes_veille("and coalesce(fil, id) = ?", p![fil])
    }

    /// Les vidéos qu'une demande a gardées, dans l'ordre, avec la raison donnée par Claude.
    pub fn retenues_veille(&self, demande: &str) -> Res<Vec<(VideoVeille, String)>> {
        let sql = format!("select x.*, t.raison from ({}) x join veille_retenue t on t.video = x.id where t.demande = ? order by t.ordre",
            VideoVeille::SQL);
        Ok(self.q(&sql, p![demande])?.iter().filter_map(|l| Some((VideoVeille::depuis_ligne(l)?, l.texte("raison")?))).collect())
    }

    /// Ce qu'a coûté une demande, en dollars : ses appels et les analyses faites pendant qu'elle tournait (la veille
    /// régulière attend qu'elle finisse).
    pub fn cout_demande(&self, d: &DemandeVeille) -> Res<f64> {
        let fin = d.fin.unwrap_or(self.maintenant);
        Ok(self.q("select coalesce(sum(usd), 0) as t from veille_cout where at >= ? and at <= ?", p![d.debut, fin])?
            .first().and_then(|l| l.nombre("t")).unwrap_or(0.0))
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
