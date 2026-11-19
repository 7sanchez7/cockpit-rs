//! La période de l'Accueil et de Data, et la façon d'en tirer un chiffre comparé.
use super::{format, Lectures};
use crate::erreur::Res;
use crate::jour::{self, Date};
use serde::Serialize;

/// La journée en cours, ou des jours complets jusqu'à hier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Periode { Jour, Semaine, Mois, Trimestre }

impl Periode {
    pub const TOUTES: [Periode; 4] = [Periode::Jour, Periode::Semaine, Periode::Mois, Periode::Trimestre];

    /// Le nombre de jours.
    pub fn n(self) -> i64 {
        match self { Periode::Jour => 1, Periode::Semaine => 7, Periode::Mois => 30, Periode::Trimestre => 90 }
    }

    pub fn texte(self) -> String { if self == Periode::Jour { "Aujourd'hui".into() } else { format!("{} j", self.n()) } }
}

/// Les jours de la période et la façon de les comparer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fenetre {
    pub periode: Periode,
    pub maintenant: Date,
}

impl Fenetre {
    pub fn jour(&self) -> bool { self.periode == Periode::Jour }
    pub fn n(&self) -> i64 { self.periode.n() }
    /// Le dernier jour de la fenêtre : aujourd'hui, ou hier pour des jours complets.
    pub fn fin(&self) -> String { jour::decale(&jour::cle(&self.maintenant), if self.jour() { 0 } else { -1 }) }
    pub fn debut(&self) -> String { jour::decale(&self.fin(), 1 - self.n()) }
    /// Minuit du premier jour, à l'heure de Paris.
    pub fn du(&self) -> Date { jour::date(&self.debut()).unwrap_or(self.maintenant) }
    /// Minuit après le dernier jour.
    pub fn au(&self) -> Date { jour::date(&jour::decale(&self.fin(), 1)).unwrap_or(self.maintenant) }

    /// Autant de jours juste avant ; None pour aujourd'hui, une journée entamée ne se compare pas à une journée entière.
    pub fn avant(&self) -> Option<(String, String)> {
        let debut = self.debut();
        (!self.jour()).then(|| (jour::decale(&debut, -self.n()), jour::decale(&debut, -1)))
    }

    pub fn sous_titre(&self) -> String {
        if self.jour() {
            return format!("{}, comparé à {} dernier à la même heure", format::date_longue(&self.maintenant),
                           format::jour_semaine(&self.maintenant));
        }
        format!("Du {} au {}, comparé aux {} jours d'avant", format::jour_court_cle(&self.debut()),
                format::jour_court_cle(&self.fin()), self.n())
    }

    /// Aujourd'hui : la valeur du jour contre `j7` (même heure une semaine avant), ou contre J-7 entier pour un `Dernier`.
    /// Sur N jours : la fenêtre cumulée contre celle d'avant, comparée seulement si celle-ci est relevée jour par jour.
    pub fn mesure(&self, l: &Lectures, m: &str, j7: Option<&str>, cumul: Cumul) -> Res<Chiffres> {
        let fin = self.fin();
        if self.jour() {
            let avant = match (cumul, j7) {
                (Cumul::Dernier, _) => l.valeur(m, &jour::decale(&fin, -7), false)?,
                (_, Some(j7)) => l.valeur(j7, &fin, false)?,
                (_, None) => None,
            };
            return Ok(Chiffres { valeur: l.valeur(m, &fin, false)?, avant, serie: l.jours(m, 14, &fin, false)?, jours: None });
        }
        let n = self.n() as usize;
        let deux = l.jours(m, 2 * self.n(), &fin, false)?;
        let (avant, courant) = deux.split_at(deux.len() - n);
        let releves = courant.iter().flatten().count();
        Ok(Chiffres {
            valeur: cumul.de(courant),
            avant: if avant.contains(&None) { None } else { cumul.de(avant) },
            serie: courant.to_vec(),
            jours: (cumul != Cumul::Dernier && releves > 0 && releves < n).then_some(releves as i64),
        })
    }
}

/// Comment une fenêtre de jours devient un chiffre : un total, une moyenne par jour, ou le dernier relevé (le MRR).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Cumul { Somme, Moyenne, Dernier }

impl Cumul {
    pub fn de(self, jours: &[Option<f64>]) -> Option<f64> {
        let v: Vec<f64> = jours.iter().flatten().copied().collect();
        let d = *v.last()?;
        Some(match self {
            Cumul::Somme => v.iter().sum(),
            Cumul::Moyenne => v.iter().sum::<f64>() / v.len() as f64,
            Cumul::Dernier => d,
        })
    }
}

/// Une carte : la valeur de la période, sa référence, la courbe.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Chiffres {
    pub valeur: Option<f64>,
    pub avant: Option<f64>,
    pub serie: Vec<Option<f64>>,
    /// Les jours relevés quand une somme ou une moyenne ne couvre qu'une partie de la période, sinon None.
    pub jours: Option<i64>,
}

impl Chiffres {
    /// « sur 8 j » quand la période n'est couverte qu'en partie.
    pub fn couverture(&self) -> Option<String> { self.jours.map(|j| format!("sur {j} j")) }

    /// Les mêmes chiffres passés par `t`, par exemple des dollars en euros.
    pub fn map(&self, t: impl Fn(f64) -> f64) -> Chiffres {
        Chiffres {
            valeur: self.valeur.map(&t),
            avant: self.avant.map(&t),
            serie: self.serie.iter().map(|v| v.map(&t)).collect(),
            jours: self.jours,
        }
    }
}
