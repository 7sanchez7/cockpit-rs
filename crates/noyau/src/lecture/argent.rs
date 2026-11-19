//! L'argent : le MRR face à l'objectif, le revenu cumulé, les retours des cohortes et les pertes du tunnel.
use super::Lectures;
use crate::erreur::Res;
use crate::jour::{self, Date};
use crate::metrique;
use crate::modeles::Cohorte;
use chrono::Duration;
use serde::Serialize;

/// Le MRR brut de RevenueCat face à l'objectif mensuel, en euros au taux `metrique::EUR_PAR_USD`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Objectif {
    pub cible_eur: f64,
    pub mrr_eur: f64,
    /// Gain de MRR par jour sur les 30 derniers jours ; None sans relevé 30 jours avant.
    pub rythme_par_jour_eur: Option<f64>,
}

impl Objectif {
    pub fn part(&self) -> f64 { if self.cible_eur > 0.0 { (self.mrr_eur / self.cible_eur).clamp(0.0, 1.0) } else { 0.0 } }
    pub fn reste_eur(&self) -> f64 { (self.cible_eur - self.mrr_eur).max(0.0) }

    /// Quand le MRR atteindrait la cible à ce rythme ; None s'il ne monte pas ou s'il faudrait plus de cinq ans.
    pub fn arrivee(&self, maintenant: &Date) -> Option<Date> {
        let r = self.rythme_par_jour_eur.filter(|r| *r > 0.0)?;
        if self.reste_eur() <= 0.0 { return None; }
        let jours = self.reste_eur() / r;
        (jours <= 5.0 * 365.0).then(|| *maintenant + Duration::milliseconds((jours * 86_400_000.0) as i64))
    }
}

/// Le retour JN de cohortes complètes, pondéré par leurs installs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TauxRetour {
    pub taux: f64,
    pub cohortes: i64,
    /// Le même taux sur autant de cohortes juste avant ; None s'il n'y en a pas assez.
    pub avant: Option<f64>,
}

/// Le jour de retour lu dans une cohorte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Jn { J1, J7, J14, J30 }

impl Jn {
    pub fn de(self, c: &Cohorte) -> Option<i64> {
        match self { Jn::J1 => c.j1, Jn::J7 => c.j7, Jn::J14 => c.j14, Jn::J30 => c.j30 }
    }
}

impl Cohorte {
    /// JN rapporté aux installs ; None tant que la cohorte n'a pas eu N + 1 jours.
    pub fn taux(&self, jn: Jn) -> Option<f64> {
        let n = jn.de(self)?;
        (self.installs > 0).then(|| n as f64 / self.installs as f64)
    }
}

/// Taux pondéré des `taille` cohortes complètes les plus récentes (liste la plus récente d'abord),
/// comparé aux `taille` d'avant si elles sont toutes là.
pub fn retour(cohortes: &[Cohorte], jn: Jn, taille: usize) -> Option<TauxRetour> {
    let completes: Vec<&Cohorte> = cohortes.iter().filter(|c| c.installs > 0 && jn.de(c).is_some()).collect();
    let pondere = |c: &[&Cohorte]| {
        c.iter().map(|c| jn.de(c).unwrap_or(0)).sum::<i64>() as f64 / c.iter().map(|c| c.installs).sum::<i64>() as f64
    };
    let recentes = &completes[..completes.len().min(taille)];
    if recentes.is_empty() { return None; }
    let precedentes = &completes[recentes.len()..completes.len().min(2 * taille)];
    Some(TauxRetour {
        taux: pondere(recentes),
        cohortes: recentes.len() as i64,
        avant: (recentes.len() == taille && precedentes.len() == taille).then(|| pondere(precedentes)),
    })
}

/// Le taux JN de chaque cohorte complète, de la plus ancienne à la plus récente.
pub fn serie(cohortes: &[Cohorte], jn: Jn) -> Vec<f64> { cohortes.iter().rev().filter_map(|c| c.taux(jn)).collect() }

/// Le revenu cumulé d'un jour.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CumulJour {
    pub jour: String,
    pub euros: f64,
}

impl Lectures<'_> {
    pub fn objectif(&self, jour: &str) -> Res<Option<Objectif>> {
        let Some(mrr) = self.valeur(metrique::MRR_BRUT, jour, false)? else { return Ok(None) };
        let avant = self.valeur(metrique::MRR_BRUT, &jour::decale(jour, -30), false)?;
        Ok(Some(Objectif {
            cible_eur: self.businesses.iter().map(|b| b.config.objectif_mensuel_eur).sum(),
            mrr_eur: metrique::en_euros(mrr),
            rythme_par_jour_eur: avant.map(|a| metrique::en_euros(mrr - a) / 30.0),
        }))
    }

    /// Le revenu brut en euros cumulé jour par jour sur les `n` jours jusqu'à `fin`, les jours sans relevé sautés.
    pub fn revenu_cumule(&self, n: i64, fin: &str) -> Res<Vec<CumulJour>> {
        let mut total = 0.0;
        Ok(jour::derniers(n, fin).into_iter().zip(self.jours(metrique::REVENU, n, fin, false)?).filter_map(|(cle, v)| {
            total += metrique::en_euros(v?);
            Some(CumulJour { jour: cle, euros: total })
        }).collect())
    }

    /// Somme des `n` jours jusqu'à `jusqua` ; None si aucun n'a de relevé.
    pub fn somme(&self, m: &str, n: i64, jusqua: &str) -> Res<Option<f64>> {
        let v: Vec<f64> = self.jours(m, n, jusqua, false)?.into_iter().flatten().collect();
        Ok((!v.is_empty()).then(|| v.iter().sum()))
    }
}

/// L'étape qui perd le plus de monde par rapport à la précédente ; None sans perte. La première des ex aequo.
pub fn plus_grosse_perte(etapes: &[(&str, i64)]) -> Option<usize> {
    let mut pire: Option<(usize, i64)> = None;
    for i in 1..etapes.len() {
        let perte = etapes[i - 1].1 - etapes[i].1;
        if pire.is_none_or(|p| p.1 < perte) { pire = Some((i, perte)); }
    }
    pire.filter(|p| p.1 > 0).map(|p| p.0)
}
