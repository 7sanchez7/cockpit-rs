//! Les lectures de l'Accueil : le plan tiré de Notes, les comptes du jour, le score de la semaine.
use super::fenetre::Cumul;
use super::videos::LigneCompte;
use super::Lectures;
use crate::erreur::Res;
use crate::jour;
use crate::metrique;
use crate::modeles::LignePlan;
use crate::p;
use serde::Serialize;
use std::collections::HashMap;

/// Le rythme visé par compte actif : trois vidéos par jour.
pub const VIDEOS_PAR_JOUR: i64 = 3;

impl Lectures<'_> {
    /// Le plan de la note « Cockpit », dans l'ordre de la note ; il ne dépend pas du business affiché.
    pub fn plan(&self) -> Res<Vec<LignePlan>> {
        Ok(self.q("select section, texte, coche, fait from plan order by ordre", p![])?.iter().map(|l| LignePlan {
            section: l.texte("section").unwrap_or_default(),
            texte: l.texte("texte").unwrap_or_default(),
            coche: l.booleen("coche"),
            fait: l.booleen("fait"),
        }).collect())
    }

    /// Chaque compte suivi avec ses vidéos publiées aujourd'hui, sa dernière publication et les vues prises sur 24 h.
    pub fn reseau(&self) -> Res<Vec<LigneReseau>> {
        let minuit = jour::date(&jour::cle(&self.maintenant)).unwrap_or(self.maintenant);
        let sql = format!("
            select plateforme, compte, sum(publiee_le >= ?) as aujourdhui, max(publiee_le) as derniere
            from video where business in {} group by plateforme, compte", self.dans());
        let mut publiees = HashMap::new();
        for l in self.q(&sql, self.autour(p![minuit], p![]))? {
            let cle = format!("{}:{}", l.texte("plateforme").unwrap_or_default(), l.texte("compte").unwrap_or_default());
            publiees.insert(cle, (l.entier("aujourdhui").unwrap_or(0), l.date("derniere")));
        }
        let mut vues: HashMap<String, i64> = HashMap::new();
        for v in self.videos(1000)? {
            if let Some(g) = v.gain24h { *vues.entry(format!("{}:{}", v.plateforme.brut(), v.compte)).or_default() += g; }
        }
        Ok(self.comptes()?.into_iter().map(|c| {
            let p = publiees.get(&c.id);
            LigneReseau {
                aujourdhui: p.map(|p| p.0).unwrap_or(0),
                derniere: p.and_then(|p| p.1),
                vues24h: vues.get(&c.id).copied(),
                compte: c,
            }
        }).collect())
    }

    /// Les sept jours complets jusqu'à `fin` contre les sept d'avant, axe par axe.
    pub fn score(&self, fin: &str) -> Res<Score> {
        fn semaines(v: &[Option<f64>]) -> (Option<f64>, Option<f64>) {
            let n = v.len();
            (Cumul::Somme.de(&v[n.saturating_sub(7)..]), Cumul::Somme.de(&v[..n.min(7)]))
        }
        fn taux(e: Option<f64>, i: Option<f64>) -> Option<f64> { i.and_then(|i| (i > 0.0).then(|| e.unwrap_or(0.0) / i)) }
        let videos = semaines(&self.publications(14, fin)?.into_iter().map(Some).collect::<Vec<_>>());
        let vues = semaines(&self.jours(metrique::VUES_TIKTOK, 14, fin, false)?);
        let installs = semaines(&self.jours(metrique::INSTALLS, 14, fin, false)?);
        let essais = semaines(&self.jours(metrique::ESSAIS, 14, fin, false)?);
        let revenu = semaines(&self.jours(metrique::REVENU, 14, fin, false)?);
        let axe = |nom: &str, c: Option<f64>, a: Option<f64>| AxeScore { nom: nom.into(), note: Score::note(c, a) };
        Ok(Score {
            axes: vec![
                axe("Vidéos", videos.0, videos.1),
                axe("Vues", vues.0, vues.1),
                axe("Installs", installs.0, installs.1),
                axe("Conversion", taux(essais.0, installs.0), taux(essais.1, installs.1)),
                axe("Revenu", revenu.0, revenu.1),
            ],
        })
    }
}

/// Les sept jours de la semaine de `jour`, du lundi au dimanche.
pub fn semaine(jour: &str) -> Vec<String> {
    let Some(j) = jour::jour_semaine(jour) else { return vec![] };
    let lundi = jour::decale(jour, -(j as i64));
    (0..7).map(|i| jour::decale(&lundi, i)).collect()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LigneReseau {
    pub compte: LigneCompte,
    /// Vidéos publiées depuis minuit, heure de Paris.
    pub aujourdhui: i64,
    pub derniere: Option<jour::Date>,
    /// Vues prises sur 24 h par ses vidéos ; None sans relevé assez ancien.
    pub vues24h: Option<i64>,
}

impl LigneReseau {
    pub fn id(&self) -> &str { &self.compte.id }
}

/// Un axe du score : la semaine écoulée rapportée à celle d'avant, sur 100.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AxeScore {
    pub nom: String,
    /// Plafonnée à 100 : faire au moins aussi bien que la semaine d'avant ; None sans référence.
    pub note: Option<i64>,
}

/// Le score de l'Accueil, la moyenne de ses axes notés.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Score {
    pub axes: Vec<AxeScore>,
}

impl Score {
    pub fn global(&self) -> Option<i64> {
        let notes: Vec<i64> = self.axes.iter().filter_map(|a| a.note).collect();
        (!notes.is_empty()).then(|| (notes.iter().sum::<i64>() as f64 / notes.len() as f64).round() as i64)
    }

    /// L'axe le plus bas, s'il est sous 100 ; le premier des ex aequo.
    pub fn faible(&self) -> Option<&AxeScore> {
        let mut r: Option<&AxeScore> = None;
        for a in self.axes.iter().filter(|a| a.note.unwrap_or(100) < 100) {
            if r.is_none_or(|m| a.note.unwrap_or(0) < m.note.unwrap_or(0)) { r = Some(a); }
        }
        r
    }

    pub fn note(courant: Option<f64>, avant: Option<f64>) -> Option<i64> {
        let (c, a) = (courant?, avant?);
        (a > 0.0).then(|| 100.min((100.0 * c / a).round() as i64))
    }
}
