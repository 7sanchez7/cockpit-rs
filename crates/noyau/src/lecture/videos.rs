//! Les lectures de l'écran Vidéos : comptes suivis, abonnés et rythme de publication.
use super::Lectures;
use crate::collecte::miniatures;
use crate::erreur::Res;
use crate::jour::{self, Date};
use crate::modeles::Plateforme;
use crate::p;
use chrono::Duration;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LigneCompte {
    pub id: String,
    pub plateforme: Plateforme,
    pub handle: String,
    pub business: String,
    pub actif: bool,
    pub abonnes: Option<i64>,
    /// Abonnés gagnés depuis le relevé d'il y a 7 jours ; None si le compte est suivi depuis moins longtemps.
    pub gain7j: Option<i64>,
    /// Le dernier nombre d'abonnés de chacun des 14 derniers jours.
    pub serie: Vec<Option<f64>>,
    pub videos: i64,
    pub releve_le: Option<Date>,
}

impl Lectures<'_> {
    /// Les comptes suivis des business affichés : dernier nombre d'abonnés, écart sur 7 jours, courbe de 14 jours.
    pub fn comptes(&self) -> Res<Vec<LigneCompte>> {
        let jours = jour::derniers(14, &jour::cle(&self.maintenant));
        let il_ya_7j = self.maintenant - Duration::seconds(7 * 86_400);
        let mut par_compte: HashMap<String, Vec<(Date, i64)>> = HashMap::new();
        for l in self.q("select compte, at, abonnes from compte_releve where abonnes is not null and at >= ? order by at",
                        p![self.maintenant - Duration::seconds(15 * 86_400)])? {
            if let (Some(c), Some(at), Some(n)) = (l.texte("compte"), l.date("at"), l.entier("abonnes")) {
                par_compte.entry(c).or_default().push((at, n));
            }
        }
        let sql = format!("
            select c.id, c.plateforme, c.handle, c.business, c.actif,
                (select count(*) from video v where v.plateforme = c.plateforme and v.compte = c.handle) as videos,
                (select r.abonnes from compte_releve r where r.compte = c.id and r.abonnes is not null
                    order by r.at desc limit 1) as abonnes,
                (select max(r.at) from compte_releve r where r.compte = c.id) as releve_le
            from compte c where c.business in {} order by c.business, c.plateforme, c.ajoute_le", self.dans());
        Ok(self.q(&sql, self.ids())?.iter().filter_map(|l| {
            let id = l.texte("id")?;
            let plateforme = Plateforme::depuis(&l.texte("plateforme").unwrap_or_default())?;
            let releves = par_compte.get(&id).map(Vec::as_slice).unwrap_or_default();
            let par_jour: HashMap<String, f64> = releves.iter().map(|(at, n)| (jour::cle(at), *n as f64)).collect();
            let abonnes = l.entier("abonnes");
            let avant = releves.iter().rev().find(|(at, _)| *at <= il_ya_7j).map(|r| r.1);
            Some(LigneCompte {
                plateforme,
                handle: l.texte("handle").unwrap_or_default(),
                business: l.texte("business").unwrap_or_default(),
                actif: l.booleen("actif"),
                abonnes,
                gain7j: abonnes.and_then(|n| avant.map(|a| n - a)),
                serie: jours.iter().map(|j| par_jour.get(j).copied()).collect(),
                videos: l.entier("videos").unwrap_or(0),
                releve_le: l.date("releve_le"),
                id,
            })
        }).collect())
    }

    /// Les vidéos publiées chaque jour de Paris sur les `n` jours jusqu'à `jusqua`, du plus ancien au plus récent.
    pub fn publications(&self, n: i64, jusqua: &str) -> Res<Vec<f64>> {
        let cles = jour::derniers(n, jusqua);
        let (Some(debut), Some(fin)) = (cles.first().and_then(|c| jour::date(c)), jour::date(&jour::decale(jusqua, 1))) else {
            return Ok(vec![]);
        };
        let sql = format!("select publiee_le from video where publiee_le >= ? and publiee_le < ? and business in {}", self.dans());
        let mut par_jour: HashMap<String, f64> = HashMap::new();
        for l in self.q(&sql, self.autour(p![debut, fin], p![]))? {
            if let Some(d) = l.date("publiee_le") { *par_jour.entry(jour::cle(&d)).or_default() += 1.0; }
        }
        Ok(cles.iter().map(|c| par_jour.get(c).copied().unwrap_or(0.0)).collect())
    }

    /// Les clés des images à mettre en mémoire, dans l'ordre où l'app les montre : icônes des apps, avatars des
    /// comptes, puis les vidéos les plus récentes. `limite` borne les vidéos à ce que le cache mémoire tient.
    pub fn images(&self, limite: i64) -> Res<Vec<Vec<String>>> {
        let mut apps: Vec<String> = self.businesses.iter().filter_map(|b| b.config.app_store_id.clone()).collect();
        apps.extend(self.q("select id from app where suivie = 1", p![])?.iter().filter_map(|l| l.texte("id")));
        let comptes = self.q(&format!("select id from compte where business in {}", self.dans()), self.ids())?;
        let sql = format!("
            select v.id, v.plateforme from video v join compte c on c.plateforme = v.plateforme and c.handle = v.compte
            where v.business in {} order by v.publiee_le desc limit ?", self.dans());
        let videos = self.q(&sql, self.autour(p![], p![limite]))?;
        Ok(apps.iter().map(|a| vec![miniatures::icone(a)])
            .chain(comptes.iter().filter_map(|l| l.texte("id")).map(|c| vec![miniatures::avatar(&c)]))
            .chain(videos.iter().filter_map(|l| {
                let id = l.texte("id")?;
                Plateforme::depuis(&l.texte("plateforme").unwrap_or_default()).map(|p| miniatures::cles(&id, p))
            }))
            .collect())
    }
}
