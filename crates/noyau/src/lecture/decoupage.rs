//! Les réponses du tunnel Roleo dites en français, et les pas de l'arbre des installs.
use super::data::Filtre;
use super::{format, noms};
use crate::dimension::Dimension;

/// Le premier jour d'install où le rond fini et les retours se mesurent.
pub const RONDS_DEPUIS: &str = "2026-10-15";

/// Les écrans du tunnel, dans l'ordre : `loin` est le rang du dernier passé, de 1 à 17.
pub const ECRANS: [(&str, &str); 17] = [
    ("welcome", "Accueil"), ("language", "Langue apprise"), ("native_language", "Langue maternelle"),
    ("acquisition_source", "Connu comment"), ("first_name", "Prénom"), ("age", "Âge"),
    ("motivation", "Motivation"), ("pain", "Ce qui bloque"), ("interests", "Centres d'intérêt"),
    ("level", "Niveau"), ("social_proof", "Preuve sociale"), ("app_tour", "Présentation"),
    ("projection", "Projection"), ("account_setup", "Compte"), ("persona", "Choix du tuteur"),
    ("paywall_viewed", "Paywall vu"), ("paywall", "Essai touché"),
];

/// Le nom de l'écran de rang `rang` (1 à 17).
pub fn ecran(rang: i64) -> Option<&'static str> {
    usize::try_from(rang - 1).ok().and_then(|i| ECRANS.get(i)).map(|e| e.1)
}

/// Une valeur brute dite en français ; une valeur inconnue s'affiche telle quelle.
pub fn libelle(d: Dimension, valeur: &str) -> String {
    if valeur == Dimension::PAS_MESURE {
        return format!("Pas mesuré (avant le {})", format::jour_court_cle(d.mesuree_depuis().unwrap_or("")));
    }
    if valeur.is_empty() {
        return match d {
            Dimension::Compte => "Pas lié",
            Dimension::Produit => "Ni essai ni achat",
            Dimension::Pays => "Inconnu",
            Dimension::Zone | Dimension::Plateforme => "Inconnue",
            _ => "Sans réponse",
        }
        .into();
    }
    match d {
        Dimension::Langue | Dimension::Maternelle => noms::langue(valeur).map(format::majuscule).unwrap_or_else(|| valeur.into()),
        Dimension::Pays => noms::pays(valeur).map(str::to_string).unwrap_or_else(|| valeur.into()),
        Dimension::Loin => ecran(valeur.parse().unwrap_or(0)).unwrap_or("Aucun écran").into(),
        Dimension::Niveau => valeur.to_uppercase(),
        Dimension::Tuteur => format::majuscule(valeur),
        Dimension::Plateforme => valeur.into(),
        Dimension::Age => match valeur.strip_suffix("_plus") {
            Some(x) => format!("{x} ans et plus"),
            None => format!("{} ans", valeur.replace('_', "-")),
        },
        _ => LIBELLES.iter().find(|(v, _)| *v == valeur).map(|(_, l)| l.to_string())
            .unwrap_or_else(|| format::majuscule(&valeur.replace('_', " "))),
    }
}

/// Un install y donne plusieurs réponses : ses valeurs se recoupent.
pub fn multiple(d: Dimension) -> bool { matches!(d, Dimension::Interet | Dimension::Blocage) }

impl Filtre {
    /// Le pas tel qu'il s'affiche dans le chemin de l'arbre.
    pub fn libelle(&self) -> String {
        match self {
            Filtre::Valeur(d, v) => format!("{} : {}", d.nom(), libelle(*d, v)),
            Filtre::Sauf(d, v) => format!("{} : sauf {}", d.nom(), libelle(*d, v)),
            Filtre::Ecran(r) => format!("A passé : {}", ecran(*r).unwrap_or("")),
            Filtre::Paywall => "A vu le paywall".into(),
            Filtre::Essai => "A pris l'essai".into(),
            Filtre::Payant => "Payant".into(),
        }
    }
}

const LIBELLES: [(&str, &str); 68] = [
    // source
    ("tiktok", "TikTok"), ("instagram", "Instagram"), ("youtube", "YouTube"), ("facebook", "Facebook"),
    ("app_store", "App Store"), ("friends", "Amis"), ("chatgpt", "ChatGPT"), ("google", "Google"), ("other", "Autre"),
    // motivation
    ("mix", "Un peu de tout"), ("living_there", "Vivre là-bas"), ("studies", "Études"), ("personal_growth", "Pour soi"),
    ("travel", "Voyager"), ("work", "Travail"),
    // blocage
    ("understand_but_stuck", "Comprend mais bloque"), ("words_not_sentences", "Des mots, pas des phrases"),
    ("too_fast", "Ça va trop vite"), ("afraid_of_mistakes", "Peur de se tromper"), ("none", "Rien"), ("accent", "L'accent"),
    // intérêts
    ("sports", "Sport"), ("trips", "Voyages"), ("music", "Musique"), ("social", "Réseaux sociaux"), ("series", "Séries"),
    ("nightlife", "Sorties"), ("food", "Cuisine"), ("relationships", "Relations"), ("dating", "Rencontres"),
    ("fashion", "Mode"), ("beauty", "Beauté"), ("animals", "Animaux"), ("cars", "Voitures"), ("books", "Livres"),
    ("history", "Histoire"), ("games", "Jeux vidéo"), ("dance", "Danse"), ("parenting", "Parentalité"),
    ("hiking", "Randonnée"), ("ai", "IA"), ("popculture", "Pop culture"), ("cycling", "Vélo"), ("art", "Art"),
    ("photo", "Photo"), ("anime", "Animés"), ("films", "Films"), ("tech", "Tech"), ("fitness", "Fitness"),
    ("podcasts", "Podcasts"), ("business", "Business"), ("finance", "Finance"), ("startup", "Startups"),
    ("shopping", "Shopping"),
    // motivation, suite
    ("culture", "Culture"), ("loved_one", "Un proche"),
    // notifications, compte, offre
    ("granted", "Acceptées"), ("denied", "Refusées"), ("apple", "Apple"), ("Yearly", "Annuel"), ("monthly", "Mensuel"),
    // zone
    ("europe", "Europe"), ("afrique", "Afrique"), ("amerique_nord", "Amérique du Nord"),
    ("amerique_latine", "Amérique latine et Caraïbes"), ("asie", "Asie et Moyen-Orient"), ("oceanie", "Océanie"),
    ("outre_mer", "Outre-mer français"),
];
