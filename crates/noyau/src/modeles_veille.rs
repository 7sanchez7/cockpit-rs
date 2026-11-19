//! La veille : vidéos des autres trouvées par mot-clé, et leur score.
use crate::db::Ligne;
use crate::jour::{self, Date};
use crate::modeles::Plateforme;
use serde::{Deserialize, Serialize};

/// Une vidéo vue dans un résultat de recherche ou sur un profil.
#[derive(Debug, Clone, PartialEq)]
pub struct Trouvaille {
    pub plateforme: Plateforme,
    pub brut: String,
    pub compte: String,
    pub legende: Option<String>,
    pub publiee: Option<Date>,
    pub duree_s: Option<f64>,
    pub vues: Option<i64>,
    pub likes: Option<i64>,
    pub commentaires: Option<i64>,
    pub partages: Option<i64>,
    pub enregistrements: Option<i64>,
    pub abonnes: Option<i64>,
    pub image: Option<String>,
    pub media: Option<String>,
}

impl Trouvaille {
    pub fn nouvelle(plateforme: Plateforme, brut: &str, compte: &str) -> Trouvaille {
        Trouvaille {
            plateforme, brut: brut.into(), compte: compte.into(), legende: None, publiee: None, duree_s: None,
            vues: None, likes: None, commentaires: None, partages: None, enregistrements: None, abonnes: None,
            image: None, media: None,
        }
    }

    pub fn id(&self) -> String { self.plateforme.id_video(&self.brut) }
    pub fn url(&self) -> String { self.plateforme.lien(&self.brut, &self.compte) }

    /// Deux lectures de la même vidéo : le premier champ connu, le plus grand compteur.
    pub fn fusion(&self, autre: &Trouvaille) -> Trouvaille {
        fn max(a: Option<i64>, b: Option<i64>) -> Option<i64> { a.into_iter().chain(b).max() }
        let mut t = self.clone();
        t.legende = self.legende.clone().or_else(|| autre.legende.clone());
        t.publiee = self.publiee.or(autre.publiee);
        t.duree_s = self.duree_s.or(autre.duree_s);
        t.vues = max(self.vues, autre.vues);
        t.likes = max(self.likes, autre.likes);
        t.commentaires = max(self.commentaires, autre.commentaires);
        t.partages = max(self.partages, autre.partages);
        t.enregistrements = max(self.enregistrements, autre.enregistrements);
        t.abonnes = self.abonnes.or(autre.abonnes);
        t.image = self.image.clone().or_else(|| autre.image.clone());
        t.media = self.media.clone().or_else(|| autre.media.clone());
        t
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MotVeille {
    pub business: String,
    pub mot: String,
    #[serde(rename = "vuesMin")]
    pub vues_min: i64,
    pub actif: bool,
    pub cherche: Option<Date>,
}

impl MotVeille {
    pub fn id(&self) -> String { format!("{}:{}", self.business, self.mot) }
}

/// Une demande libre à Claude : Sonnet choisit les mots-clés, Haiku cherche, analyse et retient les vidéos.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DemandeVeille {
    pub id: String,
    pub business: String,
    pub prompt: String,
    pub debut: Date,
    pub fin: Option<Date>,
    pub etat: String,
    pub journal: Vec<String>,
    pub synthese: Option<String>,
    pub erreur: Option<String>,
    /// La conversation : l'id de sa première demande, le sien pour une première demande.
    pub fil: String,
}

impl DemandeVeille {
    pub const EN_COURS: &'static str = "en_cours";
    pub const FINIE: &'static str = "finie";
    pub const ECHOUEE: &'static str = "echouee";

    pub fn depuis_ligne(l: &Ligne) -> Option<DemandeVeille> {
        Some(DemandeVeille {
            id: l.texte("id")?, business: l.texte("business")?, prompt: l.texte("prompt")?, debut: l.date("debut")?,
            fin: l.date("fin"), etat: l.texte("etat")?,
            journal: l.texte("journal").and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default(),
            synthese: l.texte("synthese"), erreur: l.texte("erreur"), fil: l.texte("fil").or_else(|| l.texte("id"))?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VideoVeille {
    pub id: String,
    pub business: String,
    pub plateforme: Plateforme,
    pub compte: String,
    pub url: String,
    pub legende: Option<String>,
    pub publiee: Option<Date>,
    #[serde(rename = "dureeS")]
    pub duree_s: Option<f64>,
    pub mot: Option<String>,
    pub trouvee: Date,
    pub media: Option<String>,
    pub vues: Option<i64>,
    pub likes: Option<i64>,
    pub commentaires: Option<i64>,
    pub partages: Option<i64>,
    pub enregistrements: Option<i64>,
    #[serde(rename = "releveLe")]
    pub releve_le: Option<Date>,
    pub mediane: Option<i64>,
    pub abonnes: Option<i64>,
    /// Vues gagnées par heure entre les deux derniers relevés.
    pub vitesse: Option<f64>,
    pub analyse: Option<AnalyseVeille>,
    /// Gardée par une demande à Claude.
    pub retenue: bool,
    /// L'app ou le produit dont la vidéo fait la promo, quand il est clairement nommé (classement).
    pub app: Option<String>,
    /// La langue parlée ou écrite, code de `LANGUES` (classement).
    pub langue: Option<String>,
    /// Le genre de la vidéo, un de `GENRES` (classement).
    pub genre: Option<String>,
}

impl VideoVeille {
    pub const SQL: &'static str = "
        select v.id, v.business, v.plateforme, v.compte, v.url, v.legende, v.publiee, v.duree_s, v.mot, v.trouvee, v.media,
            v.app, v.langue, v.genre,
            r.vues, r.likes, r.commentaires, r.partages, r.enregistrements, r.at as releve_at,
            c.mediane, c.abonnes, a.donnees as analyse,
            exists (select 1 from veille_retenue t where t.video = v.id) as retenue,
            (select p.vues from veille_releve p where p.video = v.id and p.at < r.at order by p.at desc limit 1) as vues_avant,
            (select p.at from veille_releve p where p.video = v.id and p.at < r.at order by p.at desc limit 1) as at_avant
        from veille_video v
        left join veille_releve r on r.video = v.id and r.at = (select max(at) from veille_releve where video = v.id)
        left join veille_compte c on c.id = v.plateforme || ':' || v.compte
        left join veille_analyse a on a.video = v.id";

    pub fn depuis_ligne(l: &Ligne) -> Option<VideoVeille> {
        let id = l.texte("id")?;
        let plateforme = Plateforme::depuis(&l.texte("plateforme").unwrap_or_default())?;
        let url = l.texte("url")?;
        let vues = l.entier("vues");
        let releve_le = l.date("releve_at");
        let vitesse = match (vues, l.entier("vues_avant"), l.date("at_avant"), releve_le) {
            (Some(v), Some(avant), Some(at), Some(fin)) => {
                let heures = (fin - at).num_milliseconds() as f64 / 3_600_000.0;
                (heures >= 1.0).then(|| (v - avant) as f64 / heures)
            }
            _ => None,
        };
        Some(VideoVeille {
            id, business: l.texte("business").unwrap_or_default(), plateforme, compte: l.texte("compte").unwrap_or_default(),
            url, legende: l.texte("legende"), publiee: l.date("publiee"), duree_s: l.nombre("duree_s"), mot: l.texte("mot"),
            trouvee: l.date("trouvee").unwrap_or_else(jour::loin_passe), media: l.texte("media"), vues,
            likes: l.entier("likes"), commentaires: l.entier("commentaires"), partages: l.entier("partages"),
            enregistrements: l.entier("enregistrements"), releve_le, mediane: l.entier("mediane"), abonnes: l.entier("abonnes"),
            vitesse, analyse: l.texte("analyse").and_then(|s| serde_json::from_str(&s).ok()), retenue: l.booleen("retenue"),
            app: l.texte("app"), langue: l.texte("langue"), genre: l.texte("genre"),
        })
    }

    pub fn ratio(&self) -> Option<f64> { ScoreVeille::ratio(self.vues, self.mediane) }
    pub fn pull(&self) -> Option<f64> {
        ScoreVeille::pull(self.vues, self.likes, self.commentaires, self.partages, self.enregistrements)
    }
    /// (likes + commentaires + partages + enregistrements) / vues.
    pub fn engagement(&self) -> Option<f64> {
        let vues = self.vues.filter(|v| *v > 0)?;
        let actes = [self.likes, self.commentaires, self.partages, self.enregistrements];
        if actes.iter().all(Option::is_none) { return None; }
        Some(actes.iter().flatten().sum::<i64>() as f64 / vues as f64)
    }
    pub fn outlier(&self) -> bool { ScoreVeille::outlier(self.ratio(), self.vues, self.pull()) }
    /// Vaut une analyse de Claude : outlier, ou au moins `RATIO_PERCEE` fois la médiane de son compte.
    pub fn a_analyser(&self) -> bool { self.outlier() || self.ratio().is_some_and(|r| r >= ScoreVeille::RATIO_PERCEE) }
    /// L'app promue : celle de l'analyse (Claude a vu la vidéo), sinon celle du classement (légende et compte).
    pub fn app_promue(&self) -> Option<&str> {
        self.analyse.as_ref().and_then(|a| a.app.as_deref()).or(self.app.as_deref())
    }
    pub fn nom_langue(code: &str) -> &str {
        Self::LANGUES.iter().find(|(c, _)| *c == code).map_or(code, |(_, n)| n)
    }

    /// Les langues du classement : (code, nom).
    pub const LANGUES: [(&'static str, &'static str); 7] = [
        ("fr", "Français"), ("en", "Anglais"), ("es", "Espagnol"), ("pt", "Portugais"), ("it", "Italien"), ("de", "Allemand"),
        ("autre", "Autre langue"),
    ];
    pub const GENRES: [&'static str; 9] = [
        "cours", "astuces", "sketch", "storytime", "réaction", "témoignage", "trend", "pub", "autre",
    ];
}

/// Le classement d'une vidéo par Claude, d'après sa légende, son compte et son analyse.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Classement {
    pub app: Option<String>,
    pub langue: Option<String>,
    pub genre: Option<String>,
}

/// Ce que Claude a vu d'une vidéo de veille (JSON dans `veille_analyse`, mêmes clés que la version macOS).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyseVeille {
    pub hook: String,
    #[serde(rename = "hookType")]
    pub hook_type: String,
    #[serde(rename = "hookVisuel")]
    pub hook_visuel: String,
    #[serde(rename = "texteEcran")]
    pub texte_ecran: String,
    pub probleme: String,
    pub format: String,
    pub blocs: Blocs,
    /// La part de la durée de chaque bloc, en % ; absente des analyses d'avant le 19 nov. 2026.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<Parts>,
    pub ecart: String,
    pub adaptable: i64,
    pub idee: String,
    /// L'app ou le produit promu, s'il est clairement nommé ; absent des analyses d'avant le 19 nov. 2026.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Blocs {
    pub hook: String,
    pub confirmation: String,
    pub valeur: String,
    pub placement: String,
    pub fin: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parts {
    pub hook: i64,
    pub confirmation: i64,
    pub valeur: i64,
    pub placement: i64,
    pub fin: i64,
}

impl AnalyseVeille {
    pub const HOOKS: [&'static str; 16] = [
        "question", "défi", "chiffre choc", "erreur courante", "promesse", "histoire", "contre-intuitif", "liste",
        "comparaison", "avant/après", "test de niveau", "POV", "tuto", "opinion forte", "secret", "réaction",
    ];
    pub const VISUELS: [&'static str; 12] = [
        "visage face caméra", "texte plein écran", "écran d'app", "écran coupé", "b-roll lieu", "objet en main",
        "mouvement brusque", "geste", "meme ou extrait", "avant/après visuel", "sous-titres seuls", "animation",
    ];
}

/// Le score d'une vidéo de veille : vues contre la médiane du compte, et engagement.
pub struct ScoreVeille;

impl ScoreVeille {
    pub const VIDEOS_BASE: usize = 20;
    pub const MIN_BASE: usize = 5;
    pub const JEUNE_S: i64 = 48 * 3_600;
    pub const RATIO_OUTLIER: f64 = 3.0;
    pub const VUES_OUTLIER: i64 = 10_000;
    pub const PULL_OUTLIER: f64 = 0.005;
    pub const RATIO_PERCEE: f64 = 2.0;

    /// La médiane des vues des 20 dernières vidéos de plus de 48 h ; `None` sous 5 vidéos.
    pub fn mediane(videos: &[(Option<Date>, i64)], maintenant: &Date) -> Option<i64> {
        let mut v: Vec<&(Option<Date>, i64)> = videos
            .iter()
            .filter(|(p, _)| p.map(|p| (*maintenant - p).num_seconds() >= Self::JEUNE_S).unwrap_or(true))
            .collect();
        v.sort_by(|a, b| b.0.unwrap_or_else(jour::loin_passe).cmp(&a.0.unwrap_or_else(jour::loin_passe)));
        let mut vues: Vec<i64> = v.into_iter().take(Self::VIDEOS_BASE).map(|x| x.1).collect();
        vues.sort();
        if vues.len() < Self::MIN_BASE { return None; }
        let m = vues.len() / 2;
        Some(if vues.len().is_multiple_of(2) { (vues[m - 1] + vues[m]) / 2 } else { vues[m] })
    }

    pub fn ratio(vues: Option<i64>, mediane: Option<i64>) -> Option<f64> {
        match (vues, mediane) {
            (Some(v), Some(m)) if m > 0 => Some(v as f64 / m as f64),
            _ => None,
        }
    }

    pub fn pull(vues: Option<i64>, likes: Option<i64>, commentaires: Option<i64>, partages: Option<i64>, enregistrements: Option<i64>) -> Option<f64> {
        let vues = vues.filter(|v| *v > 0)?;
        let actes = 4 * partages.unwrap_or(0) + 4 * enregistrements.unwrap_or(0) + 2 * commentaires.unwrap_or(0) + likes.unwrap_or(0);
        Some(actes as f64 / vues as f64)
    }

    pub fn outlier(ratio: Option<f64>, vues: Option<i64>, pull: Option<f64>) -> bool {
        match (ratio, vues, pull) {
            (Some(r), Some(v), Some(p)) => r >= Self::RATIO_OUTLIER && v >= Self::VUES_OUTLIER && p >= Self::PULL_OUTLIER,
            _ => false,
        }
    }

    /// Une vidéo de moins de 48 h déjà à deux fois la médiane de son compte.
    pub fn percee(v: &VideoVeille, maintenant: &Date) -> bool {
        match (v.publiee, v.ratio()) {
            (Some(p), Some(r)) => (*maintenant - p).num_seconds() < Self::JEUNE_S && r >= Self::RATIO_PERCEE,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn mediane_et_score() {
        let m = jour::maintenant();
        let vieux = |h: i64, v: i64| (Some(m - Duration::hours(h)), v);
        let videos = vec![vieux(10, 999_999), vieux(50, 100), vieux(60, 200), vieux(70, 300), vieux(80, 400), (None, 500)];
        assert_eq!(ScoreVeille::mediane(&videos, &m), Some(300));
        assert_eq!(ScoreVeille::mediane(&videos[..4], &m), None);
        assert_eq!(ScoreVeille::pull(Some(1000), Some(10), Some(5), Some(1), Some(0)), Some(0.024));
        assert!(ScoreVeille::outlier(Some(3.0), Some(10_000), Some(0.005)));
    }
}
