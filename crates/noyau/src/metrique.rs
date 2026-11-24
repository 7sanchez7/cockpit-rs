//! Les noms des métriques de `serie` (docs/donnees.md) et les deux boutiques.
use crate::erreur::Res;
use crate::store::Store;

pub const ACTIFS: &str = "actifs";
pub const ACTIFS_J7: &str = "actifs_j7";
pub const ACTIFS_HEURE: &str = "actifs_h";
pub const INSTALLS: &str = "installs";
pub const INSTALLS_J7: &str = "installs_j7";
pub const ESSAIS: &str = "essais";
pub const ESSAIS_J7: &str = "essais_j7";
pub const RONDS: &str = "ronds";
pub const RONDS_J7: &str = "ronds_j7";
pub const RONDS_COMMENCES: &str = "ronds_commences";
pub const RONDS_COMMENCES_J7: &str = "ronds_commences_j7";
// RevenueCat
pub const ACHATS: &str = "achats";
pub const MRR_BRUT: &str = "mrr_brut";
pub const MRR_NET: &str = "mrr_net";
pub const ABONNES: &str = "abonnes";
pub const ESSAIS_ACTIFS: &str = "essais_actifs";
pub const REVENU_28J: &str = "revenu_28j";
pub const REVENU: &str = "revenu";
pub const NOUVEAUX_CLIENTS: &str = "nouveaux_clients";
// App Store Connect et Play Console
pub const TELECHARGEMENTS: &str = "telechargements";
// Firebase
pub const COUT_IA: &str = "cout_ia";
pub const COUT_TEXTE: &str = "cout_texte";
pub const COUT_VOIX: &str = "cout_voix";
pub const TOKENS_IA: &str = "tokens_ia";
pub const ECHECS_IA: &str = "echecs_ia";
pub const MESSAGES_OUVERTS: &str = "messages_ouverts";
pub const RETOURS_NOUVEAUX: &str = "retours_nouveaux";
// TikTok
pub const VUES_TIKTOK: &str = "vues_tiktok";
pub const VUES_TIKTOK_J7: &str = "vues_tiktok_j7";
// Classements
pub const NOTE: &str = "note";
pub const NB_NOTES: &str = "nb_notes";

pub fn requetes_ia(fournisseur: &str) -> String { format!("req_ia.{fournisseur}") }
pub fn q429_ia(fournisseur: &str) -> String { format!("q429_ia.{fournisseur}") }
pub fn erreurs_ia(fournisseur: &str) -> String { format!("err_ia.{fournisseur}") }
pub fn latence_ia(fournisseur: &str) -> String { format!("ms_ia.{fournisseur}") }

/// Ce qui reste d'un revenu brut après TVA et commission des stores.
pub const PART_NETTE: f64 = 0.72;
pub const EUR_PAR_USD: f64 = 0.92;

pub fn en_euros(usd: f64) -> f64 { usd * EUR_PAR_USD }

/// Les métriques relevées par boutique s'écrivent `base.app_store` et `base.play_store`, la base est le total.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Boutique {
    #[serde(rename = "app_store")] AppStore,
    #[serde(rename = "play_store")] PlayStore,
}

impl Boutique {
    pub const TOUTES: [Boutique; 2] = [Boutique::AppStore, Boutique::PlayStore];

    pub fn brut(self) -> &'static str { match self { Boutique::AppStore => "app_store", Boutique::PlayStore => "play_store" } }
    pub fn depuis(s: &str) -> Option<Boutique> { Self::TOUTES.into_iter().find(|b| b.brut() == s) }
    pub fn nom(self) -> &'static str { match self { Boutique::AppStore => "App Store", Boutique::PlayStore => "Play Store" } }
    pub fn metrique(self, base: &str) -> String { format!("{base}.{}", self.brut()) }

    /// Écrit les téléchargements d'un store, puis leur total.
    pub fn telechargements(self, store: &Store, business: &str, jour: &str, n: f64) -> Res<()> {
        store.serie(business, &self.metrique(TELECHARGEMENTS), jour, -1, n)?;
        let parts: Vec<String> = Self::TOUTES.iter().map(|b| b.metrique(TELECHARGEMENTS)).collect();
        store.somme(business, TELECHARGEMENTS, &parts, jour)
    }

    pub const SEPAREES: [&'static str; 20] = [
        ACTIFS, ACTIFS_J7, ACTIFS_HEURE, INSTALLS, INSTALLS_J7, ESSAIS, ESSAIS_J7, RONDS, RONDS_J7, RONDS_COMMENCES,
        RONDS_COMMENCES_J7,
        ACHATS, MRR_BRUT, MRR_NET, ABONNES, ESSAIS_ACTIFS, REVENU_28J, REVENU, NOUVEAUX_CLIENTS, TELECHARGEMENTS,
    ];

    pub fn separee(m: &str) -> bool { Self::SEPAREES.contains(&m) }
}
