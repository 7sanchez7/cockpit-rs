//! Ce que les écrans affichent, calculé depuis la base sans réseau.
//! Tout ce que les écrans lisent, filtré sur les business affichés (tous, ou celui du segmenté) : rien que des
//! `select` sur les tables décrites dans docs/donnees.md.
pub mod accueil;
pub mod argent;
pub mod data;
pub mod decoupage;
pub mod fenetre;
pub mod format;
pub mod noms;
pub mod percee;
pub mod recherche;
pub mod sante;
pub mod veille;
pub mod videos;
pub mod widget;
#[cfg(test)]
mod tests;

use crate::business::Business;
use crate::db::{Database, Ligne, Valeur};
use crate::erreur::Res;
use crate::jour::{self, Date};
use crate::metrique::Boutique;
use crate::modeles::{
    Analyse, Appli, Avis, CleFil, Evenement, FilSAV, Liste, MessageSAV, Passage, Plateforme, RetourSAV, Ton, TypeFil,
};
use crate::p;
use chrono::{Duration, TimeZone, Utc};
use serde::{Serialize, Serializer};
use std::collections::HashMap;

/// Une vidéo passe le seuil quand ses partages ou ses enregistrements dépassent 1,5 % des vues (fiches contenu/bank).
pub const SEUIL_VIDEO: f64 = 0.015;

/// Les lectures d'une base, filtrées sur `businesses`.
pub struct Lectures<'a> {
    pub db: &'a Database,
    pub businesses: Vec<Business>,
    pub maintenant: Date,
    /// Le store affiché ; None = les deux.
    pub boutique: Option<Boutique>,
}

/// La date la plus lointaine, pour « sans borne ».
fn loin_futur() -> Date { Utc.with_ymd_and_hms(4001, 1, 1, 0, 0, 0).unwrap() }

fn ton(s: &str) -> Option<Ton> { [Ton::Alerte, Ton::Neutre].into_iter().find(|t| t.brut() == s) }

impl<'a> Lectures<'a> {
    pub fn new(db: &'a Database, businesses: Vec<Business>) -> Self {
        Lectures { db, businesses, maintenant: jour::maintenant(), boutique: None }
    }

    fn ids(&self) -> Vec<Valeur> { self.businesses.iter().map(|b| Valeur::from(&b.id)).collect() }

    /// `(?, ?)` pour filtrer sur les business affichés.
    fn dans(&self) -> String { format!("({})", vec!["?"; self.businesses.len()].join(", ")) }

    /// `avant`, les ids des business, puis `apres` : l'ordre des `?` de la plupart des requêtes.
    fn autour(&self, avant: Vec<Valeur>, apres: Vec<Valeur>) -> Vec<Valeur> {
        avant.into_iter().chain(self.ids()).chain(apres).collect()
    }

    fn q(&self, sql: &str, params: Vec<Valeur>) -> Res<Vec<Ligne>> { self.db.query(sql, params) }

    // séries

    /// La série lue : celle du store affiché quand la métrique est écrite par store, le total sinon.
    pub fn serie(&self, m: &str) -> String {
        match self.boutique {
            Some(b) if Boutique::separee(m) => b.metrique(m),
            _ => m.to_string(),
        }
    }

    /// Valeur d'une journée, sommée sur les business (moyenne pour une latence) ; None si aucune source ne l'a écrite.
    pub fn valeur(&self, m: &str, jour: &str, moyenne: bool) -> Res<Option<f64>> {
        let sql = format!("select {}(valeur) as v from serie where metrique = ? and jour = ? and heure = -1 and business in {}",
                          if moyenne { "avg" } else { "sum" }, self.dans());
        Ok(self.q(&sql, self.autour(p![self.serie(m), jour], p![]))?.first().and_then(|l| l.nombre("v")))
    }

    /// Les `n` derniers jours jusqu'à `jusqua`, du plus ancien au plus récent ; None pour un jour sans relevé.
    pub fn jours(&self, m: &str, n: i64, jusqua: &str, moyenne: bool) -> Res<Vec<Option<f64>>> {
        let cles = jour::derniers(n, jusqua);
        let sql = format!(
            "select jour, {}(valeur) as v from serie
            where metrique = ? and heure = -1 and jour between ? and ? and business in {} group by jour",
            if moyenne { "avg" } else { "sum" }, self.dans());
        let lignes = self.q(&sql, self.autour(p![self.serie(m), cles.first(), cles.last()], p![]))?;
        let par_jour: HashMap<String, f64> =
            lignes.iter().filter_map(|l| Some((l.texte("jour")?, l.nombre("v")?))).collect();
        Ok(cles.iter().map(|c| par_jour.get(c).copied()).collect())
    }

    /// Les 24 heures d'une journée ; None pour une heure sans relevé.
    pub fn heures(&self, m: &str, jour: &str, moyenne: bool) -> Res<Vec<Option<f64>>> {
        let sql = format!(
            "select heure, {}(valeur) as v from serie
            where metrique = ? and jour = ? and heure >= 0 and business in {} group by heure",
            if moyenne { "avg" } else { "sum" }, self.dans());
        let mut h = vec![None; 24];
        for l in self.q(&sql, self.autour(p![self.serie(m), jour], p![]))? {
            if let Some(i) = l.entier("heure").filter(|i| (0..24).contains(i)) { h[i as usize] = l.nombre("v"); }
        }
        Ok(h)
    }

    /// Les fournisseurs IA appelés depuis `jour`, du plus sollicité au moins sollicité.
    /// `glob` et non `like` : sensible à la casse, il passe par la clé primaire de `serie`.
    pub fn fournisseurs_ia(&self, depuis: &str) -> Res<Vec<String>> {
        let sql = format!(
            "select substr(metrique, 8) as f, sum(valeur) as n from serie
            where metrique glob 'req_ia.*' and heure = -1 and jour >= ? and business in {} group by f order by n desc",
            self.dans());
        Ok(self.q(&sql, self.autour(p![depuis], p![]))?.iter().filter_map(|l| l.texte("f")).collect())
    }

    // fil

    /// Les événements entre `du` (inclus) et `au` (exclu), les plus récents d'abord ; None = sans borne.
    pub fn fil(&self, du: Option<&Date>, au: Option<&Date>, limite: i64) -> Res<Vec<Evenement>> {
        let sql = format!("select * from fil where business in {} and at >= ? and at < ? order by at desc limit ?", self.dans());
        let du = du.copied().unwrap_or_else(jour::loin_passe);
        let au = au.copied().unwrap_or_else(loin_futur);
        Ok(self.q(&sql, self.autour(p![], p![du, au, limite]))?.iter().filter_map(|l| {
            Some(Evenement {
                id: l.texte("id")?,
                at: l.date("at")?,
                type_: TypeFil::depuis(&l.texte("type").unwrap_or_default())?,
                business: l.texte("business").unwrap_or_default(),
                texte: l.texte("texte").unwrap_or_default(),
                lien: l.texte("lien"),
            })
        }).collect())
    }

    // vidéos

    /// Les vidéos des comptes suivis avec leur dernier relevé, les plus vues d'abord. Un compte retiré ne montre plus
    /// ses vidéos, gardées dans la base.
    pub fn videos(&self, limite: i64) -> Res<Vec<LigneVideo>> {
        let il_ya_24h = self.maintenant - Duration::seconds(86_400);
        let sql = format!("
            select v.id, v.business, v.compte, v.plateforme, v.titre, v.legende, v.format, v.fiche, v.url,
                v.publiee_le, v.duree_s, v.verdict, r.at, r.vues, r.likes, r.commentaires, r.partages,
                r.enregistrements, r.completion, r.watch_moyen_s, r.abonnes,
                (select a.vues from video_releve a where a.video = v.id and a.at <= ? order by a.at desc limit 1) as vues_avant
            from video v
            join compte c on c.plateforme = v.plateforme and c.handle = v.compte
            join video_releve r on r.video = v.id and r.at = (select max(at) from video_releve where video = v.id)
            where v.business in {}
            order by r.vues desc limit ?", self.dans());
        Ok(self.q(&sql, self.autour(p![il_ya_24h], p![limite]))?.iter().map(|l| {
            let vues = l.entier("vues").unwrap_or(0);
            let publiee = l.date("publiee_le");
            let gain = match (l.entier("vues_avant"), publiee) {
                (Some(avant), _) => Some(vues - avant),
                (None, Some(p)) if p > il_ya_24h => Some(vues),
                _ => None,
            };
            let compte = l.texte("compte").unwrap_or_default();
            LigneVideo {
                id: l.texte("id").unwrap_or_default(),
                business: l.texte("business").unwrap_or_default(),
                plateforme: Plateforme::depuis(&l.texte("plateforme").unwrap_or_default()).unwrap_or(Plateforme::Tiktok),
                titre: titre(l.texte("legende").as_deref(), l.texte("titre").as_deref()).unwrap_or_else(|| {
                    format!("@{compte}{}", publiee.map(|d| format!(", {}", format::jour_court(&d))).unwrap_or_default())
                }),
                compte,
                format: l.texte("format"),
                fiche: l.texte("fiche"),
                url: l.texte("url"),
                publiee_le: publiee,
                duree_s: l.nombre("duree_s"),
                verdict: l.booleen("verdict"),
                vues,
                likes: l.entier("likes"),
                commentaires: l.entier("commentaires"),
                partages: l.entier("partages"),
                enregistrements: l.entier("enregistrements"),
                completion: l.nombre("completion"),
                watch_moyen_s: l.nombre("watch_moyen_s"),
                abonnes: l.entier("abonnes"),
                gain24h: gain,
                releve_le: l.date("at").unwrap_or(self.maintenant),
            }
        }).collect())
    }

    /// Les relevés d'une vidéo, du plus ancien au plus récent.
    pub fn releves(&self, video: &str) -> Res<Vec<PointVues>> {
        Ok(self.q("select at, vues from video_releve where video = ? order by at", p![video])?.iter()
            .filter_map(|l| Some(PointVues { at: l.date("at")?, vues: l.entier("vues").unwrap_or(0) }))
            .collect())
    }

    /// L'accroche de chaque vidéo qui a une courbe de rétention : la part encore là à `percee::ACCROCHE_S` secondes.
    pub fn accroches(&self) -> Res<HashMap<String, f64>> {
        let sql = format!("select video, json_extract(donnees, '$.retention[{}]') as a from video_analyse", percee::ACCROCHE_S);
        Ok(self.q(&sql, p![])?.iter().filter_map(|l| Some((l.texte("video")?, l.nombre("a")?))).collect())
    }

    /// L'analyse privée d'une vidéo et sa date ; None si sa plateforme ne l'a jamais donnée, une erreur si elle est illisible.
    pub fn analyse(&self, video: &str) -> Res<Option<AnalyseDatee>> {
        let lignes = self.q("select at, donnees from video_analyse where video = ?", p![video])?;
        let Some((at, json)) = lignes.first().and_then(|l| Some((l.date("at")?, l.texte("donnees")?))) else { return Ok(None) };
        Ok(Some(AnalyseDatee { analyse: serde_json::from_str(&json)?, at }))
    }

    // à traiter

    /// Les tâches ouvertes : en retard et alertes d'abord, puis par échéance.
    pub fn taches(&self) -> Res<Vec<LigneTache>> {
        let sql = format!("
            select * from a_traiter where fait_le is null and (business is null or business in {})
            order by echeance is null, echeance, cree_le desc", self.dans());
        let (alertes, autres): (Vec<_>, Vec<_>) =
            self.q(&sql, self.ids())?.iter().map(|l| self.tache(l)).partition(|t| t.ton == Ton::Alerte);
        Ok(alertes.into_iter().chain(autres).collect())
    }

    /// Les tâches faites ces `jours` derniers jours, les plus récentes d'abord.
    pub fn taches_faites(&self, jours: i64) -> Res<Vec<LigneTache>> {
        let sql = format!("
            select * from a_traiter where fait_le >= ? and (business is null or business in {})
            order by fait_le desc limit 30", self.dans());
        let depuis = self.maintenant - Duration::seconds(jours * 86_400);
        Ok(self.q(&sql, self.autour(p![depuis], p![]))?.iter().map(|l| self.tache(l)).collect())
    }

    pub fn nb_a_traiter(&self) -> Res<i64> {
        let sql = format!("select count(*) as n from a_traiter where fait_le is null and (business is null or business in {})",
                          self.dans());
        Ok(self.q(&sql, self.ids())?.first().and_then(|l| l.entier("n")).unwrap_or(0))
    }

    fn tache(&self, l: &Ligne) -> LigneTache {
        let echeance = l.texte("echeance");
        let en_retard = echeance.as_ref().is_some_and(|e| *e < jour::cle(&self.maintenant));
        LigneTache {
            id: l.texte("id").unwrap_or_default(),
            business: l.texte("business"),
            texte: l.texte("texte").unwrap_or_default(),
            etiquette: l.texte("etiquette"),
            ton: if en_retard { Ton::Alerte } else { l.texte("ton").and_then(|t| ton(&t)).unwrap_or(Ton::Neutre) },
            echeance,
            source: l.texte("source").unwrap_or_else(|| "manuel".into()),
            lien: l.texte("lien"),
            cree_le: l.date("cree_le"),
            fait_le: l.date("fait_le"),
        }
    }

    // SAV

    /// Les fils, ceux à répondre d'abord, puis du plus récent au plus ancien.
    pub fn fils_sav(&self, limite: i64) -> Res<Vec<FilSAV>> {
        let sql = format!("
            select f.*, c.email from sav_fil f left join sav_compte c on c.business = f.business and c.uid = f.uid
            where f.business in {} order by f.ouvert desc, f.dernier_at desc limit ?", self.dans());
        Ok(self.q(&sql, self.autour(p![], p![limite]))?.iter().filter_map(fil).collect())
    }

    pub fn fil_sav(&self, cle: &CleFil) -> Res<Option<FilSAV>> {
        Ok(self.q("
            select f.*, c.email from sav_fil f left join sav_compte c on c.business = f.business and c.uid = f.uid
            where f.business = ? and f.uid = ?", p![&cle.business, &cle.uid])?.first().and_then(fil))
    }

    /// Les messages d'un fil, du plus ancien au plus récent.
    pub fn messages_sav(&self, cle: &CleFil) -> Res<Vec<MessageSAV>> {
        Ok(self.q("select * from sav_message where business = ? and uid = ? order by at, id", p![&cle.business, &cle.uid])?
            .iter()
            .filter_map(|l| Some(MessageSAV {
                id: l.texte("id")?,
                at: l.date("at")?,
                equipe: l.booleen("equipe"),
                texte: l.texte("texte").unwrap_or_default(),
            }))
            .collect())
    }

    /// Les retours, les plus récents d'abord ; `traites` None = tous.
    pub fn retours_sav(&self, traites: Option<bool>, limite: i64) -> Res<Vec<RetourSAV>> {
        let filtre = traites.map(|t| format!("and r.traite = {}", t as i64)).unwrap_or_default();
        let sql = format!("
            select r.*, c.email from sav_retour r left join sav_compte c on c.business = r.business and c.uid = r.uid
            where r.business in {} {filtre} order by r.at desc limit ?", self.dans());
        Ok(self.q(&sql, self.autour(p![], p![limite]))?.iter().filter_map(retour).collect())
    }

    /// Un retour par sa clé, pour le traiter ou le rouvrir.
    pub fn retour_sav(&self, business: &str, id: &str) -> Res<Option<RetourSAV>> {
        Ok(self.q("
            select r.*, c.email from sav_retour r left join sav_compte c on c.business = r.business and c.uid = r.uid
            where r.business = ? and r.id = ?", p![business, id])?.first().and_then(retour))
    }

    /// L'email connu d'un compte, vide pour un invité, None s'il n'a pas encore été lu.
    pub fn email_sav(&self, cle: &CleFil) -> Res<Option<String>> {
        Ok(self.q("select email from sav_compte where business = ? and uid = ?", p![&cle.business, &cle.uid])?
            .first().and_then(|l| l.texte("email")))
    }

    /// Les fils à répondre et les retours à traiter.
    pub fn nb_sav(&self) -> Res<NbSAV> {
        let sql = format!("
            select (select count(*) from sav_fil where ouvert = 1 and business in {d}) as fils,
                (select count(*) from sav_retour where traite = 0 and business in {d}) as retours", d = self.dans());
        let n = self.q(&sql, self.autour(p![], self.ids()))?;
        let n = n.first();
        Ok(NbSAV {
            fils: n.and_then(|l| l.entier("fils")).unwrap_or(0),
            retours: n.and_then(|l| l.entier("retours")).unwrap_or(0),
        })
    }

    // collecte

    /// Le dernier passage de chaque source.
    pub fn derniers_passages(&self) -> Res<HashMap<String, Passage>> {
        let mut r = HashMap::new();
        for l in self.q("select * from collecte c where debut = (select max(debut) from collecte where source = c.source)", p![])? {
            let p = Passage::depuis_ligne(&l);
            r.entry(p.source.clone()).or_insert(p);
        }
        Ok(r)
    }

    /// Les derniers passages d'une source, ou de toutes.
    pub fn journal(&self, source: Option<&str>, limite: i64) -> Res<Vec<Passage>> {
        let lignes = match source {
            Some(s) => self.q("select * from collecte where source = ? order by debut desc limit ?", p![s, limite])?,
            None => self.q("select * from collecte order by debut desc limit ?", p![limite])?,
        };
        Ok(lignes.iter().map(Passage::depuis_ligne).collect())
    }

    // classements

    /// Le dernier passage de la source Classements, et celui d'environ 24 h avant.
    fn passages_rang(&self) -> Res<Option<(Date, Option<Date>)>> {
        let Some(dernier) = self.q("select max(at) as at from rang", p![])?.first().and_then(|l| l.date("at")) else {
            return Ok(None);
        };
        let veille = self.q("select max(at) as at from rang where at <= ?", p![dernier - Duration::hours(23)])?
            .first().and_then(|l| l.date("at"));
        Ok(Some((dernier, veille)))
    }

    fn positions(&self, at: &Date) -> Res<HashMap<String, HashMap<CleRang, i64>>> {
        let mut r: HashMap<String, HashMap<CleRang, i64>> = HashMap::new();
        for l in self.q("select app, pays, genre, liste, position from rang where at = ?", p![at])? {
            let (Some(app), Some(pays), Some(genre), Some(liste), Some(p)) = (
                l.texte("app"), l.texte("pays"), l.entier("genre"),
                Liste::depuis(&l.texte("liste").unwrap_or_default()), l.entier("position"),
            ) else { continue };
            r.entry(app).or_default().insert(CleRang { pays, genre, liste }, p);
        }
        Ok(r)
    }

    /// Nos apps des business affichés, puis celles des potes, avec leurs rangs au dernier passage et à la veille.
    pub fn apps(&self) -> Res<Vec<LigneApp>> {
        let sql = format!("select * from app where suivie = 1 and (business is null or business in {}) order by business is null, nom",
                          self.dans());
        let suivies = self.q(&sql, self.ids())?;
        let passages = self.passages_rang()?;
        let mut derniers = match &passages { Some((d, _)) => self.positions(d)?, None => HashMap::new() };
        let mut veille = match &passages { Some((_, Some(v))) => self.positions(v)?, _ => HashMap::new() };
        Ok(suivies.iter().map(|l| {
            let app = Appli {
                id: l.texte("id").unwrap_or_default(),
                nom: l.texte("nom").unwrap_or_default(),
                editeur: l.texte("editeur"),
                icone: l.texte("icone"),
                genre: l.entier("genre"),
                note: l.nombre("note"),
                nb_notes: l.entier("nb_notes"),
                version: l.texte("version"),
                business: l.texte("business"),
            };
            LigneApp {
                rangs: derniers.remove(&app.id).unwrap_or_default(),
                veille: veille.remove(&app.id).unwrap_or_default(),
                lu: passages.map(|p| p.0),
                app,
            }
        }).collect())
    }

    /// La meilleure position du jour dans un classement, sur les `n` derniers jours ; None = hors top ou pas lu.
    pub fn rang_jours(&self, app: &str, cle: &CleRang, n: i64) -> Res<Vec<Option<f64>>> {
        let cles = jour::derniers(n, &jour::cle(&self.maintenant));
        let Some(debut) = cles.first().and_then(|c| jour::date(c)) else { return Ok(vec![]) };
        let mut meilleur: HashMap<String, i64> = HashMap::new();
        for l in self.q("select at, position from rang where app = ? and pays = ? and genre = ? and liste = ? and at >= ?",
                        p![app, &cle.pays, cle.genre, cle.liste.brut(), debut])? {
            let (Some(at), Some(p)) = (l.date("at"), l.entier("position")) else { continue };
            let m = meilleur.entry(jour::cle(&at)).or_insert(p);
            *m = (*m).min(p);
        }
        Ok(cles.iter().map(|c| meilleur.get(c).map(|&p| p as f64)).collect())
    }

    /// Le rang de notre app dans son classement principal : premier business affiché, premier pays, gratuit.
    pub fn rang_principal(&self) -> Res<Option<RangPrincipal>> {
        let Some(b) = self.businesses.iter().find(|b| b.config.app_store_id.is_some()) else { return Ok(None) };
        let (Some(id), Some(pays)) = (&b.config.app_store_id, b.config.pays.first()) else { return Ok(None) };
        let Some(ligne) = self.apps()?.into_iter().find(|a| &a.app.id == id) else { return Ok(None) };
        Ok(Some(RangPrincipal { cle: CleRang { pays: pays.clone(), genre: b.config.genre, liste: Liste::Gratuit }, ligne }))
    }

    // avis

    pub fn avis(&self, limite: i64) -> Res<Vec<Avis>> {
        let sql = format!("select * from avis where business in {} order by at desc limit ?", self.dans());
        Ok(self.q(&sql, self.autour(p![], p![limite]))?.iter().filter_map(|l| Some(Avis {
            id: l.texte("id")?,
            at: l.date("at")?,
            business: l.texte("business").unwrap_or_default(),
            pays: l.texte("pays"),
            note: l.entier("note").unwrap_or(0),
            titre: l.texte("titre"),
            texte: l.texte("texte"),
            auteur: l.texte("auteur"),
            repondu: l.booleen("repondu"),
        })).collect())
    }
}

fn retour(l: &Ligne) -> Option<RetourSAV> {
    Some(RetourSAV {
        business: l.texte("business")?,
        id: l.texte("id")?,
        at: l.date("at")?,
        uid: l.texte("uid"),
        email: l.texte("email"),
        source: l.texte("source"),
        categorie: l.texte("categorie"),
        etoiles: l.entier("etoiles"),
        message: l.texte("message").unwrap_or_default(),
        citation: l.texte("citation"),
        conversation: l.texte("conversation"),
        traite: l.booleen("traite"),
        traite_le: l.date("traite_le"),
        plateforme: l.texte("plateforme"),
        build: l.texte("build"),
    })
}

fn fil(l: &Ligne) -> Option<FilSAV> {
    Some(FilSAV {
        business: l.texte("business")?,
        uid: l.texte("uid")?,
        dernier_at: l.date("dernier_at")?,
        nom: l.texte("nom"),
        email: l.texte("email"),
        dernier_equipe: l.booleen("dernier_equipe"),
        apercu: l.texte("apercu").unwrap_or_default(),
        ouvert: l.booleen("ouvert"),
        non_vu: l.booleen("non_vu"),
    })
}

/// Le titre de la fiche, sinon le début de la légende sans les hashtags (comme la source TikTok).
pub fn titre(legende: Option<&str>, fiche: Option<&str>) -> Option<String> {
    if let Some(f) = fiche.filter(|f| !f.is_empty()) { return Some(f.to_string()); }
    let mots = legende.unwrap_or("").split_whitespace().filter(|m| !m.starts_with('#')).collect::<Vec<_>>().join(" ");
    if mots.is_empty() { return None; }
    Some(if mots.chars().count() > 60 { mots.chars().take(59).collect::<String>() + "…" } else { mots })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LigneVideo {
    pub id: String,
    pub business: String,
    pub compte: String,
    pub plateforme: Plateforme,
    pub titre: String,
    pub format: Option<String>,
    pub fiche: Option<String>,
    pub url: Option<String>,
    pub publiee_le: Option<Date>,
    pub duree_s: Option<f64>,
    pub verdict: bool,
    pub vues: i64,
    pub likes: Option<i64>,
    pub commentaires: Option<i64>,
    pub partages: Option<i64>,
    pub enregistrements: Option<i64>,
    pub completion: Option<f64>,
    pub watch_moyen_s: Option<f64>,
    pub abonnes: Option<i64>,
    /// Vues gagnées sur 24 h ; None sans relevé assez ancien.
    pub gain24h: Option<i64>,
    pub releve_le: Date,
}

impl LigneVideo {
    pub fn part_partages(&self) -> Option<f64> { self.part(self.partages) }
    pub fn part_enreg(&self) -> Option<f64> { self.part(self.enregistrements) }
    pub fn seuil_atteint(&self) -> bool { self.part_partages().unwrap_or(0.0).max(self.part_enreg().unwrap_or(0.0)) >= SEUIL_VIDEO }

    /// Likes, commentaires, partages et enregistrements sur les vues ; None sans aucun de ces compteurs.
    pub fn engagement(&self) -> Option<f64> {
        let n: Vec<i64> = [self.likes, self.commentaires, self.partages, self.enregistrements].into_iter().flatten().collect();
        (!n.is_empty() && self.vues != 0).then(|| n.iter().sum::<i64>() as f64 / self.vues as f64)
    }

    /// Abonnés gagnés pour 1 000 vues : la vidéo qui convertit, connue seulement par les stats privées.
    pub fn conversion(&self) -> Option<f64> {
        self.abonnes.and_then(|a| (self.vues > 0).then(|| a as f64 * 1000.0 / self.vues as f64))
    }

    fn part(&self, n: Option<i64>) -> Option<f64> { n.map(|n| if self.vues > 0 { n as f64 / self.vues as f64 } else { 0.0 }) }
}

/// Un relevé de vues d'une vidéo.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PointVues {
    pub at: Date,
    pub vues: i64,
}

/// L'analyse privée d'une vidéo et la date où elle a été lue.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyseDatee {
    pub analyse: Analyse,
    pub at: Date,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LigneTache {
    pub id: String,
    pub business: Option<String>,
    pub texte: String,
    pub etiquette: Option<String>,
    pub ton: Ton,
    pub echeance: Option<String>,
    pub source: String,
    pub lien: Option<String>,
    pub cree_le: Option<Date>,
    pub fait_le: Option<Date>,
}

impl LigneTache {
    /// L'étiquette posée par la source, sinon l'échéance dite en clair.
    pub fn pastille(&self, aujourdhui: &str) -> Option<String> {
        if let Some(e) = &self.etiquette { return Some(e.clone()); }
        let echeance = self.echeance.as_deref()?;
        Some(if echeance < aujourdhui { "retard".into() }
             else if echeance == aujourdhui { "aujourd'hui".into() }
             else if echeance == jour::decale(aujourdhui, 1) { "demain".into() }
             else { format::jour_court_cle(echeance) })
    }
}

/// Les fils à répondre et les retours à traiter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NbSAV {
    pub fils: i64,
    pub retours: i64,
}

/// Un classement lu par la source Classements.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleRang {
    pub pays: String,
    pub genre: i64,
    pub liste: Liste,
}

/// Une position dans un classement, telle que l'interface la reçoit.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PositionRang<'a> {
    #[serde(flatten)]
    cle: &'a CleRang,
    position: i64,
}

/// Les positions en liste triée : une clé de JSON ne peut pas être un objet.
fn en_liste<S: Serializer>(m: &HashMap<CleRang, i64>, s: S) -> Result<S::Ok, S::Error> {
    let mut v: Vec<PositionRang> = m.iter().map(|(cle, &position)| PositionRang { cle, position }).collect();
    v.sort_by(|a, b| (&a.cle.pays, a.cle.genre, a.cle.liste.brut()).cmp(&(&b.cle.pays, b.cle.genre, b.cle.liste.brut())));
    v.serialize(s)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LigneApp {
    pub app: Appli,
    /// Position au dernier passage ; une clé absente = hors top 200.
    #[serde(serialize_with = "en_liste")]
    pub rangs: HashMap<CleRang, i64>,
    #[serde(serialize_with = "en_liste")]
    pub veille: HashMap<CleRang, i64>,
    /// Heure du dernier passage de Classements, None s'il n'y en a jamais eu.
    pub lu: Option<Date>,
}

impl LigneApp {
    pub fn id(&self) -> &str { &self.app.id }
    pub fn est_a_nous(&self) -> bool { self.app.business.is_some() }

    /// Places gagnées depuis la veille : positif = monte. None si absent d'un des deux passages.
    pub fn gain(&self, cle: &CleRang) -> Option<i64> { Some(self.veille.get(cle)? - self.rangs.get(cle)?) }
}

/// Notre app dans son classement principal.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RangPrincipal {
    pub cle: CleRang,
    pub ligne: LigneApp,
}
