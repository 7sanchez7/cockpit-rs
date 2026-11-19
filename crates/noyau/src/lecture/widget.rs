//! Le résumé du widget et de la barre de menus : les chiffres d'aujourd'hui de l'Accueil, déjà mis en forme.
use super::fenetre::{Cumul, Fenetre, Periode};
use super::{format, Lectures};
use crate::erreur::Res;
use crate::jour;
use crate::metrique;
use serde::{Deserialize, Serialize};

/// Ce qu'affiche le widget du bureau, déjà mis en forme (mêmes clés que `widget.json` de la version macOS).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeWidget {
    /// L'heure de la dernière collecte, précédée du jour si ce n'est pas aujourd'hui.
    pub maj: Option<String>,
    /// Le revenu brut du jour en euros, `hier 31 €`, `3 essais, 1 achat`.
    pub revenu: String,
    pub hier: String,
    pub detail: String,
    /// Le revenu brut cumulé sur `jours` jours, en euros.
    pub cumul: Vec<f64>,
    pub jours: i64,
    pub kpis: Vec<Kpi>,
    pub a_traiter: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Kpi {
    pub libelle: String,
    pub valeur: String,
    pub delta: Option<String>,
    /// L'écart signé qui donne sa couleur au delta et à la courbe.
    pub ecart: Option<f64>,
    pub note: Option<String>,
    /// Les 14 derniers jours.
    pub serie: Vec<Option<f64>>,
}

impl Lectures<'_> {
    /// Le résumé du widget : les chiffres d'aujourd'hui de l'Accueil, comparés et annotés comme l'Accueil.
    pub fn resume_widget(&self) -> Res<ResumeWidget> {
        let f = Fenetre { periode: Periode::Jour, maintenant: self.maintenant };
        let fin = f.fin();
        let n = Periode::Mois.n();
        let euros = |v: Option<f64>| v.map(|v| format::euros(metrique::en_euros(v)));
        let kpi = |libelle: &str, c: &super::fenetre::Chiffres, delta: Option<String>, note: Option<String>| Kpi {
            libelle: libelle.into(),
            valeur: c.valeur.map(format::entier).unwrap_or_else(|| format::VIDE.into()),
            delta,
            ecart: format::ecart(c.valeur, c.avant),
            note,
            serie: c.serie.clone(),
        };

        let essais = f.mesure(self, metrique::ESSAIS, Some(metrique::ESSAIS_J7), Cumul::Somme)?;
        let installs = f.mesure(self, metrique::INSTALLS, Some(metrique::INSTALLS_J7), Cumul::Somme)?;
        let actifs = f.mesure(self, metrique::ACTIFS, Some(metrique::ACTIFS_J7), Cumul::Somme)?;
        let vues = f.mesure(self, metrique::VUES_TIKTOK, Some(metrique::VUES_TIKTOK_J7), Cumul::Somme)?;
        let abonnes = f.mesure(self, metrique::ABONNES, None, Cumul::Dernier)?;
        let achats = f.mesure(self, metrique::ACHATS, None, Cumul::Somme)?;
        let ronds = f.mesure(self, metrique::RONDS, Some(metrique::RONDS_J7), Cumul::Somme)?;
        let conversion = installs.valeur.and_then(|i| (i > 0.0).then(|| format::pourcent(essais.valeur.unwrap_or(0.0) / i, 0)));
        let publiees: f64 = self.publications(f.n(), &fin)?.iter().sum();
        let detail = [format::nombre(essais.valeur, "essai"), format::nombre(achats.valeur, "achat")]
            .into_iter().flatten().collect::<Vec<_>>().join(", ");
        // même jour à l'heure de Paris
        let maj = self.derniers_passages()?.values().map(|p| p.fin).max().map(|d| {
            if jour::cle(&d) == jour::cle(&self.maintenant) { format::heure(&d) } else { format::jour_heure(&d) }
        });

        Ok(ResumeWidget {
            maj,
            revenu: euros(self.valeur(metrique::REVENU, &fin, false)?).unwrap_or_else(|| format::VIDE.into()),
            hier: format!("hier {}", euros(self.valeur(metrique::REVENU, &jour::decale(&fin, -1), false)?)
                .unwrap_or_else(|| format::VIDE.into())),
            detail: if detail.is_empty() { format::VIDE.into() } else { detail },
            cumul: self.revenu_cumule(n, &fin)?.into_iter().map(|c| c.euros).collect(),
            jours: n,
            kpis: vec![
                kpi("Abonnés", &abonnes, format::unites(format::ecart(abonnes.valeur, abonnes.avant)),
                    euros(self.valeur(metrique::MRR_BRUT, &fin, false)?).map(|e| format!("MRR {e}"))),
                kpi("Essais", &essais, format::unites(format::ecart(essais.valeur, essais.avant)),
                    self.valeur(metrique::ESSAIS_ACTIFS, &fin, false)?.map(|v| format!("{} en cours", format::entier(v)))),
                kpi("Installs", &installs, format::evolution(installs.valeur, installs.avant),
                    conversion.map(|c| format!("{c} en essai"))),
                kpi("Actifs", &actifs, format::evolution(actifs.valeur, actifs.avant), format::nombre(ronds.valeur, "rond")),
                kpi("Vues TikTok", &vues, format::evolution(vues.valeur, vues.avant),
                    format::nombre(Some(publiees), "vidéo")
                        .map(|t| format!("{t}{}", if publiees < 2.0 { " publiée" } else { " publiées" }))),
            ],
            a_traiter: self.nb_a_traiter()?,
        })
    }
}
