//! Charge et échecs IA, messages et retours en attente, et le SAV (`sav`), lus dans Firestore.
//! Voir docs/sources/firebase.md.
pub mod cout;
pub mod firestore;
pub mod sav;

use crate::business::Business;
use crate::collecte::{Contexte, Source};
use crate::erreur::{format_err, Erreur, Res};
use crate::jour::{self, Date};
use crate::metrique;
use crate::modeles::{ids_sav, Evenement, Tache, Ton, TypeFil};
use crate::store::Store;
use async_trait::async_trait;
use firestore::{Document, Firestore, Valeur};
use serde_json::json;
use std::collections::BTreeMap;

pub const ID_SOURCE: &str = ids_sav::SOURCE;
/// Jours relus quand hier ou le plus vieux manque : `aiLoad` n'en garde que 30 (TTL Firestore de Roleo).
pub const JOURS_HISTORIQUE: i64 = 30;
/// Échecs d'un fournisseur en une heure (429 compris) qui font une ligne du fil.
pub const SEUIL_ECHECS: i64 = 20;
/// Requêtes de l'heure à partir desquelles un fournisseur tout en échec fait une ligne, même sous le seuil.
pub const MINIMUM_PANNE: i64 = 5;
/// Un message sans réponse depuis plus longtemps passe en alerte.
pub const DELAI_MESSAGE_S: i64 = 24 * 3600;
pub const LONGUEUR_EXTRAIT: usize = 80;

/// Le nom affiché d'un fournisseur IA.
pub fn nom_fournisseur(f: &str) -> String {
    match f {
        "anthropic" => "Anthropic".into(),
        "mistral" => "Mistral".into(),
        "deepinfra" => "DeepInfra".into(),
        "fireworks" => "Fireworks".into(),
        "gemini" => "Gemini".into(),
        _ => capitaliser(f),
    }
}

/// Le dashboard admin, sur le site Hosting par défaut du projet.
pub fn dashboard(projet: &str) -> String { format!("https://{projet}.web.app") }

pub struct Firebase;

#[async_trait]
impl Source for Firebase {
    fn id(&self) -> &'static str { ID_SOURCE }
    fn nom(&self) -> &'static str { "Firebase" }
    fn cadence(&self) -> i64 { 15 * 60 }

    async fn collecte(&self, ctx: &Contexte) -> Res<usize> {
        let projets: Vec<(&Business, &String)> = ctx.businesses.iter()
            .filter_map(|b| b.config.firebase_project.as_ref().filter(|p| !p.is_empty()).map(|p| (b, p)))
            .collect();
        if projets.is_empty() {
            return Err(Erreur::NonConfigure("aucun business n'a de projet Firebase".into()));
        }
        let mut lignes = 0;
        for (b, projet) in projets {
            match collecte_business(b, projet, ctx).await {
                Ok(n) => lignes += n,
                Err(Erreur::NonConfigure(m)) => return Err(Erreur::NonConfigure(format!("{} : {m}", b.nom))),
                Err(e) => return Err(Erreur::Commande(format!("{} : {e}", b.nom))),
            }
        }
        Ok(lignes)
    }
}

async fn collecte_business(b: &Business, projet: &str, ctx: &Contexte) -> Res<usize> {
    let f = Firestore { projet: projet.into(), http: ctx.http.clone(), jeton: ctx.gcloud.jeton().await? };
    let maintenant = jour::maintenant();
    let base = dashboard(projet);
    // le SAV d'abord, dans son propre lot : une charge IA illisible (un moteur de voix sans prix) ne le fige pas
    let messages = f.ou("users", "supportOpen", json!({"booleanValue": true}), Some(&sav::CHAMPS_FIL)).await?;
    let retours = f.ou("feedback", "status", json!({"stringValue": "new"}), None).await?;
    let releve = sav::lire_sav(&b.id, &f, &messages, &retours, &ctx.store).await?;
    let lignes_sav = ctx.store.lot(|| {
        Ok(ecrire_messages(&messages, &retours, &b.id, &base, &maintenant, &ctx.store)?
            + ctx.store.synchroniser_sav(&b.id, &releve)?)
    })?;
    let historique = ctx.store.historique_incomplet(&b.id, metrique::COUT_TEXTE, JOURS_HISTORIQUE, &jour::cle(&maintenant))?;
    let heures = heures(if historique { JOURS_HISTORIQUE } else { 2 }, &maintenant);
    let ids: Vec<String> = heures.iter().map(id_heure).collect();
    let docs = f.documents("aiLoad", &ids, &["m"]).await?;
    let charges = heures.iter()
        .map(|h| Ok((*h, charge(docs.get(&id_heure(h)))?)))
        .collect::<Res<Vec<_>>>()?;
    Ok(lignes_sav + ctx.store.lot(|| ecrire_charges(&charges, &b.id, &format!("{base}/ia/"), &ctx.store))?)
}

// charge IA

/// Le début de chaque heure, de minuit à Paris il y a `jours - 1` jours jusqu'à l'heure de `fin` comprise.
pub fn heures(jours: i64, fin: &Date) -> Vec<Date> {
    let Some(debut) = jour::date(&jour::decale(&jour::cle(fin), 1 - jours)) else { return vec![] };
    std::iter::successors(Some(debut), |h| Some(*h + chrono::Duration::hours(1)))
        .take_while(|h| h <= fin)
        .collect()
}

/// L'id du document `aiLoad` d'une heure : `2026-11-12T23`, heure UTC.
pub fn id_heure(debut: &Date) -> String { jour::iso(debut)[..13].to_string() }

/// La charge d'un document `aiLoad`, vide s'il n'existe pas (heure sans appel ou pas commencée).
pub fn charge(doc: Option<&Document>) -> Res<Charge> {
    let Some(doc) = doc else { return Ok(Charge::default()) };
    let m = doc.champ("m").ok_or_else(|| format_err(format!("aiLoad/{} sans champ m", doc.id())))?;
    Charge::depuis(m)
}

/// Écrit les séries IA des jours de Paris touchés par `heures`, et une ligne du fil par heure
/// et fournisseur notables. Le journalier est la somme des heures. Rend le nombre de lignes.
pub fn ecrire_charges(heures: &[(Date, Charge)], business: &str, lien: &str, store: &Store) -> Res<usize> {
    let mut jours: BTreeMap<String, BTreeMap<i64, Charge>> = BTreeMap::new();
    let mut lignes = 0;
    for (debut, c) in heures {
        // deux heures UTC tombent sur la même heure de Paris au passage à l'heure d'hiver
        jours.entry(jour::cle(debut)).or_default().entry(jour::heure(debut)).or_default().ajoute(c);
        for (f, x) in &c.fournisseurs {
            let Some(texte) = alerte(f, x) else { continue };
            let e = Evenement {
                id: format!("{ID_SOURCE}:ia:{f}:{}", id_heure(debut)), business: business.into(), at: *debut,
                type_: TypeFil::Ia, texte, lien: Some(lien.into()),
            };
            if store.fil(&e)? {
                lignes += 1;
            }
        }
    }
    for (jour, par_heure) in &jours {
        let mut total = Charge::default();
        for c in par_heure.values() {
            total.ajoute(c);
        }
        store.serie(business, metrique::TOKENS_IA, jour, -1, total.tokens as f64)?;
        store.serie(business, metrique::ECHECS_IA, jour, -1, total.echecs as f64)?;
        store.serie(business, metrique::COUT_TEXTE, jour, -1, cout::euros_tokens(total.tokens as f64))?;
        // un jour sans voix garde ce qui est déjà relevé : Roleo ne compte la voix dans aiLoad que depuis le 17 nov. 2026
        if !total.voix.is_empty() || total.dictee_ms > 0 {
            let euros = cout::euros_voix(&total.voix)? + cout::euros_dictee(total.dictee_ms as f64);
            store.serie(business, metrique::COUT_VOIX, jour, -1, euros)?;
            lignes += 1;
        }
        store.somme(business, metrique::COUT_IA, &[metrique::COUT_TEXTE.into(), metrique::COUT_VOIX.into()], jour)?;
        lignes += 4;
        for (f, c) in &total.fournisseurs {
            lignes += serie(c, f, business, jour, -1, store)?;
            for (h, x) in par_heure {
                lignes += serie(&x.fournisseurs.get(f).cloned().unwrap_or_default(), f, business, jour, *h, store)?;
            }
        }
    }
    Ok(lignes)
}

fn serie(c: &Compteurs, f: &str, business: &str, jour: &str, heure: i64, store: &Store) -> Res<usize> {
    store.serie(business, &metrique::requetes_ia(f), jour, heure, c.n as f64)?;
    store.serie(business, &metrique::q429_ia(f), jour, heure, c.q as f64)?;
    store.serie(business, &metrique::erreurs_ia(f), jour, heure, c.e as f64)?;
    if c.n <= 0 {
        return Ok(3);
    }
    store.serie(business, &metrique::latence_ia(f), jour, heure, c.ms as f64 / c.n as f64)?;
    Ok(4)
}

/// Le texte du fil quand un fournisseur a trop échoué dans l'heure, `None` sinon.
pub fn alerte(fournisseur: &str, c: &Compteurs) -> Option<String> {
    let nom = nom_fournisseur(fournisseur);
    if c.n >= MINIMUM_PANNE && c.q + c.e >= c.n {
        return Some(format!("{nom} ne répond plus, {} requêtes en échec cette heure", c.n));
    }
    if c.q + c.e < SEUIL_ECHECS {
        return None;
    }
    let mut morceaux = vec![];
    if c.q > 0 {
        morceaux.push(if c.q == 1 { "1 erreur 429".to_string() } else { format!("{} erreurs 429", c.q) });
    }
    if c.e > 0 {
        morceaux.push(if c.e == 1 { "1 échec".to_string() } else { format!("{} échecs", c.e) });
    }
    Some(format!("{nom}, {} cette heure", morceaux.join(" et ")))
}

// messages et retours

/// Fils ouverts et retours nouveaux : compteurs du jour, tâches (un seul appel, la liste complète),
/// lignes du fil. Rend le nombre de lignes.
pub fn ecrire_messages(messages: &[Document], retours: &[Document], business: &str, base: &str, maintenant: &Date,
                       store: &Store) -> Res<usize> {
    let mut taches = vec![];
    let mut evenements = vec![];
    for d in messages {
        let at = d.champ("supportLastAt").and_then(Valeur::instant)
            .ok_or_else(|| format_err("un fil ouvert sans supportLastAt"))?;
        let cle = format!("msg:{}:{}", d.id(), jour::iso(&at));
        let texte = extrait(d.champ("supportPreview").and_then(Valeur::texte).unwrap_or(""))
            .unwrap_or_else(|| "Nouveau message".into());
        let lien = format!("{base}/messages/");
        let ton = if (*maintenant - at).num_milliseconds() > DELAI_MESSAGE_S * 1000 { Ton::Alerte } else { Ton::Neutre };
        evenements.push(Evenement {
            id: format!("{ID_SOURCE}:{cle}"), business: business.into(), at, type_: TypeFil::Msg, texte: texte.clone(),
            lien: Some(lien.clone()),
        });
        taches.push(Tache { cle, texte, etiquette: Some(age(&at, maintenant)), ton, lien: Some(lien), ..Default::default() });
    }
    for d in retours {
        let at = d.champ("createdAt").and_then(Valeur::instant).ok_or_else(|| format_err("un retour sans createdAt"))?;
        let cle = format!("retour:{}", d.id());
        let texte = libelle(d);
        let lien = format!("{base}/feedback/");
        evenements.push(Evenement {
            id: format!("{ID_SOURCE}:{cle}"), business: business.into(), at, type_: TypeFil::Retour, texte: texte.clone(),
            lien: Some(lien.clone()),
        });
        taches.push(Tache { cle, texte, etiquette: Some(age(&at, maintenant)), lien: Some(lien), ..Default::default() });
    }
    store.synchroniser_taches(ID_SOURCE, business, &taches)?;
    let jour = jour::cle(maintenant);
    store.serie(business, metrique::MESSAGES_OUVERTS, &jour, -1, messages.len() as f64)?;
    store.serie(business, metrique::RETOURS_NOUVEAUX, &jour, -1, retours.len() as f64)?;
    let mut nouveaux = 0;
    for e in &evenements {
        if store.fil(e)? {
            nouveaux += 1;
        }
    }
    Ok(taches.len() + nouveaux + 2)
}

/// « Problème : le micro coupe », d'après la provenance du retour.
pub fn libelle(d: &Document) -> String {
    let source = d.champ("source").and_then(Valeur::texte);
    let categorie = d.champ("category").and_then(Valeur::texte);
    let genre = match (source, categorie) {
        (Some("profile"), Some("idea")) => "Idée",
        (Some("profile"), Some("problem")) => "Problème",
        (Some("rating"), _) => "Après le bilan",
        (Some("chat_report"), _) => "Signalement",
        _ => "Retour",
    };
    match extrait(d.champ("message").and_then(Valeur::texte).unwrap_or("")) {
        Some(m) => format!("{genre} : {m}"),
        None => genre.into(),
    }
}

/// Le texte sur une ligne, coupé à `LONGUEUR_EXTRAIT` caractères ; `None` s'il est vide.
pub fn extrait(texte: &str) -> Option<String> {
    let ligne = texte.split_whitespace().collect::<Vec<_>>().join(" ");
    if ligne.is_empty() {
        return None;
    }
    if ligne.chars().count() > LONGUEUR_EXTRAIT {
        return Some(ligne.chars().take(LONGUEUR_EXTRAIT - 1).collect::<String>() + "…");
    }
    Some(ligne)
}

/// « 12 min », « 2 h », « 3 j ».
pub fn age(at: &Date, maintenant: &Date) -> String {
    let minutes = ((*maintenant - *at).num_seconds() / 60).max(1);
    if minutes < 60 {
        format!("{minutes} min")
    } else if minutes < 24 * 60 {
        format!("{} h", minutes / 60)
    } else {
        format!("{} j", minutes / (24 * 60))
    }
}

/// Chaque mot avec sa première lettre en capitale et le reste en minuscules (`capitalized` de Swift).
fn capitaliser(s: &str) -> String {
    let mut sortie = String::new();
    let mut debut = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if debut {
                sortie.extend(c.to_uppercase());
            } else {
                sortie.extend(c.to_lowercase());
            }
            debut = false;
        } else {
            sortie.push(c);
            debut = true;
        }
    }
    sortie
}

/// Ce que Roleo compte dans `aiLoad.m.<minute>` : par fournisseur `n`, `q` (429), `e` (autres échecs),
/// `ms` (latence cumulée) ; `tk` les tokens facturables, `f` les échecs vus par l'user, `v` les caractères
/// de voix par moteur.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Charge {
    pub tokens: i64,
    pub echecs: i64,
    pub fournisseurs: BTreeMap<String, Compteurs>,
    pub voix: BTreeMap<String, i64>,
    /// ms d'audio transcrites par la dictée Inworld (`s`).
    pub dictee_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Compteurs {
    pub n: i64,
    pub q: i64,
    pub e: i64,
    pub ms: i64,
}

impl Compteurs {
    fn ajoute(&mut self, c: &Compteurs) {
        self.n += c.n;
        self.q += c.q;
        self.e += c.e;
        self.ms += c.ms;
    }
}

impl Charge {
    pub fn ajoute(&mut self, c: &Charge) {
        self.tokens += c.tokens;
        self.echecs += c.echecs;
        self.dictee_ms += c.dictee_ms;
        for (f, x) in &c.fournisseurs {
            self.fournisseurs.entry(f.clone()).or_default().ajoute(x);
        }
        for (moteur, n) in &c.voix {
            *self.voix.entry(moteur.clone()).or_default() += n;
        }
    }

    /// Somme les minutes de `m` ; une clé dont la valeur est une carte est un fournisseur, sauf `v`.
    pub fn depuis(m: &Valeur) -> Res<Charge> {
        let Valeur::Carte(minutes) = m else { return Err(format_err("aiLoad.m n'est pas une carte")) };
        let mut charge = Charge::default();
        for minute in minutes.values() {
            let Valeur::Carte(champs) = minute else { return Err(format_err("une minute d'aiLoad n'est pas une carte")) };
            for (cle, v) in champs {
                if cle == "v" {
                    let Valeur::Carte(moteurs) = v else { return Err(format_err("aiLoad v n'est pas une carte")) };
                    for (moteur, n) in moteurs {
                        *charge.voix.entry(moteur.clone()).or_default() += compte(Some(n))?;
                    }
                } else if let Valeur::Carte(c) = v {
                    charge.fournisseurs.entry(cle.clone()).or_default().ajoute(&Compteurs {
                        n: compte(c.get("n"))?, q: compte(c.get("q"))?, e: compte(c.get("e"))?, ms: compte(c.get("ms"))?,
                    });
                } else if cle == "tk" {
                    charge.tokens += compte(Some(v))?;
                } else if cle == "f" {
                    charge.echecs += compte(Some(v))?;
                } else if cle == "s" {
                    charge.dictee_ms += compte(Some(v))?;
                }
            }
        }
        Ok(charge)
    }
}

fn compte(v: Option<&Valeur>) -> Res<i64> {
    match v {
        None => Ok(0),
        Some(Valeur::Entier(n)) => Ok(*n),
        Some(Valeur::Reel(x)) => Ok(*x as i64),
        _ => Err(format_err("un compteur d'aiLoad n'est pas un nombre")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Ligne;
    use crate::p;
    use firestore::tests::document;
    use std::collections::HashMap;

    const BASE: &str = "https://roleo-73820.web.app";

    fn d(s: &str) -> Date { jour::depuis_iso(s).unwrap() }

    fn valeur(store: &Store, metrique: &str, jour: &str, heure: i64) -> Option<f64> {
        store.db.query("select valeur from serie where business = 'roleo' and metrique = ? and jour = ? and heure = ?",
            p![metrique, jour, heure]).unwrap().first().and_then(|l| l.nombre("valeur"))
    }

    /// Un `aiLoad` réduit à deux minutes, à la forme de la prod.
    const AI_LOAD: &str = r#"
        {"name": "projects/roleo-73820/databases/(default)/documents/aiLoad/2026-11-12T23", "fields": {"m": {"mapValue": {"fields": {
          "05": {"mapValue": {"fields": {
            "mistral": {"mapValue": {"fields": {"n": {"integerValue": "10"}, "q": {"integerValue": "2"}, "e": {"integerValue": "1"},
                                                "ms": {"integerValue": "5000"}, "p": {"integerValue": "1"}}}},
            "tk": {"integerValue": "12000"}, "f": {"integerValue": "1"}, "w": {"integerValue": "300"},
            "v": {"mapValue": {"fields": {"voxtral3": {"integerValue": "1000"}}}}}}},
          "06": {"mapValue": {"fields": {
            "mistral": {"mapValue": {"fields": {"n": {"integerValue": "30"}, "q": {"integerValue": "20"}, "ms": {"integerValue": "15000"}}}},
            "anthropic": {"mapValue": {"fields": {"n": {"integerValue": "4"}, "ms": {"integerValue": "2400"}}}},
            "tk": {"integerValue": "8000"}, "s": {"integerValue": "1800000"},
            "v": {"mapValue": {"fields": {"voxtral3": {"integerValue": "500"}, "kokoro": {"doubleValue": 2000}}}}}}}}}}}}
        "#;

    fn voix(paires: &[(&str, i64)]) -> BTreeMap<String, i64> { paires.iter().map(|(k, v)| (k.to_string(), *v)).collect() }

    #[test]
    fn charge_somme_les_minutes() {
        let c = charge(Some(&document(AI_LOAD).unwrap())).unwrap();
        assert_eq!(c.tokens, 20000);
        assert_eq!(c.echecs, 1);
        assert_eq!(c.fournisseurs["mistral"], Compteurs { n: 40, q: 22, e: 1, ms: 20000 });
        assert_eq!(c.fournisseurs["anthropic"], Compteurs { n: 4, q: 0, e: 0, ms: 2400 });
        assert_eq!(c.fournisseurs.len(), 2);
        assert_eq!(c.voix, voix(&[("voxtral3", 1500), ("kokoro", 2000)]));
        assert_eq!(c.dictee_ms, 1_800_000);
        assert_eq!(charge(None).unwrap(), Charge::default());
        assert!(charge(Some(&document(r#"{"name": "a/aiLoad/x", "fields": {}}"#).unwrap())).is_err());
        let faux = r#"{"name": "a/aiLoad/x", "fields": {"m": {"mapValue": {"fields": {"05": {"mapValue": {"fields": {"tk": {"stringValue": "1"}}}}}}}}}"#;
        assert!(charge(Some(&document(faux).unwrap())).is_err());
        let voix_fausse = r#"{"name": "a/aiLoad/x", "fields": {"m": {"mapValue": {"fields": {"05": {"mapValue": {"fields": {"v": {"integerValue": "1"}}}}}}}}}"#;
        assert!(charge(Some(&document(voix_fausse).unwrap())).is_err());
    }

    #[test]
    fn heures_de_paris_en_ids_utc() {
        let h = heures(2, &d("2026-11-13T14:20:00Z"));
        assert_eq!(h.len(), 40);
        assert_eq!(id_heure(&h[0]), "2026-11-11T23");
        assert_eq!(id_heure(h.last().unwrap()), "2026-11-13T14");
        // passage à l'heure d'hiver : 25 heures UTC, 24 heures de Paris
        let hiver = heures(1, &d("2026-10-25T22:30:00Z"));
        assert_eq!(hiver.len(), 25);
        assert_eq!(hiver.iter().map(jour::heure).collect::<std::collections::HashSet<_>>().len(), 24);
        assert!(hiver.iter().all(|h| jour::cle(h) == "2026-10-25"));
    }

    #[test]
    fn series_ia_a_l_heure_de_paris() {
        let store = Store::memoire().unwrap();
        let c = charge(Some(&document(AI_LOAD).unwrap())).unwrap();
        let suite = Charge {
            tokens: 100, fournisseurs: BTreeMap::from([("anthropic".into(), Compteurs { n: 2, ms: 200, ..Default::default() })]),
            ..Default::default()
        };
        let h = [(d("2026-11-12T23:00:00Z"), c), (d("2026-11-13T00:00:00Z"), suite)];
        let lien = format!("{BASE}/ia/");
        let n = ecrire_charges(&h, "roleo", &lien, &store).unwrap();

        let j = "2026-11-13";
        let v = |m: &str, heure: i64| valeur(&store, m, j, heure);
        assert_eq!(v(metrique::TOKENS_IA, -1), Some(20100.0));
        assert_eq!(valeur(&store, metrique::TOKENS_IA, "2026-11-12", -1), None);
        assert_eq!(v(metrique::ECHECS_IA, -1), Some(1.0));
        assert_eq!(v(metrique::COUT_TEXTE, -1), Some(cout::euros_tokens(20100.0)));
        let cout_voix = cout::euros_voix(&voix(&[("voxtral3", 1500), ("kokoro", 2000)])).unwrap() + cout::euros_dictee(1_800_000.0);
        assert_eq!(v(metrique::COUT_VOIX, -1), Some(cout_voix));
        assert_eq!(v(metrique::COUT_IA, -1), Some(cout::euros_tokens(20100.0) + cout_voix));
        assert_eq!(v(&metrique::requetes_ia("mistral"), 0), Some(40.0));
        assert_eq!(v(&metrique::requetes_ia("mistral"), 1), Some(0.0));
        assert_eq!(v(&metrique::requetes_ia("mistral"), -1), Some(40.0));
        assert_eq!(v(&metrique::q429_ia("mistral"), -1), Some(22.0));
        assert_eq!(v(&metrique::erreurs_ia("mistral"), 0), Some(1.0));
        assert_eq!(v(&metrique::latence_ia("mistral"), 0), Some(500.0));
        assert_eq!(v(&metrique::latence_ia("mistral"), 1), None);
        assert_eq!(v(&metrique::requetes_ia("anthropic"), -1), Some(6.0));
        assert_eq!(v(&metrique::latence_ia("anthropic"), -1), Some(2600.0 / 6.0));

        let fil = store.db.query("select id, at, type, texte, lien from fil", p![]).unwrap();
        assert_eq!(fil.len(), 1);
        assert_eq!(fil[0].texte("id").as_deref(), Some("firebase:ia:mistral:2026-11-12T23"));
        assert_eq!(fil[0].date("at"), Some(d("2026-11-12T23:00:00Z")));
        assert_eq!(fil[0].texte("type").as_deref(), Some("IA"));
        assert_eq!(fil[0].texte("texte").as_deref(), Some("Mistral, 22 erreurs 429 et 1 échec cette heure"));
        assert_eq!(fil[0].texte("lien"), Some(lien.clone()));
        // 4 séries du jour et la voix, 2 fournisseurs × 3 périodes, 4 séries sauf mistral à 1 h (3), 1 ligne du fil
        assert_eq!(n, 5 + 4 + 4 + 3 + 4 + 4 + 4 + 1);

        assert_eq!(ecrire_charges(&h, "roleo", &lien, &store).unwrap(), n - 1);
    }

    #[test]
    fn alertes() {
        let c = |n, q, e| Compteurs { n, q, e, ms: 0 };
        assert_eq!(alerte("mistral", &c(100, 19, 0)), None);
        assert_eq!(alerte("mistral", &c(100, 20, 0)).as_deref(), Some("Mistral, 20 erreurs 429 cette heure"));
        assert_eq!(alerte("mistral", &c(100, 0, 25)).as_deref(), Some("Mistral, 25 échecs cette heure"));
        assert_eq!(alerte("nouveau", &c(100, 1, 19)).as_deref(), Some("Nouveau, 1 erreur 429 et 19 échecs cette heure"));
        assert_eq!(alerte("deepinfra", &c(6, 0, 6)).as_deref(), Some("DeepInfra ne répond plus, 6 requêtes en échec cette heure"));
        assert_eq!(alerte("anthropic", &c(4, 0, 4)), None);
    }

    #[test]
    fn jour_sans_voix_garde_le_cout_releve() {
        let store = Store::memoire().unwrap();
        store.serie("roleo", metrique::COUT_VOIX, "2026-11-13", -1, 4.0).unwrap();
        let h = [(d("2026-11-13T09:00:00Z"), Charge { tokens: 1_000_000, ..Default::default() })];
        ecrire_charges(&h, "roleo", &format!("{BASE}/ia/"), &store).unwrap();
        assert_eq!(valeur(&store, metrique::COUT_VOIX, "2026-11-13", -1), Some(4.0));
        assert_eq!(valeur(&store, metrique::COUT_IA, "2026-11-13", -1), Some(4.0 + cout::euros_tokens(1_000_000.0)));
        // sans aucune part, pas de total
        store.somme("roleo", metrique::COUT_IA, &[metrique::COUT_TEXTE.into(), metrique::COUT_VOIX.into()], "2026-09-01").unwrap();
        assert_eq!(valeur(&store, metrique::COUT_IA, "2026-09-01", -1), None);
    }

    fn message(uid: &str, at: &str, apercu: &str) -> Document {
        document(&format!(r#"
            {{"name": "projects/p/databases/(default)/documents/users/{uid}", "fields": {{
              "supportLastAt": {{"timestampValue": "{at}"}}, "supportPreview": {{"stringValue": "{apercu}"}}}}}}
            "#)).unwrap()
    }

    fn retour(id: &str, source: &str, categorie: Option<&str>, texte: &str, at: &str) -> Document {
        let cat = categorie.map(|c| format!(r#"{{"stringValue": "{c}"}}"#)).unwrap_or_else(|| r#"{"nullValue": null}"#.into());
        document(&format!(r#"
            {{"name": "projects/p/databases/(default)/documents/feedback/{id}", "fields": {{
              "source": {{"stringValue": "{source}"}}, "category": {cat}, "message": {{"stringValue": "{texte}"}},
              "createdAt": {{"timestampValue": "{at}"}}}}}}
            "#)).unwrap()
    }

    fn taches(store: &Store) -> HashMap<String, Ligne> {
        store.db.query("select id, texte, etiquette, ton, lien, fait_le from a_traiter where source = 'firebase'", p![]).unwrap()
            .into_iter().map(|l| (l.texte("id").unwrap(), l)).collect()
    }

    #[test]
    fn messages_et_retours_en_taches() {
        let store = Store::memoire().unwrap();
        let maintenant = d("2026-11-13T12:00:00Z");
        let messages = [message("u1", "2026-11-13T09:59:59.250000Z", r"Bonjour,\n  l'appli plante"),
                        message("u2", "2026-11-10T09:00:00Z", "")];
        let retours = [retour("r1", "profile", Some("problem"), "Le micro coupe", "2026-11-13T11:48:00Z"),
                       retour("r2", "rating", None, "", "2026-11-12T12:00:00Z"),
                       retour("r3", "chat_report", Some("other"), &"a".repeat(200), "2026-11-13T11:59:50Z")];
        let n = ecrire_messages(&messages, &retours, "roleo", BASE, &maintenant, &store).unwrap();
        assert_eq!(n, 5 + 5 + 2);

        let t = taches(&store);
        assert_eq!(t.len(), 5);
        let u1 = &t["firebase:msg:u1:2026-11-13T09:59:59Z"];
        assert_eq!(u1.texte("texte").as_deref(), Some("Bonjour, l'appli plante"));
        assert_eq!(u1.texte("etiquette").as_deref(), Some("2 h"));
        assert_eq!(u1.texte("ton").as_deref(), Some("neutre"));
        assert_eq!(u1.texte("lien"), Some(format!("{BASE}/messages/")));
        let u2 = &t["firebase:msg:u2:2026-11-10T09:00:00Z"];
        assert_eq!(u2.texte("texte").as_deref(), Some("Nouveau message"));
        assert_eq!(u2.texte("etiquette").as_deref(), Some("3 j"));
        assert_eq!(u2.texte("ton").as_deref(), Some("alerte"));
        assert_eq!(t["firebase:retour:r1"].texte("texte").as_deref(), Some("Problème : Le micro coupe"));
        assert_eq!(t["firebase:retour:r1"].texte("etiquette").as_deref(), Some("12 min"));
        assert_eq!(t["firebase:retour:r1"].texte("lien"), Some(format!("{BASE}/feedback/")));
        assert_eq!(t["firebase:retour:r2"].texte("texte").as_deref(), Some("Après le bilan"));
        assert_eq!(t["firebase:retour:r2"].texte("etiquette").as_deref(), Some("1 j"));
        let r3 = t["firebase:retour:r3"].texte("texte").unwrap();
        assert_eq!(r3.chars().count(), "Signalement : ".chars().count() + 80);
        assert!(r3.ends_with('…'));
        assert_eq!(t["firebase:retour:r3"].texte("etiquette").as_deref(), Some("1 min"));

        let fil = store.db.query("select id, type, at from fil order by id", p![]).unwrap();
        assert_eq!(fil.iter().map(|l| l.texte("id").unwrap()).collect::<Vec<_>>(),
                   ["firebase:msg:u1:2026-11-13T09:59:59Z", "firebase:msg:u2:2026-11-10T09:00:00Z",
                    "firebase:retour:r1", "firebase:retour:r2", "firebase:retour:r3"]);
        assert_eq!(fil.iter().map(|l| l.texte("type").unwrap()).collect::<Vec<_>>(), ["MSG", "MSG", "RETOUR", "RETOUR", "RETOUR"]);
        assert_eq!(fil.last().unwrap().date("at"), Some(d("2026-11-13T11:59:50Z")));
        assert_eq!(valeur(&store, metrique::MESSAGES_OUVERTS, "2026-11-13", -1), Some(2.0));
        assert_eq!(valeur(&store, metrique::RETOURS_NOUVEAUX, "2026-11-13", -1), Some(3.0));

        // u1 réécrit, u2 a sa réponse, r1 et r2 sont traités
        let plus_tard = d("2026-11-13T13:00:00Z");
        let n2 = ecrire_messages(&[message("u1", "2026-11-13T12:30:00Z", "Toujours là")], &retours[2..], "roleo", BASE,
                                 &plus_tard, &store).unwrap();
        assert_eq!(n2, 2 + 1 + 2);
        let t2 = taches(&store);
        assert_eq!(t2["firebase:msg:u1:2026-11-13T12:30:00Z"].texte("fait_le"), None);
        assert!(t2["firebase:msg:u1:2026-11-13T09:59:59Z"].texte("fait_le").is_some());
        assert!(t2["firebase:msg:u2:2026-11-10T09:00:00Z"].texte("fait_le").is_some());
        assert!(t2["firebase:retour:r1"].texte("fait_le").is_some());
        assert_eq!(t2["firebase:retour:r3"].texte("fait_le"), None);
        assert_eq!(t2["firebase:retour:r3"].texte("etiquette").as_deref(), Some("1 h"));
        assert_eq!(valeur(&store, metrique::MESSAGES_OUVERTS, "2026-11-13", -1), Some(1.0));
        assert_eq!(valeur(&store, metrique::RETOURS_NOUVEAUX, "2026-11-13", -1), Some(1.0));
    }

    #[test]
    fn message_sans_date_leve() {
        let store = Store::memoire().unwrap();
        let m = document(r#"{"name": "a/users/u", "fields": {"supportPreview": {"stringValue": "x"}}}"#).unwrap();
        assert!(ecrire_messages(&[m], &[], "roleo", BASE, &jour::maintenant(), &store).is_err());
    }

    #[test]
    fn ages() {
        let t = d("2026-11-13T12:00:00Z");
        let plus = |s: i64| t + chrono::Duration::seconds(s);
        assert_eq!(age(&t, &plus(30)), "1 min");
        assert_eq!(age(&t, &plus(59 * 60)), "59 min");
        assert_eq!(age(&t, &plus(3600)), "1 h");
        assert_eq!(age(&t, &plus(24 * 3600 - 60)), "23 h");
        assert_eq!(age(&t, &plus(24 * 3600)), "1 j");
    }
}
