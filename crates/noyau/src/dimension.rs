//! Les dimensions d'un install : ce que l'onboarding a appris de la personne.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dimension {
    Loin, Langue, Maternelle, Source, Age, Motivation, Blocage, Interet, Niveau, Tuteur, Notifs, Compte, Pays,
    Version, Build, Produit, Zone,
}

use Dimension::*;

impl Dimension {
    pub const TOUTES: [Dimension; 17] = [
        Loin, Langue, Maternelle, Source, Age, Motivation, Blocage, Interet, Niveau, Tuteur, Notifs, Compte, Pays,
        Version, Build, Produit, Zone,
    ];

    /// Celles qu'on lit dans les événements ; `zone` se déduit du pays.
    pub fn lues() -> impl Iterator<Item = Dimension> { Self::TOUTES.into_iter().filter(|d| *d != Zone) }

    pub const PAS_MESURE: &'static str = "?";

    pub fn brut(self) -> &'static str {
        match self {
            Loin => "loin", Langue => "langue", Maternelle => "maternelle", Source => "source", Age => "age",
            Motivation => "motivation", Blocage => "blocage", Interet => "interet", Niveau => "niveau",
            Tuteur => "tuteur", Notifs => "notifs", Compte => "compte", Pays => "pays", Version => "version",
            Build => "build", Produit => "produit", Zone => "zone",
        }
    }

    pub fn depuis(s: &str) -> Option<Dimension> { Self::TOUTES.into_iter().find(|d| d.brut() == s) }

    pub fn nom(self) -> &'static str {
        match self {
            Loin => "Écran atteint", Langue => "Langue apprise", Maternelle => "Langue maternelle", Source => "Connu par",
            Age => "Âge", Motivation => "Motivation", Blocage => "Ce qui bloque", Interet => "Centres d'intérêt",
            Niveau => "Niveau de départ", Tuteur => "Tuteur choisi", Notifs => "Notifications", Compte => "Compte lié",
            Pays => "Pays", Version => "Version de l'app", Build => "Build", Produit => "Offre prise", Zone => "Zone",
        }
    }

    /// Le premier jour où l'app envoie cette dimension ; avant, elle vaut `?`.
    pub fn mesuree_depuis(self) -> Option<&'static str> {
        match self {
            Tuteur => Some("2026-09-02"),
            Motivation => Some("2026-09-16"),
            Interet | Maternelle | Niveau => Some("2026-10-13"),
            Age => Some("2026-10-24"),
            Notifs => Some("2026-11-09"),
            _ => None,
        }
    }

    /// La zone d'un code pays ISO (régions ONU), l'outre-mer français à part.
    pub fn zone(pays: &str) -> Option<&'static str> {
        const OUTRE_MER: [&str; 11] = ["RE", "YT", "GP", "MQ", "GF", "PF", "NC", "PM", "BL", "MF", "WF"];
        if OUTRE_MER.contains(&pays) { return Some("outre_mer"); }
        ZONES.iter().find(|(_, codes)| codes.split(' ').any(|c| c == pays)).map(|(z, _)| *z)
    }
}

const ZONES: [(&str, &str); 6] = [
    ("afrique", "DZ EG EH LY MA SD SS TN BF BJ CI CV GH GM GN GW LR ML MR NE NG SH SL SN TG AO CD CF CG CM GA GQ ST TD \
                 BI DJ ER ET IO KE KM MG MU MW MZ RW SC SO TF TZ UG ZM ZW YT RE BW LS NA SZ ZA"),
    ("europe", "AX DK EE FI FO GB GG IE IM IS JE LT LV NO SE SJ AD AL BA ES GI GR HR IT ME MK MT PT RS SI SM VA XK \
                AT BE CH DE FR LI LU MC NL BG BY CZ HU MD PL RO RU SK UA"),
    ("asie", "AF BD BT IN IR LK MV NP PK CN HK JP KP KR MN MO TW BN ID KH LA MM MY PH SG TH TL VN \
              KG KZ TJ TM UZ AE AM AZ BH CY GE IL IQ JO KW LB OM PS QA SA SY TR YE"),
    ("oceanie", "AU NZ NF CC CX HM FJ NC PG SB VU FM GU KI MH MP NR PW UM AS CK NU PF PN TK TO TV WF WS"),
    ("amerique_nord", "BM CA GL PM US"),
    ("amerique_latine", "AG AI AW BB BL BQ BS CU CW DM DO GD GP HT JM KN KY LC MF MQ MS PR SX TC TT VC VG VI \
                         BZ CR GT HN MX NI PA SV AR BO BR BV CL CO EC FK GF GS GY PE PY SR UY VE"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zones() {
        assert_eq!(Dimension::zone("FR"), Some("europe"));
        assert_eq!(Dimension::zone("RE"), Some("outre_mer"));
        assert_eq!(Dimension::zone("US"), Some("amerique_nord"));
        assert_eq!(Dimension::zone("MX"), Some("amerique_latine"));
        assert_eq!(Dimension::zone("MA"), Some("afrique"));
        assert_eq!(Dimension::zone("JP"), Some("asie"));
        assert_eq!(Dimension::zone("AU"), Some("oceanie"));
        assert_eq!(Dimension::zone("AQ"), None);
        assert_eq!(Dimension::lues().count(), 16);
    }
}
