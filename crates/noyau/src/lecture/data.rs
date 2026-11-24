//! L'écran Data : l'arbre des installs, le tunnel, les cohortes et les portes du paywall.
use super::decoupage::{ECRANS, RONDS_DEPUIS};
use super::Lectures;
use crate::db::{Ligne, Valeur};
use crate::dimension::Dimension;
use crate::erreur::Res;
use crate::jour;
use crate::modeles::Cohorte;
use crate::p;
use serde::{Deserialize, Serialize};

/// Un nœud de l'arbre des installs : ceux qui passent tous les filtres sur la période, ce qu'ils ont fait, ce que
/// RevenueCat dit de leur paiement. Ronds commencés, finis et retours ne comptent que les installs depuis `RONDS_DEPUIS`, et
/// J1, J7 les seuls jours d'install qui ont eu tout leur temps.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub valeur: String,
    pub installs: i64,
    pub paywall: i64,
    pub essais: i64,
    pub directs: i64,
    /// Installs mesurables pour les ronds, ceux qui en ont commencé un, ceux qui en ont fini un.
    pub base_rond: i64,
    pub commences: i64,
    pub ronds: i64,
    /// Convertis que RevenueCat a vus payer : essais passés à payant et achats sans essai.
    pub payants: i64,
    pub essais_payes: i64,
    /// Essais qui courent encore chez RevenueCat.
    pub en_cours: i64,
    /// Convertis que RevenueCat n'a pas encore lus, ou qu'il ne connaît pas en production.
    pub a_verifier: i64,
    pub base_j1: i64,
    pub j1: i64,
    pub base_j7: i64,
    pub j7: i64,
}

impl Segment {
    /// Sous ce nombre d'installs, un taux bouge trop d'un install à l'autre pour désigner le meilleur segment.
    pub const MINIMUM: i64 = 30;

    pub fn taux(n: i64, sur: i64) -> Option<f64> { (sur > 0).then(|| n as f64 / sur as f64) }

    pub fn paywall_vu(&self) -> Option<f64> { Self::taux(self.paywall, self.installs) }
    pub fn essai(&self) -> Option<f64> { Self::taux(self.essais, self.installs) }
    pub fn payant(&self) -> Option<f64> { Self::taux(self.payants, self.installs) }
    /// Essais que RevenueCat ne voit plus courir : payés, annulés, ou pas encore lus.
    pub fn essais_finis(&self) -> i64 { self.essais - self.en_cours }
    pub fn essai_vers_payant(&self) -> Option<f64> { Self::taux(self.essais_payes, self.essais_finis()) }
    pub fn commence(&self) -> Option<f64> { Self::taux(self.commences, self.base_rond) }
    pub fn rond(&self) -> Option<f64> { Self::taux(self.ronds, self.base_rond) }
    /// Ceux qui ont fini un rond, sur ceux qui en ont commencé un.
    pub fn fin_du_rond(&self) -> Option<f64> { Self::taux(self.ronds, self.commences) }
    pub fn retour_j1(&self) -> Option<f64> { Self::taux(self.j1, self.base_j1) }
    pub fn retour_j7(&self) -> Option<f64> { Self::taux(self.j7, self.base_j7) }
}

/// Un pas dans l'arbre : une réponse gardée ou écartée, ou une étape du tunnel passée.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Filtre {
    /// Une valeur de la dimension ; la valeur vide garde ceux qui n'ont pas répondu, `Dimension::PAS_MESURE` ceux
    /// d'avant la mesure.
    Valeur(Dimension, String),
    /// Tous sauf cette valeur.
    Sauf(Dimension, String),
    /// A passé l'écran de ce rang (1 à 17, `ECRANS`).
    Ecran(i64),
    /// A vu le paywall, depuis le tunnel ou une porte de l'app.
    Paywall,
    Essai,
    Payant,
}

/// Un build de l'app, sa version et ses installs, du premier au dernier jour d'install.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Build {
    pub numero: String,
    pub version: String,
    pub debut: String,
    pub fin: String,
    pub installs: i64,
}

impl Build {
    pub fn filtre(&self) -> Filtre { Filtre::Valeur(Dimension::Build, self.numero.clone()) }
}

impl Lectures<'_> {
    // arbre des installs

    /// Le dernier jour d'install dont le retour JN est complet au dernier calcul des installs : un install de 23 h 59
    /// a jusqu'à minuit de J+N+2 pour finir un rond entre N et N+1 jours après.
    pub fn complet_jusqua(&self, n: i64) -> Res<String> {
        let cles = self.businesses.iter().map(|b| Valeur::from(format!("posthog.horaire.{}", b.id))).collect();
        let sql = format!("select min(valeur) as v from reglage where cle in {}", self.dans());
        let calcul = self.q(&sql, cles)?.first().and_then(|l| l.date("v")).unwrap_or(self.maintenant);
        Ok(jour::decale(&jour::cle(&calcul), -n - 2))
    }

    /// Les installs de `debut` à `fin` qui passent tous les filtres.
    pub fn noeud(&self, filtres: &[Filtre], debut: &str, fin: &str) -> Res<Segment> {
        Ok(self.agrege(filtres, "''", "", "cle", debut, fin)?.into_iter().next().unwrap_or_default())
    }

    /// Le nœud découpé par valeur de la dimension, le plus gros d'abord (par rang pour l'écran atteint, le plus récent
    /// d'abord pour le build) ; un install à plusieurs réponses compte dans chacune, un install d'avant la mesure va
    /// sous `Dimension::PAS_MESURE`.
    pub fn enfants(&self, filtres: &[Filtre], d: Dimension, debut: &str, fin: &str) -> Res<Vec<Segment>> {
        let sans = d.mesuree_depuis()
            .map(|m| format!("case when i.jour < '{m}' then '{}' else '' end", Dimension::PAS_MESURE))
            .unwrap_or_else(|| "''".into());
        let ordre = match d {
            Dimension::Loin => "cast(cle as integer)",
            Dimension::Build => "cast(cle as integer) desc",
            _ => "installs desc, cle",
        };
        self.agrege(filtres, &format!("coalesce(v.value, {sans})"),
                    &format!("left join json_each(i.valeurs, '$.{}') v", d.brut()), ordre, debut, fin)
    }

    /// Le tunnel du nœud : combien ont passé chaque écran, rang 1 à 17.
    pub fn tunnel(&self, filtres: &[Filtre], debut: &str, fin: &str) -> Res<Vec<i64>> {
        let rangs: Vec<usize> = (1..=ECRANS.len()).collect();
        let (clauses, valeurs) = clauses(filtres);
        let sql = format!("
            select {}
            from install i left join paiement p on p.business = i.business and p.client = i.client
            where i.business in {} and i.jour between ? and ?{clauses}",
            rangs.iter().map(|r| format!("sum(i.loin >= {r}) as e{r}")).collect::<Vec<_>>().join(", "), self.dans());
        let lignes = self.q(&sql, self.autour(p![], [p![debut, fin], valeurs].concat()))?;
        let Some(l) = lignes.first() else { return Ok(vec![]) };
        Ok(rangs.iter().map(|r| l.entier(&format!("e{r}")).unwrap_or(0)).collect())
    }

    /// Les builds des installs, le plus récent d'abord.
    pub fn builds(&self) -> Res<Vec<Build>> {
        let sql = format!("
            select b.value as numero, max(json_extract(i.valeurs, '$.version[0]')) as version, min(i.jour) as debut,
                max(i.jour) as fin, count(*) as installs
            from install i join json_each(i.valeurs, '$.build') b
            where i.business in {}
            group by numero order by cast(numero as integer) desc", self.dans());
        Ok(self.q(&sql, self.ids())?.iter().map(|l| Build {
            numero: l.texte("numero").unwrap_or_default(),
            version: l.texte("version").unwrap_or_default(),
            debut: l.texte("debut").unwrap_or_default(),
            fin: l.texte("fin").unwrap_or_default(),
            installs: l.entier("installs").unwrap_or(0),
        }).collect())
    }

    /// Une cohorte par jour d'install de `debut` à `fin` parmi ceux qui passent les filtres, mesurée depuis
    /// `RONDS_DEPUIS`, la plus récente d'abord ; un Jn reste nul tant qu'il est incomplet.
    pub fn cohortes(&self, filtres: &[Filtre], debut: &str, fin: &str) -> Res<Vec<Cohorte>> {
        let complet = [1, 7, 14, 30].map(|n| self.complet_jusqua(n)).into_iter().collect::<Res<Vec<_>>>()?;
        let debut = debut.max(RONDS_DEPUIS);
        let (clauses, valeurs) = clauses(filtres);
        let sql = format!("
            select i.jour as jour, count(*) as installs, sum(i.j1) as j1, sum(i.j7) as j7, sum(i.j14) as j14,
                sum(i.j30) as j30
            from install i left join paiement p on p.business = i.business and p.client = i.client
            where i.jour between ? and ? and i.business in {}{clauses}
            group by i.jour order by i.jour desc", self.dans());
        Ok(self.q(&sql, self.autour(p![debut, fin], valeurs))?.iter().map(|l| {
            let jour = l.texte("jour").unwrap_or_default();
            let jn = |i: usize, c: &str| if jour <= complet[i] { l.entier(c) } else { None };
            Cohorte { j1: jn(0, "j1"), j7: jn(1, "j7"), j14: jn(2, "j14"), j30: jn(3, "j30"),
                      installs: l.entier("installs").unwrap_or(0), jour: jour.clone() }
        }).collect())
    }

    fn agrege(&self, filtres: &[Filtre], cle: &str, jointure: &str, ordre: &str, debut: &str, fin: &str) -> Res<Vec<Segment>> {
        let (r, c1, c7) = (RONDS_DEPUIS, self.complet_jusqua(1)?, self.complet_jusqua(7)?);
        let (clauses, valeurs) = clauses(filtres);
        let sql = format!("
            select {cle} as cle, count(*) as installs, sum(i.paywall) as paywall, sum(i.essai) as essais,
                sum(i.direct) as directs, sum(p.etat = 'paye') as payants,
                sum(i.essai and p.etat = 'paye') as essais_payes, sum(i.essai and p.etat = 'essai') as en_cours,
                sum((i.essai or i.direct) and coalesce(p.etat, 'inconnu') = 'inconnu') as a_verifier,
                sum(i.jour >= ?) as base_rond, sum(i.jour >= ? and i.commence) as commences,
                sum(i.jour >= ? and i.rond) as ronds,
                sum(i.jour between ? and ?) as base_j1, sum(i.jour between ? and ? and i.j1) as j1,
                sum(i.jour between ? and ?) as base_j7, sum(i.jour between ? and ? and i.j7) as j7
            from install i left join paiement p on p.business = i.business and p.client = i.client {jointure}
            where i.business in {} and i.jour between ? and ?{clauses}
            group by cle order by {ordre}", self.dans());
        let avant = p![r, r, r, r, &c1, r, &c1, r, &c7, r, &c7];
        Ok(self.q(&sql, self.autour(avant, [p![debut, fin], valeurs].concat()))?.iter().map(segment).collect())
    }
}

fn segment(l: &Ligne) -> Segment {
    let n = |c: &str| l.entier(c).unwrap_or(0);
    Segment {
        valeur: l.texte("cle").unwrap_or_default(),
        installs: n("installs"), paywall: n("paywall"), essais: n("essais"), directs: n("directs"),
        base_rond: n("base_rond"), commences: n("commences"), ronds: n("ronds"), payants: n("payants"), essais_payes: n("essais_payes"),
        en_cours: n("en_cours"), a_verifier: n("a_verifier"), base_j1: n("base_j1"), j1: n("j1"),
        base_j7: n("base_j7"), j7: n("j7"),
    }
}

/// Les filtres en SQL, chacun précédé de `and`, et leurs paramètres.
pub(super) fn clauses(filtres: &[Filtre]) -> (String, Vec<Valeur>) {
    let (mut sql, mut valeurs) = (String::new(), vec![]);
    for f in filtres {
        match f {
            Filtre::Valeur(d, v) => {
                let (c, p) = condition(*d, v);
                sql += &format!(" and ({c})");
                valeurs.extend(p);
            }
            Filtre::Sauf(d, v) => {
                let (c, p) = condition(*d, v);
                sql += &format!(" and not ({c})");
                valeurs.extend(p);
            }
            Filtre::Ecran(rang) => {
                sql += " and i.loin >= ?";
                valeurs.push(Valeur::from(*rang));
            }
            Filtre::Paywall => sql += " and i.paywall",
            Filtre::Essai => sql += " and i.essai",
            Filtre::Payant => sql += " and p.etat = 'paye'",
        }
    }
    (sql, valeurs)
}

/// L'install a cette valeur ; la valeur vide : pas de réponse alors que la question était posée.
fn condition(d: Dimension, v: &str) -> (String, Vec<Valeur>) {
    let aucune = format!("not exists (select 1 from json_each(i.valeurs, '$.{}'))", d.brut());
    match (v, d.mesuree_depuis()) {
        ("", None) => (aucune, vec![]),
        ("", Some(depuis)) => (format!("{aucune} and i.jour >= ?"), p![depuis]),
        (Dimension::PAS_MESURE, depuis) => (format!("{aucune} and i.jour < ?"), p![depuis.unwrap_or("")]),
        _ => (format!("exists (select 1 from json_each(i.valeurs, '$.{}') where value = ?)", d.brut()), p![v]),
    }
}
