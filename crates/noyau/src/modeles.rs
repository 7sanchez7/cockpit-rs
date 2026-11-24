//! Les objets que les sources écrivent et que les lectures rendent.
use crate::jour::Date;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Une ligne du fil. `id` commence par l'id de sa source suivi de `:`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Evenement {
    pub id: String,
    pub business: String,
    pub at: Date,
    #[serde(rename = "type")]
    pub type_: TypeFil,
    pub texte: String,
    pub lien: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TypeFil {
    #[serde(rename = "ESSAI")] Essai,
    #[serde(rename = "ACHAT")] Achat,
    #[serde(rename = "AVIS")] Avis,
    #[serde(rename = "MSG")] Msg,
    #[serde(rename = "RETOUR")] Retour,
    #[serde(rename = "VIDEO")] Video,
    #[serde(rename = "RANG")] Rang,
    #[serde(rename = "IA")] Ia,
    #[serde(rename = "VERSION")] Version,
    #[serde(rename = "VEILLE")] Veille,
}

impl TypeFil {
    pub const TOUS: [TypeFil; 10] = [
        TypeFil::Essai, TypeFil::Achat, TypeFil::Avis, TypeFil::Msg, TypeFil::Retour,
        TypeFil::Video, TypeFil::Rang, TypeFil::Ia, TypeFil::Version, TypeFil::Veille,
    ];

    pub fn brut(self) -> &'static str {
        match self {
            TypeFil::Essai => "ESSAI", TypeFil::Achat => "ACHAT", TypeFil::Avis => "AVIS", TypeFil::Msg => "MSG",
            TypeFil::Retour => "RETOUR", TypeFil::Video => "VIDEO", TypeFil::Rang => "RANG", TypeFil::Ia => "IA",
            TypeFil::Version => "VERSION", TypeFil::Veille => "VEILLE",
        }
    }

    pub fn depuis(s: &str) -> Option<TypeFil> { Self::TOUS.into_iter().find(|t| t.brut() == s) }

    pub fn libelle(self) -> &'static str {
        match self {
            TypeFil::Essai => "Essai", TypeFil::Achat => "Achat", TypeFil::Avis => "Avis", TypeFil::Msg => "Message",
            TypeFil::Retour => "Retour", TypeFil::Video => "Vidéo", TypeFil::Rang => "Classement", TypeFil::Ia => "IA",
            TypeFil::Version => "Version", TypeFil::Veille => "Veille",
        }
    }

    /// Le réglage qui coupe ses notifications : `notifs.essai` à "0".
    pub fn reglage_notif(self) -> String { format!("notifs.{}", self.brut().to_lowercase()) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Plateforme { Tiktok, Instagram, Youtube, Facebook }

impl Plateforme {
    pub const TOUTES: [Plateforme; 4] = [Plateforme::Tiktok, Plateforme::Instagram, Plateforme::Youtube, Plateforme::Facebook];

    pub fn brut(self) -> &'static str {
        match self {
            Plateforme::Tiktok => "tiktok", Plateforme::Instagram => "instagram",
            Plateforme::Youtube => "youtube", Plateforme::Facebook => "facebook",
        }
    }

    pub fn depuis(s: &str) -> Option<Plateforme> { Self::TOUTES.into_iter().find(|p| p.brut() == s) }

    pub fn nom(self) -> &'static str {
        match self {
            Plateforme::Tiktok => "TikTok", Plateforme::Instagram => "Instagram",
            Plateforme::Youtube => "YouTube", Plateforme::Facebook => "Facebook",
        }
    }

    pub fn profil(self, handle: &str) -> String {
        match self {
            Plateforme::Tiktok => format!("https://www.tiktok.com/@{handle}"),
            Plateforme::Instagram => format!("https://www.instagram.com/{handle}/"),
            Plateforme::Youtube => format!("https://www.youtube.com/@{handle}"),
            Plateforme::Facebook => format!("https://www.facebook.com/{handle}"),
        }
    }

    pub fn lien(self, brut: &str, compte: &str) -> String {
        match self {
            Plateforme::Tiktok => format!("https://www.tiktok.com/@{compte}/video/{brut}"),
            Plateforme::Instagram => format!("https://www.instagram.com/reel/{brut}/"),
            Plateforme::Youtube => format!("https://www.youtube.com/shorts/{brut}"),
            Plateforme::Facebook => format!("https://www.facebook.com/{brut}"),
        }
    }

    /// L'id d'une vidéo en base : l'id brut sur TikTok (l'historique), préfixé ailleurs.
    pub fn id_video(self, brut: &str) -> String {
        if self == Plateforme::Tiktok { brut.to_string() } else { format!("{}:{brut}", self.brut()) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct Compte {
    pub business: String,
    pub plateforme: Plateforme,
    pub handle: String,
    pub actif: bool,
}

impl Compte {
    pub fn nouveau(business: &str, plateforme: Plateforme, handle: &str) -> Compte {
        Compte { business: business.into(), plateforme, handle: handle.into(), actif: true }
    }

    pub fn id(&self) -> String { Self::id_de(self.plateforme, &self.handle) }

    pub fn id_de(p: Plateforme, handle: &str) -> String { format!("{}:{handle}", p.brut()) }

    /// Le handle d'une saisie : un lien de profil, `@nom` ou `nom`. `None` si ce n'en est pas un.
    pub fn handle(saisie: &str) -> Option<String> {
        static LIEN: OnceLock<Regex> = OnceLock::new();
        static VALIDE: OnceLock<Regex> = OnceLock::new();
        let lien = LIEN.get_or_init(|| Regex::new(r"(?:tiktok|instagram|youtube|facebook)\.com/@?([A-Za-z0-9._-]+)").unwrap());
        let valide = VALIDE.get_or_init(|| Regex::new(r"^[a-z0-9._-]{1,64}$").unwrap());
        let mut h = saisie.trim().to_string();
        if let Some(m) = lien.captures(&h) { h = m[1].to_string(); }
        let h = h.replace('@', "").to_lowercase();
        valide.is_match(&h).then_some(h)
    }
}

impl<'de> Deserialize<'de> for Compte {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Brut { business: String, plateforme: Plateforme, handle: String, actif: Option<bool> }
        let b = Brut::deserialize(d)?;
        let handle = Compte::handle(&b.handle)
            .ok_or_else(|| serde::de::Error::custom(format!("« {} » n'est pas un nom de compte", b.handle)))?;
        Ok(Compte { business: b.business, plateforme: b.plateforme, handle, actif: b.actif.unwrap_or(true) })
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ReleveCompte {
    pub compte: String,
    pub at: Date,
    pub abonnes: Option<i64>,
    pub videos: Option<i64>,
    pub likes: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Video {
    pub id: String,
    pub business: String,
    pub compte: String,
    pub legende: Option<String>,
    pub publiee_le: Option<Date>,
    pub duree_s: Option<f64>,
    pub url: Option<String>,
    pub plateforme: Plateforme,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FicheVideo {
    pub id: String,
    pub business: String,
    pub compte: String,
    pub url: String,
    pub titre: String,
    pub format: Option<String>,
    pub fiche: String,
    pub verdict: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Releve {
    pub video: String,
    pub at: Date,
    pub vues: i64,
    pub likes: Option<i64>,
    pub commentaires: Option<i64>,
    pub partages: Option<i64>,
    pub enregistrements: Option<i64>,
    pub watch_moyen_s: Option<f64>,
    pub completion: Option<f64>,
    pub abonnes: Option<i64>,
}

/// Le détail d'une vidéo, en JSON dans `video_analyse` (mêmes clés que la version macOS).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Analyse {
    #[serde(rename = "watchTotalS", skip_serializing_if = "Option::is_none")]
    pub watch_total_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spectateurs: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nouveaux: Option<f64>,
    #[serde(rename = "abonnesPart", skip_serializing_if = "Option::is_none")]
    pub abonnes_part: Option<f64>,
    pub retention: Vec<f64>,
    pub sources: Vec<Part>,
    pub recherches: Vec<Part>,
    pub ages: Vec<Part>,
    pub pays: Vec<Part>,
    pub jalons: Vec<Part>,
    #[serde(rename = "vuesParJour")]
    pub vues_par_jour: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Part {
    pub nom: String,
    pub part: f64,
}

impl Analyse {
    pub fn est_vide(&self) -> bool { *self == Analyse::default() }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Appli {
    pub id: String,
    pub nom: String,
    pub editeur: Option<String>,
    pub icone: Option<String>,
    pub genre: Option<i64>,
    pub note: Option<f64>,
    pub nb_notes: Option<i64>,
    pub version: Option<String>,
    pub business: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Liste { Gratuit, Payant, Recettes }

impl Liste {
    pub const TOUTES: [Liste; 3] = [Liste::Gratuit, Liste::Payant, Liste::Recettes];
    pub fn brut(self) -> &'static str {
        match self { Liste::Gratuit => "gratuit", Liste::Payant => "payant", Liste::Recettes => "recettes" }
    }
    pub fn depuis(s: &str) -> Option<Liste> { Self::TOUTES.into_iter().find(|l| l.brut() == s) }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Avis {
    pub id: String,
    pub business: String,
    pub pays: Option<String>,
    pub note: i64,
    pub titre: Option<String>,
    pub texte: Option<String>,
    pub auteur: Option<String>,
    pub at: Date,
    pub repondu: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Ton { Alerte, #[default] Neutre }

impl Ton {
    pub fn brut(self) -> &'static str { match self { Ton::Alerte => "alerte", Ton::Neutre => "neutre" } }
}

/// Une tâche qu'une source propose ; `cle` est stable d'un passage à l'autre.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tache {
    pub cle: String,
    pub texte: String,
    pub etiquette: Option<String>,
    pub ton: Ton,
    pub echeance: Option<String>,
    pub lien: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FilSAV {
    pub business: String,
    pub uid: String,
    pub nom: Option<String>,
    pub email: Option<String>,
    #[serde(rename = "dernierAt")]
    pub dernier_at: Date,
    #[serde(rename = "dernierEquipe")]
    pub dernier_equipe: bool,
    pub apercu: String,
    pub ouvert: bool,
    #[serde(rename = "nonVu")]
    pub non_vu: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EtatFil { ARepondre, PasLu, Lu, Traite }

impl FilSAV {
    pub fn cle(&self) -> CleFil { CleFil { business: self.business.clone(), uid: self.uid.clone() } }
    pub fn etat(&self) -> EtatFil {
        if self.ouvert { EtatFil::ARepondre } else if self.non_vu { EtatFil::PasLu } else if self.dernier_equipe { EtatFil::Lu } else { EtatFil::Traite }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CleFil {
    pub business: String,
    pub uid: String,
}

impl CleFil {
    pub fn id(&self) -> String { format!("{}:{}", self.business, self.uid) }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MessageSAV {
    pub id: String,
    pub equipe: bool,
    pub texte: String,
    pub at: Date,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RetourSAV {
    pub business: String,
    pub id: String,
    pub uid: Option<String>,
    pub email: Option<String>,
    pub source: Option<String>,
    pub categorie: Option<String>,
    pub etoiles: Option<i64>,
    pub message: String,
    pub citation: Option<String>,
    pub conversation: Option<String>,
    pub traite: bool,
    #[serde(rename = "traiteLe")]
    pub traite_le: Option<Date>,
    pub plateforme: Option<String>,
    pub build: Option<String>,
    pub at: Date,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ReleveSAV {
    pub fils: Vec<FilSAV>,
    pub retours: Vec<RetourSAV>,
    pub messages: BTreeMap<String, Vec<MessageSAV>>,
    pub emails: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct LignePlan {
    pub section: String,
    pub texte: String,
    pub coche: bool,
    pub fait: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Install {
    pub jour: String,
    pub loin: i64,
    pub paywall: bool,
    pub essai: bool,
    pub direct: bool,
    pub commence: bool,
    pub rond: bool,
    pub j1: bool,
    pub j7: bool,
    pub j14: bool,
    pub j30: bool,
    pub client: Option<String>,
    /// La personne PostHog, pour joindre ses événements (table `evenement`).
    pub personne: String,
    pub commences: i64,
    pub ronds: i64,
    /// Jours de Paris avec au moins un rond fini.
    pub jours_ronds: i64,
    /// Secondes de l'install au premier rond.
    pub premier_rond: Option<i64>,
    /// Jour de Paris du dernier rond fini.
    pub dernier_rond: Option<String>,
    pub valeurs: BTreeMap<crate::dimension::Dimension, Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EtatPaiement { Paye, Essai, Non, Inconnu }

impl EtatPaiement {
    pub fn brut(self) -> &'static str {
        match self { EtatPaiement::Paye => "paye", EtatPaiement::Essai => "essai", EtatPaiement::Non => "non", EtatPaiement::Inconnu => "inconnu" }
    }
    pub fn depuis(s: &str) -> Option<EtatPaiement> {
        [EtatPaiement::Paye, EtatPaiement::Essai, EtatPaiement::Non, EtatPaiement::Inconnu].into_iter().find(|e| e.brut() == s)
    }
}

/// Un événement de l'app gardé tel quel (table `evenement`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EvenementApp {
    pub uuid: String,
    pub personne: String,
    /// Instant de l'événement, ISO UTC.
    pub at: String,
    /// Jour de Paris.
    pub jour: String,
    pub nom: String,
    pub plateforme: String,
    pub build: String,
    /// Les propriétés hors `$…`, en objet JSON.
    pub props: String,
    /// Arrivée chez PostHog, en millisecondes Unix : le curseur de la collecte.
    pub recu_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct Porte {
    pub jour: String,
    pub porte: String,
    pub vues: i64,
    pub essais: i64,
    pub directs: i64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct Cohorte {
    pub jour: String,
    pub installs: i64,
    pub j1: Option<i64>,
    pub j7: Option<i64>,
    pub j14: Option<i64>,
    pub j30: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EtatPassage {
    #[serde(rename = "ok")] Ok,
    #[serde(rename = "erreur")] Erreur,
    #[serde(rename = "non_configure")] NonConfigure,
}

impl EtatPassage {
    pub fn brut(self) -> &'static str {
        match self { EtatPassage::Ok => "ok", EtatPassage::Erreur => "erreur", EtatPassage::NonConfigure => "non_configure" }
    }
    pub fn depuis(s: &str) -> EtatPassage {
        match s { "ok" => EtatPassage::Ok, "non_configure" => EtatPassage::NonConfigure, _ => EtatPassage::Erreur }
    }
}

/// Un passage d'une source, journalisé dans `collecte`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Passage {
    pub source: String,
    pub debut: Date,
    pub fin: Date,
    pub etat: EtatPassage,
    pub erreur: Option<String>,
    pub lignes: i64,
}

impl Passage {
    pub fn depuis_ligne(l: &crate::db::Ligne) -> Passage {
        Passage {
            source: l.texte("source").unwrap_or_default(),
            debut: l.date("debut").unwrap_or_else(crate::jour::loin_passe),
            fin: l.date("fin").unwrap_or_else(crate::jour::loin_passe),
            etat: EtatPassage::depuis(&l.texte("etat").unwrap_or_default()),
            erreur: l.texte("erreur"),
            lignes: l.entier("lignes").unwrap_or(0),
        }
    }
}

/// Ids des tâches du SAV, partagés par la source Firebase et le store.
pub mod ids_sav {
    pub const SOURCE: &str = "firebase";
    pub fn prefixe_tache_fil(uid: &str) -> String { format!("{SOURCE}:msg:{uid}:") }
    pub fn tache_retour(id: &str) -> String { format!("{SOURCE}:retour:{id}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles() {
        assert_eq!(Compte::handle(" https://www.tiktok.com/@HolaEsTony?lang=fr ").as_deref(), Some("holaestony"));
        assert_eq!(Compte::handle("@roleo.app").as_deref(), Some("roleo.app"));
        assert_eq!(Compte::handle("pas un nom"), None);
        assert_eq!(Plateforme::Youtube.id_video("abc"), "youtube:abc");
        assert_eq!(Plateforme::Tiktok.id_video("123"), "123");
    }

    #[test]
    fn analyse_json() {
        let a = Analyse { watch_total_s: Some(3.5), retention: vec![1.0, 0.5], ..Default::default() };
        let s = serde_json::to_string(&a).unwrap();
        assert!(s.contains("\"watchTotalS\":3.5"));
        assert!(!s.contains("spectateurs"));
        let b: Analyse = serde_json::from_str(&s).unwrap();
        assert_eq!(a, b);
        assert!(Analyse::default().est_vide());
    }
}
