//! Les ronds dans le détail : ce que font les installs d'une période après le tunnel (table `install`), et ce que
//! disent les événements du rond, du bilan et du retour, tous users confondus (table `evenement`, noms de Roleo).
use super::data::{clauses, Filtre};
use super::percee::mediane;
use super::Lectures;
use crate::db::{Ligne, Valeur};
use crate::erreur::Res;
use crate::jour;
use crate::p;
use crate::sources::posthog::CLE_EVENEMENTS_DEPUIS;
use std::collections::HashMap;

/// Des chiffres lus jour par jour, qui s'additionnent sur une fenêtre.
pub trait ParJour: Default + Clone {
    fn jour(&self) -> &str;
    fn somme(jours: &[Self]) -> Self;
}

/// Les installs d'un jour et leurs ronds ; avant `decoupage::RONDS_DEPUIS`, les ronds ne se mesuraient pas.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PremiersRonds {
    pub jour: String,
    pub installs: i64,
    /// Ceux qui ont commencé un rond, ceux qui en ont fini un.
    pub commence: i64,
    pub fini: i64,
    /// Ronds finis et jours avec un rond fini, tous installs confondus.
    pub ronds: i64,
    pub jours: i64,
    /// Secondes de l'install au premier rond, une par install qui en a commencé un.
    pub delais: Vec<f64>,
}

impl ParJour for PremiersRonds {
    fn jour(&self) -> &str { &self.jour }

    fn somme(jours: &[PremiersRonds]) -> PremiersRonds {
        let mut t = PremiersRonds::default();
        for j in jours {
            t.installs += j.installs;
            t.commence += j.commence;
            t.fini += j.fini;
            t.ronds += j.ronds;
            t.jours += j.jours;
            t.delais.extend(&j.delais);
        }
        t
    }
}

impl PremiersRonds {
    pub fn ronds_par_install(&self) -> Option<f64> { taux(self.ronds, self.installs) }
    /// Jours avec un rond, par install qui en a fini un.
    pub fn jours_par_fidele(&self) -> Option<f64> { taux(self.jours, self.fini) }
    /// Le délai médian de l'install au premier rond, en secondes.
    pub fn delai(&self) -> Option<f64> { (!self.delais.is_empty()).then(|| mediane(&self.delais)) }
}

/// Les événements d'un jour : le rond, le bilan, le retour.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct JourRonds {
    pub jour: String,
    pub commences: i64,
    pub finis: i64,
    /// Sortis après au moins une réponse : depuis le build 49, l'app envoie aussi `path_round_left` à l'ouverture de chaque rond.
    pub quittes: i64,
    /// Sur les ronds finis : réponses, justes, réponses à la voix.
    pub reponses: i64,
    pub justes: i64,
    pub voix: i64,
    /// Formes du jour dites, sur les formes du jour, des ronds finis qui portent les deux.
    pub dites: i64,
    pub formes: i64,
    /// Durée des ronds finis, en secondes.
    pub durees: Vec<f64>,
    /// Temps de réponse du tuteur, en millisecondes.
    pub tuteur: Vec<f64>,
    pub bilans_fermes: i64,
    /// Bilans fermés au bout (`how = finish`).
    pub bilans_finis: i64,
    /// Questions du quiz du bilan répondues, ratées.
    pub quiz: i64,
    pub quiz_rates: i64,
    pub ouvertures: i64,
    pub notifs: i64,
    pub avis: i64,
    pub avis_oui: i64,
}

impl ParJour for JourRonds {
    fn jour(&self) -> &str { &self.jour }

    fn somme(jours: &[JourRonds]) -> JourRonds {
        let mut t = JourRonds::default();
        for j in jours {
            t.commences += j.commences;
            t.finis += j.finis;
            t.quittes += j.quittes;
            t.reponses += j.reponses;
            t.justes += j.justes;
            t.voix += j.voix;
            t.dites += j.dites;
            t.formes += j.formes;
            t.durees.extend(&j.durees);
            t.tuteur.extend(&j.tuteur);
            t.bilans_fermes += j.bilans_fermes;
            t.bilans_finis += j.bilans_finis;
            t.quiz += j.quiz;
            t.quiz_rates += j.quiz_rates;
            t.ouvertures += j.ouvertures;
            t.notifs += j.notifs;
            t.avis += j.avis;
            t.avis_oui += j.avis_oui;
        }
        t
    }
}

impl JourRonds {
    pub fn quittes(&self) -> Option<f64> { taux(self.quittes, self.commences) }
    pub fn dites(&self) -> Option<f64> { taux(self.dites, self.formes) }
    pub fn justes(&self) -> Option<f64> { taux(self.justes, self.reponses) }
    pub fn voix(&self) -> Option<f64> { taux(self.voix, self.reponses) }
    pub fn duree(&self) -> Option<f64> { (!self.durees.is_empty()).then(|| mediane(&self.durees)) }
    /// En secondes.
    pub fn tuteur(&self) -> Option<f64> { (!self.tuteur.is_empty()).then(|| mediane(&self.tuteur) / 1000.0) }
    pub fn bilan_fini(&self) -> Option<f64> { taux(self.bilans_finis, self.bilans_fermes) }
    pub fn quiz_juste(&self) -> Option<f64> { taux(self.quiz - self.quiz_rates, self.quiz) }
    pub fn par_notif(&self) -> Option<f64> { taux(self.notifs, self.ouvertures) }
    pub fn avis_oui(&self) -> Option<f64> { taux(self.avis_oui, self.avis) }
}

fn taux(n: i64, sur: i64) -> Option<f64> { (sur > 0).then(|| n as f64 / sur as f64) }

/// Les événements que lit `ronds_par_jour`.
const NOMS: [&str; 8] = [
    "path_round_started", "path_round_done", "path_round_left", "path_reply", "path_bilan_closed", "app_opened",
    "notif_opened", "rating_sheet_answered",
];

/// Les jours de `debut` à `fin`, compris.
fn jours_de(debut: &str, fin: &str) -> Vec<String> {
    let mut v = vec![];
    let mut j = debut.to_string();
    while j.as_str() <= fin {
        v.push(j.clone());
        j = jour::decale(&j, 1);
    }
    v
}

impl Lectures<'_> {
    /// Les installs de `debut` à `fin` qui passent les filtres et leurs ronds, jour d'install par jour, un jour sans
    /// install compris.
    pub fn premiers_ronds(&self, filtres: &[Filtre], debut: &str, fin: &str) -> Res<Vec<PremiersRonds>> {
        let (clauses, valeurs) = clauses(filtres);
        let depuis = format!("
            from install i left join paiement p on p.business = i.business and p.client = i.client
            where i.business in {} and i.jour between ? and ?{clauses}", self.dans());
        let params = || self.autour(p![], [p![debut, fin], valeurs.clone()].concat());
        let sql = format!("
            select i.jour as jour, count(*) as installs, sum(i.commence) as commence, sum(i.rond) as fini,
                sum(i.ronds) as ronds, sum(i.jours_ronds) as jours
            {depuis} group by i.jour");
        let mut par_jour: HashMap<String, PremiersRonds> = self.q(&sql, params())?.iter().map(|l| {
            let n = |c: &str| l.entier(c).unwrap_or(0);
            let jour = l.texte("jour").unwrap_or_default();
            (jour.clone(), PremiersRonds {
                jour, installs: n("installs"), commence: n("commence"), fini: n("fini"), ronds: n("ronds"),
                jours: n("jours"), delais: vec![],
            })
        }).collect();
        let sql = format!("select i.jour as jour, i.premier_rond as s {depuis} and i.premier_rond is not null");
        for l in self.q(&sql, params())? {
            if let (Some(j), Some(s)) = (l.texte("jour"), l.nombre("s")) {
                par_jour.entry(j).or_default().delais.push(s);
            }
        }
        Ok(jours_de(debut, fin).into_iter()
            .map(|j| par_jour.remove(&j).map(|p| PremiersRonds { jour: j.clone(), ..p }).unwrap_or(PremiersRonds { jour: j, ..Default::default() }))
            .collect())
    }

    /// Le premier jour que la table `evenement` couvre entier pour chaque business affiché ; None tant qu'un d'eux n'a
    /// pas d'événements.
    pub fn evenements_depuis(&self) -> Res<Option<String>> {
        let cles = self.businesses.iter().map(|b| Valeur::from(format!("{CLE_EVENEMENTS_DEPUIS}{}", b.id))).collect();
        let sql = format!("select count(*) as n, max(valeur) as v from reglage where cle in {}", self.dans());
        let l = self.q(&sql, cles)?;
        let l = l.first();
        Ok(match l.and_then(|l| l.entier("n")) {
            Some(n) if n as usize == self.businesses.len() && n > 0 => l.and_then(|l| l.texte("v")),
            _ => None,
        })
    }

    /// Les événements du rond, du bilan et du retour de `debut` à `fin`, jour par jour, un jour sans événement
    /// compris ; `jusqua` (ISO UTC) les arrête à cet instant.
    pub fn ronds_par_jour(&self, debut: &str, fin: &str, jusqua: Option<&str>) -> Res<Vec<JourRonds>> {
        let noms = format!("({})", vec!["?"; NOMS.len()].join(", "));
        let coupe = if jusqua.is_some() { " and at <= ?" } else { "" };
        let params = || {
            let mut v = self.autour(p![], p![debut, fin]);
            v.extend(NOMS.iter().map(|n| Valeur::from(*n)));
            v.extend(jusqua.map(Valeur::from));
            v
        };
        let ou = format!("where business in {} and jour between ? and ? and nom in {noms}{coupe}", self.dans());
        let fini = |champ: &str| format!("sum(case when nom = 'path_round_done' then json_extract(props, '$.{champ}') end)");
        let sql = format!("
            select jour, sum(nom = 'path_round_started') as commences, sum(nom = 'path_round_done') as finis,
                sum(nom = 'path_round_left' and json_extract(props, '$.last_verdict') is not null) as quittes,
                {} as reponses, {} as justes, {} as voix,
                sum(case when nom = 'path_round_done' and json_extract(props, '$.forms') is not null
                    then json_extract(props, '$.said') end) as dites,
                sum(case when nom = 'path_round_done' and json_extract(props, '$.said') is not null
                    then json_extract(props, '$.forms') end) as formes,
                sum(nom = 'path_bilan_closed') as bilans_fermes,
                sum(nom = 'path_bilan_closed' and json_extract(props, '$.how') = 'finish') as bilans_finis,
                sum(case when nom = 'path_bilan_closed' and json_extract(props, '$.missed') is not null
                    then json_extract(props, '$.answered') end) as quiz,
                sum(case when nom = 'path_bilan_closed' and json_extract(props, '$.answered') is not null
                    then json_extract(props, '$.missed') end) as quiz_rates,
                sum(nom = 'app_opened') as ouvertures, sum(nom = 'notif_opened') as notifs,
                sum(nom = 'rating_sheet_answered') as avis,
                sum(nom = 'rating_sheet_answered' and json_extract(props, '$.answer') = 'yes') as avis_oui
            from evenement {ou} group by jour",
            fini("replies"), fini("rights"), fini("voice_replies"));
        let mut par_jour: HashMap<String, JourRonds> = self.q(&sql, params())?.iter().map(|l| {
            let j = jour_ronds(l);
            (j.jour.clone(), j)
        }).collect();
        let sql = format!("
            select jour, nom, json_extract(props, case nom when 'path_round_done' then '$.elapsed_s' else '$.tutor_ms' end) as v
            from evenement {ou} and nom in ('path_round_done', 'path_reply')");
        for l in self.q(&sql, params())? {
            let (Some(j), Some(nom), Some(v)) = (l.texte("jour"), l.texte("nom"), l.nombre("v")) else { continue };
            let e = par_jour.entry(j).or_default();
            if nom == "path_round_done" { e.durees.push(v) } else { e.tuteur.push(v) }
        }
        Ok(jours_de(debut, fin).into_iter()
            .map(|j| par_jour.remove(&j).map(|r| JourRonds { jour: j.clone(), ..r }).unwrap_or(JourRonds { jour: j, ..Default::default() }))
            .collect())
    }
}

fn jour_ronds(l: &Ligne) -> JourRonds {
    let n = |c: &str| l.entier(c).unwrap_or(0);
    JourRonds {
        jour: l.texte("jour").unwrap_or_default(),
        commences: n("commences"), finis: n("finis"), quittes: n("quittes"),
        reponses: n("reponses"), justes: n("justes"), voix: n("voix"), dites: n("dites"),
        formes: n("formes"), durees: vec![], tuteur: vec![], bilans_fermes: n("bilans_fermes"),
        bilans_finis: n("bilans_finis"), quiz: n("quiz"), quiz_rates: n("quiz_rates"), ouvertures: n("ouvertures"),
        notifs: n("notifs"), avis: n("avis"), avis_oui: n("avis_oui"),
    }
}
