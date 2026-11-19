//! Ce qui a fait percer une vidéo, mesuré contre les autres vidéos de son compte (docs/ecrans.md § Vidéos).
//!
//! Pour chaque mesure rapportée aux vues, la corrélation de rang (Spearman) entre la mesure et les vues, sur les
//! vidéos du compte, dit si elle porte les vues de ce compte. Le levier d'une vidéo est la mesure qui maximise
//! (rang de la vidéo dans le compte − 0,5) × corrélation : la plus au-dessus de ses autres vidéos, parmi celles qui comptent.
use super::{format, LigneVideo};
use serde::Serialize;
use std::collections::HashMap;

/// Sous ce nombre de vidéos qui ont la mesure, ni rang ni corrélation ne veulent rien dire.
pub const MINIMUM: usize = 8;
/// La seconde de la courbe de rétention qui dit l'accroche.
pub const ACCROCHE_S: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Mesure { Accroche, Visionnage, Completion, Likes, Commentaires, Partages, Enregistrements, Abonnes }

impl Mesure {
    pub const TOUTES: [Mesure; 8] = [
        Mesure::Accroche, Mesure::Visionnage, Mesure::Completion, Mesure::Likes, Mesure::Commentaires,
        Mesure::Partages, Mesure::Enregistrements, Mesure::Abonnes,
    ];

    pub fn nom(self) -> &'static str {
        match self {
            Mesure::Accroche => "Accroche",
            Mesure::Visionnage => "Visionnage",
            Mesure::Completion => "Complétion",
            Mesure::Likes => "Likes",
            Mesure::Commentaires => "Commentaires",
            Mesure::Partages => "Partages",
            Mesure::Enregistrements => "Enregistrements",
            Mesure::Abonnes => "Abonnés",
        }
    }

    /// La valeur d'une vidéo ; l'accroche vient de sa courbe de rétention.
    pub fn valeur(self, v: &LigneVideo, accroche: Option<f64>) -> Option<f64> {
        let taux = |n: Option<i64>| n.and_then(|n| (v.vues > 0).then(|| n as f64 / v.vues as f64));
        match self {
            Mesure::Accroche => accroche,
            Mesure::Visionnage => v.watch_moyen_s.and_then(|w| v.duree_s.and_then(|d| (d > 0.0).then(|| w / d))),
            Mesure::Completion => v.completion,
            Mesure::Likes => taux(v.likes),
            Mesure::Commentaires => taux(v.commentaires),
            Mesure::Partages => taux(v.partages),
            Mesure::Enregistrements => taux(v.enregistrements),
            Mesure::Abonnes => v.conversion(),
        }
    }

    /// « 0,09 % », « 3,8 ».
    pub fn court(self, x: f64) -> String {
        if self == Mesure::Abonnes { return format::decimal(x, 1); }
        format::pourcent(x, if x < 0.01 { 2 } else if x < 0.1 { 1 } else { 0 })
    }

    /// « 58 % encore là à 3 s ».
    pub fn texte(self, x: f64) -> String {
        let unite = match self {
            Mesure::Accroche => format!("encore là à {ACCROCHE_S} s"),
            Mesure::Visionnage => "de la vidéo regardée en moyenne".into(),
            Mesure::Completion => "vont jusqu'au bout".into(),
            Mesure::Likes => "des vues likent".into(),
            Mesure::Commentaires => "des vues commentent".into(),
            Mesure::Partages => "des vues partagent".into(),
            Mesure::Enregistrements => "des vues enregistrent".into(),
            Mesure::Abonnes => "abonnés pour 1 000 vues".into(),
        };
        format!("{} {unite}", self.court(x))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Levier {
    pub mesure: Mesure,
    pub valeur: f64,
    pub mediane: f64,
    /// Part des autres vidéos du compte sous cette valeur, de 0 à 1.
    pub rang: f64,
    /// Corrélation de rang entre la mesure et les vues du compte.
    pub poids: f64,
    /// Plus de vues que la médiane du compte.
    pub percee: bool,
}

/// Le levier de `v` parmi les vidéos de son compte ; None si le compte a trop peu de vidéos ou si `v` ne
/// dépasse sa médiane sur aucune mesure liée aux vues.
pub fn levier(v: &LigneVideo, videos: &[LigneVideo], accroches: &HashMap<String, f64>) -> Option<Levier> {
    let compte = compte(v, videos);
    if compte.len() < MINIMUM || !compte.iter().any(|w| w.id == v.id) { return None; }
    let vues_mediane = mediane(&compte.iter().map(|w| w.vues as f64).collect::<Vec<_>>());
    let mut meilleur: Option<Levier> = None;
    for m in Mesure::TOUTES {
        let paires: Vec<(f64, f64)> = compte.iter()
            .filter_map(|w| m.valeur(w, accroches.get(&w.id).copied()).map(|x| (w.vues as f64, x)))
            .collect();
        if paires.len() < MINIMUM { continue; }
        let Some(x) = m.valeur(v, accroches.get(&v.id).copied()) else { continue };
        let sous = paires.iter().filter(|p| p.1 < x).count() as f64;
        let egaux = paires.iter().filter(|p| p.1 == x).count() as f64 - 1.0;
        let rang = (sous + egaux / 2.0) / (paires.len() - 1) as f64;
        let poids = spearman(&paires);
        if poids <= 0.0 || rang <= 0.5 { continue; }
        let l = Levier { mesure: m, valeur: x, mediane: mediane(&paires.iter().map(|p| p.1).collect::<Vec<_>>()), rang, poids,
                         percee: v.vues as f64 > vues_mediane };
        // le premier des ex aequo gagne
        if meilleur.is_none_or(|b| (b.rang - 0.5) * b.poids < (l.rang - 0.5) * l.poids) { meilleur = Some(l); }
    }
    meilleur
}

/// Les vidéos vues du compte de `v`, la base de comparaison.
pub fn compte<'a>(v: &LigneVideo, videos: &'a [LigneVideo]) -> Vec<&'a LigneVideo> {
    videos.iter().filter(|w| w.plateforme == v.plateforme && w.compte == v.compte && w.vues > 0).collect()
}

/// Corrélation de rang de Spearman, de −1 à 1 ; les ex aequo prennent leur rang moyen.
pub fn spearman(paires: &[(f64, f64)]) -> f64 {
    let a = rangs(&paires.iter().map(|p| p.0).collect::<Vec<_>>());
    let b = rangs(&paires.iter().map(|p| p.1).collect::<Vec<_>>());
    let n = paires.len() as f64;
    let (ma, mb) = (a.iter().sum::<f64>() / n, b.iter().sum::<f64>() / n);
    let cov: f64 = a.iter().zip(&b).map(|(x, y)| (x - ma) * (y - mb)).sum();
    let va: f64 = a.iter().map(|x| (x - ma) * (x - ma)).sum();
    let vb: f64 = b.iter().map(|y| (y - mb) * (y - mb)).sum();
    if va > 0.0 && vb > 0.0 { cov / (va * vb).sqrt() } else { 0.0 }
}

fn rangs(x: &[f64]) -> Vec<f64> {
    let mut ordre: Vec<usize> = (0..x.len()).collect();
    ordre.sort_by(|&a, &b| x[a].total_cmp(&x[b]));
    let mut r = vec![0.0; x.len()];
    let mut i = 0;
    while i < ordre.len() {
        let mut j = i;
        while j + 1 < ordre.len() && x[ordre[j + 1]] == x[ordre[i]] { j += 1; }
        for &k in &ordre[i..=j] { r[k] = (i + j) as f64 / 2.0; }
        i = j + 1;
    }
    r
}

pub fn mediane(x: &[f64]) -> f64 {
    let mut s = x.to_vec();
    s.sort_by(f64::total_cmp);
    let n = s.len();
    if n == 0 { return 0.0; }
    if n % 2 == 1 { s[n / 2] } else { (s[n / 2 - 1] + s[n / 2]) / 2.0 }
}
