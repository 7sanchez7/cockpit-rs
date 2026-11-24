//! MRR, abonnés, essais, revenu, achats, fil ACHAT et paiement des installs convertis, par l'API v2 de RevenueCat
//! en lecture seule.
//! Voir docs/sources/revenuecat.md. Jamais l'API v1 `GET /subscribers/{id}` : elle crée le client qu'elle ne trouve pas.
use crate::business::Business;
use crate::collecte::http::decoder;
use crate::collecte::{Contexte, Requete, Source};
use crate::erreur::{format_err, Erreur, Res};
use crate::jour::{self, Date};
use crate::metrique::{self, Boutique};
use crate::modeles::{EtatPaiement, Evenement, TypeFil};
use crate::sources::pays_fr;
use crate::store::Store;
use async_trait::async_trait;
use chrono::{DateTime, Duration};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

pub struct RevenueCat;

pub const BASE: &str = "https://api.revenuecat.com/v2";
pub const NOM_CLE: &str = "REVENUECAT_SECRET_KEY_V2";
/// Jours relus dans les graphiques : 90 au premier passage, puis les derniers qui se consolident.
const HISTORIQUE: i64 = 90;
const RELECTURE: i64 = 7;
/// Jours du revenu glissant, comme l'overview.
const FENETRE_REVENU: i64 = 28;
/// Achats cherchés au premier passage, en heures.
const PREMIERS_ACHATS: i64 = 3 * 24;
/// Un essai dure au plus 7 jours : le premier paiement tombe au plus 8 jours après le début de l'abonnement.
const ESSAI_MAX: i64 = 8;
/// Les convertis de la table `install` : lus une fois, puis relus toutes les 6 h tant que l'essai court ou qu'ils
/// n'ont pas payé, sur les installs des 40 derniers jours ; au plus 500 par passage.
const PAIEMENTS_PAR_PASSAGE: i64 = 500;
const RELECTURE_PAIEMENT: i64 = 6;
const SUIVI_PAIEMENT: i64 = 40;

/// Jour → valeur.
pub type Serie = HashMap<String, f64>;
/// Jour → (nom de la mesure → valeur).
pub type Jours = HashMap<String, HashMap<String, f64>>;

#[async_trait]
impl Source for RevenueCat {
    fn id(&self) -> &'static str { "revenuecat" }
    fn nom(&self) -> &'static str { "RevenueCat" }
    fn cadence(&self) -> Option<i64> { Some(30 * 60) }

    async fn collecte(&self, ctx: &Contexte) -> Res<usize> {
        let dernier = ctx.store.dernier_succes(self.id())?;
        let mut lignes = 0;
        let mut configures = 0;
        for b in &ctx.businesses {
            let cle = match ctx.secrets.valeur(NOM_CLE, b).await {
                Ok(c) => c,
                Err(Erreur::NonConfigure(_)) => continue,
                Err(e) => return Err(nomme(e, b)),
            };
            configures += 1;
            lignes += collecte(b, &cle, dernier, ctx).await.map_err(|e| nomme(e, b))?;
        }
        if configures == 0 {
            return Err(Erreur::NonConfigure(format!("aucun business avec {NOM_CLE}")));
        }
        Ok(lignes)
    }
}

async fn collecte(b: &Business, cle: &str, dernier: Option<Date>, ctx: &Contexte) -> Res<usize> {
    let lire = |chemin: String| {
        let r = Requete::get(format!("{BASE}{chemin}")).jeton(cle);
        async move { ctx.http.donnees(&r).await }
    };
    let projet = projet(&lire("/projects".into()).await?, &b.nom)?;
    let apercu = apercu(&lire(format!("/projects/{projet}/metrics/overview")).await?)?;
    let aujourdhui = jour::aujourdhui();
    let mut complet = dernier.is_some();
    for m in [metrique::REVENU_28J, metrique::REVENU, metrique::NOUVEAUX_CLIENTS] {
        let suivies = std::iter::once(m.to_string()).chain(Boutique::TOUTES.iter().map(|s| s.metrique(m)));
        for s in suivies {
            complet = complet && !ctx.store.historique_incomplet(&b.id, &s, HISTORIQUE, &aujourdhui)?;
        }
    }
    let debut = jour::decale(&aujourdhui, -(if complet { RELECTURE } else { HISTORIQUE }));
    let graphique = |nom: &str, depuis: &str, segment: &str| {
        lire(format!("/projects/{projet}/charts/{nom}?resolution=0&start_date={depuis}&end_date={aujourdhui}&segment={segment}"))
    };
    let g = Graphiques {
        mrr: graphique("mrr", &debut, "store").await?,
        abonnes: graphique("actives", &debut, "store").await?,
        essais: graphique("trials", &debut, "store").await?,
        achats: graphique("actives_new", &debut, "store").await?,
        revenu: graphique("revenue", &jour::decale(&debut, 1 - FENETRE_REVENU), "store").await?,
        nouveaux: graphique("customers_new", &debut, "first_platform").await?,
    };
    let mut points_lus = Vec::new();
    for boutique in [None, Some(Boutique::AppStore), Some(Boutique::PlayStore)] {
        points_lus.extend(points(&g, &apercu, boutique, &debut, &aujourdhui)?);
    }
    ecrire(&points_lus, &b.id, &ctx.store)?;
    let mut lignes = points_lus.len();

    // Fil ACHAT : la v2 ne liste pas les transactions d'un projet. On part des comptes payants que la Function
    // de Roleo vient de relire (webhook RevenueCat) dans Firestore, puis on lit leurs abonnements récents.
    if let Some(firebase) = &b.config.firebase_project {
        let depuis = dernier.map_or_else(|| jour::maintenant() - Duration::hours(PREMIERS_ACHATS), |d| d - Duration::hours(2));
        let url = format!("https://firestore.googleapis.com/v1/projects/{firebase}/databases/(default)/documents:runQuery");
        let r = Requete::new(url, "POST")
            .jeton(&ctx.gcloud.jeton().await?)
            .entete("x-goog-user-project", firebase)
            .json(&requete_comptes(&jour::maintenant()));
        let comptes = ctx.http.donnees(&r).await?;
        for uid in comptes_payes(&comptes, &depuis)? {
            let reponse = match lire(format!("/projects/{projet}/customers/{uid}/subscriptions?limit=50")).await {
                Ok(d) => d,
                Err(Erreur::Http(404, _)) => continue,
                Err(e) => return Err(e),
            };
            for a in abonnements(&reponse, &(depuis - Duration::days(ESSAI_MAX)))? {
                let id_fil = format!("revenuecat:achat:{}", a.id);
                if !ctx.store.db.query("select 1 from fil where id = ?", crate::p![id_fil.as_str()])?.is_empty() {
                    continue;
                }
                let transactions =
                    lire(format!("/projects/{projet}/subscriptions/{}/transactions?sort=purchased_at&direction=asc&limit=20", a.id)).await?;
                if let Some(e) = achat(&id_fil, &a, &transactions, &b.id)? {
                    if ctx.store.fil(&e)? {
                        lignes += 1;
                    }
                }
            }
        }
    }

    // Paiements : parmi les installs que PostHog a vus convertir, qui a vraiment payé.
    let relu = jour::maintenant() - Duration::hours(RELECTURE_PAIEMENT);
    let clients = ctx.store.clients_a_verifier(&b.id, &jour::decale(&aujourdhui, -SUIVI_PAIEMENT), &relu, PAIEMENTS_PAR_PASSAGE)?;
    for client in clients {
        let etat = match lire(format!("/projects/{projet}/customers/{}/subscriptions?limit=50", encoder_client(&client))).await {
            Ok(d) => paiement(&d)?,
            Err(Erreur::Http(404, _)) => EtatPaiement::Inconnu,
            Err(e) => return Err(e),
        };
        ctx.store.paiement(&b.id, &client, etat, &jour::maintenant())?;
        lignes += 1;
    }
    Ok(lignes)
}

/// Le client en chemin d'URL : lettres, chiffres, `-` et `_` gardés, le reste en `%XX`.
fn encoder_client(client: &str) -> String {
    let mut r = String::new();
    for c in client.chars() {
        if c.is_alphanumeric() || c == '-' || c == '_' {
            r.push(c);
        } else {
            let mut o = [0; 4];
            for x in c.encode_utf8(&mut o).bytes() {
                r.push_str(&format!("%{x:02X}"));
            }
        }
    }
    r
}

/// Le message d'erreur nomme le business (`Roleo : HTTP 401 …`).
pub fn nomme(e: Erreur, b: &Business) -> Erreur {
    match e {
        Erreur::NonConfigure(m) => Erreur::NonConfigure(format!("{} : {m}", b.nom)),
        Erreur::Http(c, m) => Erreur::Http(c, format!("{} : {m}", b.nom)),
        Erreur::Format(m) => Erreur::Format(format!("{} : {m}", b.nom)),
        Erreur::Commande(m) => Erreur::Commande(format!("{} : {m}", b.nom)),
        e => e,
    }
}

// décodage

/// L'id du projet RevenueCat : celui qui porte le nom du business, sinon le seul.
pub fn projet(data: &[u8], nom: &str) -> Res<String> {
    #[derive(Deserialize)]
    struct Reponse { items: Vec<P> }
    #[derive(Deserialize)]
    struct P { id: String, name: String }
    let items = decoder::<Reponse>(data)?.items;
    let p = items.iter().find(|p| p.name == nom).or(if items.len() == 1 { items.first() } else { None });
    p.map(|p| p.id.clone()).ok_or_else(|| format_err(format!("aucun projet RevenueCat nommé {nom}")))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Apercu {
    pub mrr: f64,
    pub abonnes: f64,
    pub essais: f64,
    pub revenu28j: f64,
}

pub fn apercu(data: &[u8]) -> Res<Apercu> {
    #[derive(Deserialize)]
    struct Reponse { metrics: Vec<M> }
    #[derive(Deserialize)]
    struct M { id: String, value: f64 }
    let mut v: HashMap<String, f64> = HashMap::new();
    for m in decoder::<Reponse>(data)?.metrics {
        v.entry(m.id).or_insert(m.value);
    }
    match (v.get("mrr"), v.get("active_subscriptions"), v.get("active_trials"), v.get("revenue")) {
        (Some(&mrr), Some(&abonnes), Some(&essais), Some(&revenu28j)) => Ok(Apercu { mrr, abonnes, essais, revenu28j }),
        _ => Err(format_err("overview sans mrr, active_subscriptions, active_trials ou revenue")),
    }
}

/// Un graphique à la journée : jour → (nom de la mesure → valeur). `cohort` est minuit UTC du jour, en secondes.
/// Demandé avec `segment=store`, il se lit pour le total (`boutique` nul) ou pour un store, à 0 les jours où le store
/// n'a pas de segment (Play n'en a qu'avec ses premières données).
pub fn graphique(data: &[u8], boutique: Option<Boutique>) -> Res<Jours> {
    #[derive(Deserialize)]
    struct Reponse { measures: Vec<Mesure>, segments: Option<Vec<Segment>>, values: Vec<Point> }
    #[derive(Deserialize)]
    struct Mesure { display_name: String }
    #[derive(Deserialize)]
    struct Segment { display_name: String, is_total: bool }
    #[derive(Deserialize)]
    struct Point { cohort: f64, measure: i64, segment: Option<i64>, value: Option<f64> }
    let r = decoder::<Reponse>(data)?;
    let segments = r.segments.unwrap_or_default();
    let segment = |i: i64| usize::try_from(i).ok().and_then(|i| segments.get(i));
    let lus = |garde: &dyn Fn(Option<i64>) -> bool| -> Res<Jours> {
        let mut jours = Jours::new();
        for p in &r.values {
            let Some(m) = usize::try_from(p.measure).ok().and_then(|m| r.measures.get(m)) else { continue };
            let Some(v) = p.value.filter(|_| garde(p.segment)) else { continue };
            jours.entry(jour::cle(&instant(p.cohort)?)).or_default().insert(m.display_name.clone(), v);
        }
        Ok(jours)
    };
    let total = lus(&|i| i.is_none_or(|i| segment(i).is_some_and(|s| s.is_total)))?;
    let Some(boutique) = boutique else { return Ok(total) };
    let part = lus(&|i| {
        i.and_then(segment).is_some_and(|s| !s.is_total && self::boutique(&s.display_name) == Some(boutique))
    })?;
    Ok(total
        .into_iter()
        .map(|(j, mesures)| {
            let p = mesures.keys().map(|k| (k.clone(), part.get(&j).and_then(|x| x.get(k)).copied().unwrap_or(0.0))).collect();
            (j, p)
        })
        .collect())
}

/// Des secondes Unix (avec décimales) en date.
fn instant(secondes: f64) -> Res<Date> {
    DateTime::from_timestamp_millis((secondes * 1000.0) as i64).ok_or_else(|| format_err(format!("instant illisible : {secondes}")))
}

/// Le store d'un segment, par son nom affiché : `App Store` ou `iOS`, `Play Store` ou `Android` (`first_platform`,
/// seul découpage des nouveaux clients) ; les autres segments sont ignorés.
pub fn boutique(segment: &str) -> Option<Boutique> {
    let n = segment.to_lowercase();
    if n == "app store" || n == "ios" {
        return Some(Boutique::AppStore);
    }
    (n.contains("play") || n == "android").then_some(Boutique::PlayStore)
}

/// Les six graphiques d'un passage, bruts : segmentés par store, les nouveaux clients par première plateforme.
pub struct Graphiques {
    pub mrr: Vec<u8>,
    pub abonnes: Vec<u8>,
    pub essais: Vec<u8>,
    pub achats: Vec<u8>,
    pub revenu: Vec<u8>,
    pub nouveaux: Vec<u8>,
}

/// Les séries d'un segment, jour par jour.
pub struct Series {
    pub mrr: Serie,
    pub abonnes: Serie,
    pub essais: Serie,
    pub achats: Serie,
    pub revenu: Serie,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Point {
    pub metrique: String,
    pub jour: String,
    pub valeur: f64,
}

fn point(metrique: &str, jour: &str, valeur: f64) -> Point { Point { metrique: metrique.into(), jour: jour.into(), valeur } }

/// Les points du total (`boutique` nul) ou d'un store, aux métriques suffixées (`mrr_brut.play_store`). L'overview
/// ne se découpe pas : aujourd'hui, un store en prend la part qu'il a dans les graphiques, les stores somment au total.
pub fn points(g: &Graphiques, apercu: &Apercu, boutique: Option<Boutique>, debut: &str, aujourdhui: &str) -> Res<Vec<Point>> {
    let series = |b: Option<Boutique>| -> Res<Series> {
        Ok(Series {
            mrr: mesure("MRR", &graphique(&g.mrr, b)?)?,
            abonnes: mesure("Actives", &graphique(&g.abonnes, b)?)?,
            essais: mesure("Active Trials", &graphique(&g.essais, b)?)?,
            achats: achats(&graphique(&g.achats, b)?)?,
            revenu: mesure("Revenue", &graphique(&g.revenu, b)?)?,
        })
    };
    let s = series(boutique)?;
    let mut a = *apercu;
    if boutique.is_some() {
        let t = series(None)?;
        let part = |store: &Serie, total: &Serie, jours: &[String]| -> f64 {
            let d: f64 = jours.iter().filter_map(|j| total.get(j)).sum();
            if d > 0.0 { jours.iter().filter_map(|j| store.get(j)).sum::<f64>() / d } else { 0.0 }
        };
        let jour_ = [aujourdhui.to_string()];
        a = Apercu {
            mrr: apercu.mrr * part(&s.mrr, &t.mrr, &jour_),
            abonnes: apercu.abonnes * part(&s.abonnes, &t.abonnes, &jour_),
            essais: apercu.essais * part(&s.essais, &t.essais, &jour_),
            revenu28j: apercu.revenu28j * part(&s.revenu, &t.revenu, &jour::derniers(FENETRE_REVENU, aujourdhui)),
        };
    }
    let nouveaux = mesure("New Customers", &graphique(&g.nouveaux, boutique)?)?;
    let mut p = points_des_series(&a, &s, &revenus28j(&s.revenu, debut, aujourdhui), aujourdhui);
    p.extend(nouveaux.iter().map(|(j, v)| point(metrique::NOUVEAUX_CLIENTS, j, *v)));
    let Some(boutique) = boutique else { return Ok(p) };
    Ok(p.into_iter().map(|x| Point { metrique: boutique.metrique(&x.metrique), ..x }).collect())
}

/// Une mesure jour par jour ; lève si RevenueCat l'a renommée.
pub fn mesure(nom: &str, graphique: &Jours) -> Res<Serie> {
    let jours: Serie = graphique.iter().filter_map(|(j, m)| m.get(nom).map(|v| (j.clone(), *v))).collect();
    if jours.len() != graphique.len() {
        return Err(format_err(format!("mesure « {nom} » absente du graphique")));
    }
    Ok(jours)
}

/// Nouveaux abonnements payés du jour (fins d'essai, offres, achats directs, retours), sans les changements de produit.
pub fn achats(graphique: &Jours) -> Res<Serie> {
    let total = mesure("Total Paid Subscriptions", graphique)?;
    let changements = mesure("Product Changes", graphique)?;
    Ok(total.into_iter().map(|(j, v)| {
        let c = changements.get(&j).copied().unwrap_or(0.0);
        (j, v - c)
    }).collect())
}

/// Le revenu des 28 jours finissant à chaque jour de `debut` à la veille d'`aujourdhui`, quand le graphique les a tous.
pub fn revenus28j(revenu: &Serie, debut: &str, aujourdhui: &str) -> Serie {
    let mut sommes = Serie::new();
    let mut j = debut.to_string();
    while j.as_str() < aujourdhui {
        let fenetre: Vec<f64> = jour::derniers(FENETRE_REVENU, &j).iter().filter_map(|x| revenu.get(x).copied()).collect();
        if fenetre.len() as i64 == FENETRE_REVENU {
            sommes.insert(j.clone(), fenetre.iter().sum());
        }
        j = jour::decale(&j, 1);
    }
    sommes
}

/// Les jours passés viennent des graphiques, aujourd'hui de l'overview ; `achats` et `revenu` du graphique jusqu'à
/// aujourd'hui.
pub fn points_des_series(apercu: &Apercu, s: &Series, revenu28j: &Serie, aujourdhui: &str) -> Vec<Point> {
    let mut p = Vec::new();
    for (j, v) in s.mrr.iter().filter(|(j, _)| j.as_str() < aujourdhui) {
        p.push(point(metrique::MRR_BRUT, j, *v));
        p.push(point(metrique::MRR_NET, j, v * metrique::PART_NETTE));
    }
    let passes = |serie: &Serie, m: &str, p: &mut Vec<Point>| {
        p.extend(serie.iter().filter(|(j, _)| j.as_str() < aujourdhui).map(|(j, v)| point(m, j, *v)));
    };
    passes(&s.abonnes, metrique::ABONNES, &mut p);
    passes(&s.essais, metrique::ESSAIS_ACTIFS, &mut p);
    p.extend(s.achats.iter().map(|(j, v)| point(metrique::ACHATS, j, *v)));
    p.extend(s.revenu.iter().map(|(j, v)| point(metrique::REVENU, j, *v)));
    passes(revenu28j, metrique::REVENU_28J, &mut p);
    p.extend([
        point(metrique::MRR_BRUT, aujourdhui, apercu.mrr),
        point(metrique::MRR_NET, aujourdhui, apercu.mrr * metrique::PART_NETTE),
        point(metrique::ABONNES, aujourdhui, apercu.abonnes),
        point(metrique::ESSAIS_ACTIFS, aujourdhui, apercu.essais),
        point(metrique::REVENU_28J, aujourdhui, apercu.revenu28j),
    ]);
    p
}

pub fn ecrire(points: &[Point], business: &str, store: &Store) -> Res<()> {
    store.lot(|| {
        for p in points {
            store.serie(business, &p.metrique, &p.jour, -1, p.valeur)?;
        }
        Ok(())
    })
}

// achats

/// Les comptes dont le droit premium court encore (`users`, une centaine de documents).
pub fn requete_comptes(maintenant: &Date) -> Value {
    json!({"structuredQuery": {
        "from": [{"collectionId": "users"}],
        "select": {"fields": [{"fieldPath": "premiumTrial"}, {"fieldPath": "entitlementCheckedAt"}]},
        "where": {"fieldFilter": {
            "field": {"fieldPath": "premiumUntil"}, "op": "GREATER_THAN",
            "value": {"timestampValue": jour::iso(maintenant)},
        }},
    }})
}

/// Les uid de ces comptes hors essai que la Function a relus depuis `depuis` : un achat les fait relire.
pub fn comptes_payes(data: &[u8], depuis: &Date) -> Res<Vec<String>> {
    #[derive(Deserialize)]
    struct Ligne { document: Option<Document> }
    #[derive(Deserialize)]
    struct Document { name: String, fields: Option<Champs> }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Champs { premium_trial: Option<Valeur>, entitlement_checked_at: Option<Valeur> }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Valeur { timestamp_value: Option<String>, boolean_value: Option<bool> }
    let limite = *depuis - Duration::seconds(1);
    Ok(decoder::<Vec<Ligne>>(data)?
        .into_iter()
        .filter_map(|l| l.document)
        .filter_map(|d| {
            let champs = d.fields.as_ref();
            if champs.and_then(|c| c.premium_trial.as_ref()).and_then(|v| v.boolean_value) == Some(true) {
                return None;
            }
            // Firestore donne des microsecondes, on s'arrête à la seconde comme le Swift
            let releve = champs?.entitlement_checked_at.as_ref()?.timestamp_value.as_ref()?;
            let le = jour::depuis_iso(&format!("{}Z", releve.chars().take(19).collect::<String>()))?;
            (le >= limite).then(|| d.name.rsplit('/').next().map(String::from)).flatten()
        })
        .collect())
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Abonnement {
    pub id: String,
    pub starts_at: f64,
    pub environment: String,
    pub country: Option<String>,
}

/// Les abonnements de production commencés depuis `depuis` (`starts_at` en millisecondes).
pub fn abonnements(data: &[u8], depuis: &Date) -> Res<Vec<Abonnement>> {
    #[derive(Deserialize)]
    struct Reponse { items: Vec<Abonnement> }
    let seuil = depuis.timestamp_millis() as f64 / 1000.0;
    Ok(decoder::<Reponse>(data)?.items.into_iter().filter(|a| a.environment == "production" && a.starts_at / 1000.0 >= seuil).collect())
}

/// Payé dès qu'un abonnement de production a rapporté, en essai si l'un l'est encore, sinon non ;
/// inconnu sans abonnement de production.
pub fn paiement(data: &[u8]) -> Res<EtatPaiement> {
    #[derive(Deserialize)]
    struct Reponse { items: Vec<A> }
    #[derive(Deserialize)]
    struct A { status: String, environment: String, total_revenue_in_usd: Option<Montant> }
    #[derive(Deserialize)]
    struct Montant { gross: f64 }
    let production: Vec<A> = decoder::<Reponse>(data)?.items.into_iter().filter(|a| a.environment == "production").collect();
    Ok(if production.is_empty() {
        EtatPaiement::Inconnu
    } else if production.iter().any(|a| a.total_revenue_in_usd.as_ref().is_some_and(|m| m.gross > 0.0)) {
        EtatPaiement::Paye
    } else if production.iter().any(|a| a.status == "trialing") {
        EtatPaiement::Essai
    } else {
        EtatPaiement::Non
    })
}

/// Le premier paiement d'un abonnement, nul tant qu'il est en essai. Une transaction à 0 avant lui = un essai.
pub fn achat(id: &str, a: &Abonnement, transactions: &[u8], business: &str) -> Res<Option<Evenement>> {
    #[derive(Deserialize)]
    struct Reponse { items: Vec<T> }
    #[derive(Deserialize)]
    struct T { product_store_identifier: Option<String>, purchased_at: f64, revenue_in_local_currency: Montant }
    #[derive(Deserialize)]
    struct Montant { currency: String, gross: f64 }
    let mut items = decoder::<Reponse>(transactions)?.items;
    items.sort_by(|x, y| x.purchased_at.total_cmp(&y.purchased_at));
    let Some(i) = items.iter().position(|t| t.revenue_in_local_currency.gross > 0.0) else { return Ok(None) };
    let t = &items[i];
    let mut morceaux = vec![
        formule(t.product_store_identifier.as_deref().unwrap_or("")) + if i > 0 { " après essai" } else { "" },
        prix(t.revenue_in_local_currency.gross, &t.revenue_in_local_currency.currency),
    ];
    morceaux.extend(a.country.as_deref().and_then(pays_fr::nom).map(String::from));
    Ok(Some(Evenement {
        id: id.into(),
        business: business.into(),
        at: instant(t.purchased_at / 1000.0)?,
        type_: TypeFil::Achat,
        texte: morceaux.join(", "),
        lien: None,
    }))
}

/// Un montant à la française, comme `formatted(.currency(code:).locale(fr_FR))` : `1 069,99 €`, `29,99 $CA`.
pub fn prix(montant: f64, devise: &str) -> String {
    let decimales = if matches!(devise, "JPY" | "KRW") { 0 } else { 2 };
    let texte = format!("{:.*}", decimales, montant.abs());
    let (entier, fraction) = texte.split_once('.').unwrap_or((&texte, ""));
    let mut groupes: Vec<&str> = Vec::new();
    let mut fin = entier.len();
    while fin > 3 {
        groupes.push(&entier[fin - 3..fin]);
        fin -= 3;
    }
    groupes.push(&entier[..fin]);
    groupes.reverse();
    let mut nombre = groupes.join("\u{202F}");
    if !fraction.is_empty() {
        nombre = format!("{nombre},{fraction}");
    }
    let symbole = match devise {
        "EUR" => "€",
        "USD" => "$US",
        "CAD" => "$CA",
        "GBP" => "£GB",
        "AUD" => "$AU",
        "MXN" => "$MX",
        "BRL" => "R$",
        autre => autre,
    };
    format!("{}{nombre}\u{A0}{symbole}", if montant < 0.0 { "-" } else { "" })
}

/// La durée d'un produit, lue dans son identifiant du store (`Yearly`, `roleo_quarterly:trimestriel`).
pub fn formule(produit: &str) -> String {
    let p = produit.to_lowercase();
    let f = if p.contains("week") || p.contains("hebdo") {
        "Hebdo"
    } else if p.contains("month") || p.contains("mensuel") {
        "Mensuel"
    } else if p.contains("quarter") || p.contains("trimestr") {
        "Trimestriel"
    } else if p.contains("year") || p.contains("annual") || p.contains("annuel") {
        "Annuel"
    } else if produit.is_empty() {
        "Abonnement"
    } else {
        produit
    };
    f.to_string()
}

#[cfg(test)]
mod tests;
