//! Actifs, installs, essais, ronds commencés et finis, actifs par heure, fil des essais, détail de chaque install, portes du paywall
//! et événements de l'app un par un, en HogQL.
//! Voir docs/sources/posthog.md ; le décodage et l'écriture sont dans `posthog/decodage.rs`.
mod decodage;
#[cfg(test)]
mod tests;

pub use decodage::{ecrire, Compte, Horaire, LecturePostHog, Series, NOMS_PORTES};

use crate::business::{Business, Evenements};
use crate::collecte::{Contexte, Requete, Source};
use crate::dimension::Dimension;
use crate::erreur::{format_err, Erreur, Res};
use crate::jour::{self, Date};
use crate::metrique::{self, Boutique};
use async_trait::async_trait;
use chrono_tz::Europe::Paris;
use serde_json::{json, Value};

pub struct PostHog;

pub const CLE_PROJET: &str = "POSTHOG_PROJECT_ID";
pub const CLE_API: &str = "POSTHOG_PERSONAL_API_KEY";

#[async_trait]
impl Source for PostHog {
    fn id(&self) -> &'static str { "posthog" }
    fn nom(&self) -> &'static str { "PostHog" }
    fn cadence(&self) -> i64 { 15 * 60 }

    async fn collecte(&self, ctx: &Contexte) -> Res<usize> {
        let suivis: Vec<&Business> =
            ctx.businesses.iter().filter(|b| !b.config.posthog_host.as_deref().unwrap_or("").is_empty()).collect();
        if suivis.is_empty() {
            return Err(Erreur::NonConfigure("aucun business n'a d'hôte PostHog".into()));
        }
        let mut lignes = 0;
        for b in suivis {
            lignes += collecte(b, ctx).await.map_err(|e| match e {
                Erreur::NonConfigure(m) => Erreur::NonConfigure(format!("{} : {m}", b.nom)),
                e => Erreur::Commande(format!("{} : {e}", b.nom)),
            })?;
        }
        Ok(lignes)
    }
}

async fn collecte(b: &Business, ctx: &Contexte) -> Res<usize> {
    let projet = ctx.secrets.valeur(CLE_PROJET, b).await?;
    let cle = ctx.secrets.valeur(CLE_API, b).await?;
    let url = format!("{}/api/projects/{projet}/query/", b.config.posthog_host.as_deref().unwrap_or(""));
    let requete = |q: String| Requete::new(url.as_str(), "POST").jeton(&cle).json(&json!({"query": {"kind": "HogQLQuery", "query": q}}));
    let hogql = |q: String| {
        let r = requete(q);
        async move { decodage::lignes(&ctx.http.objet(&r).await?) }
    };

    let maintenant = jour::maintenant();
    let aujourdhui = jour::cle(&maintenant);
    // historique : au premier passage, ou quand hier ou J-90 manque pour un segment
    let mut historique = false;
    for s in SEGMENTS {
        historique = historique || ctx.store.historique_incomplet(&b.id, &nom(s, metrique::ACTIFS), JOURS_HISTORIQUE, &aujourdhui)?;
    }
    let jours = jour::derniers(if historique { JOURS_HISTORIQUE } else { 2 }, &aujourdhui);
    let jours_heures = jour::derniers(if historique { 9 } else { 2 }, &aujourdhui);
    let coupe = semaine_avant(&maintenant);
    let depuis = minuit(&jour::decale(&jours[0], -7))?;

    let e = &b.config.evenements;
    let mut l = LecturePostHog {
        series: decodage::series(&hogql(requete_jours(e, depuis, coupe)).await?, &hogql(requete_installs(e, depuis, coupe)).await?)?,
        actifs_heure: decodage::par_heure(&hogql(requete_heures(minuit(&jours_heures[0])?)).await?)?,
        ..Default::default()
    };
    let veille = maintenant.timestamp() - 48 * 3600;
    l.fil = decodage::fil(&hogql(requete_essais(e, veille)).await?, &b.id)?;

    // installs détaillés et portes : une fois par heure
    let cle_horaire = format!("posthog.horaire.{}", b.id);
    let derniere = ctx.store.reglage(&cle_horaire)?.and_then(|s| jour::depuis_iso(&s));
    if derniere.is_none_or(|d| (maintenant - d).num_seconds() >= 3600) {
        let debut = jour::decale(&aujourdhui, -JOURS_INSTALLS);
        let installs = decodage::installs(&hogql(requete_par_install(e, minuit(&debut)?)).await?)?;
        let portes = decodage::portes(&hogql(requete_portes(e, minuit(&debut)?)).await?)?;
        l.horaire = Some(Horaire { debut, installs, portes });
    }

    let mut n = ctx.store.lot(|| {
        let n = ecrire(&l, &b.id, &jours, &jours_heures, &maintenant, &ctx.store)?;
        if l.horaire.is_some() {
            ctx.store.regler(&cle_horaire, Some(&jour::iso(&maintenant)))?;
        }
        Ok(n)
    })?;

    // événements un par un, par pages, depuis le curseur (au premier passage, la fenêtre des installs)
    if e.detail.is_empty() {
        return Ok(n);
    }
    let cle_curseur = format!("posthog.evenements.{}", b.id);
    let mut curseur = match ctx.store.reglage(&cle_curseur)?.and_then(|s| s.parse::<i64>().ok()) {
        Some(c) => c,
        None => {
            // le premier jour couvert entier, pour que l'écran Data ne compare pas à des jours à moitié relus
            let premier = jour::decale(&aujourdhui, -JOURS_INSTALLS);
            ctx.store.regler(&format!("{CLE_EVENEMENTS_DEPUIS}{}", b.id), Some(&premier))?;
            minuit(&premier)? * 1000
        }
    };
    let mut depuis = curseur - MARGE_EVENEMENTS_MS;
    for _ in 0..PAGES_EVENEMENTS {
        let page = decodage::evenements(&decodage::page(&ctx.http.objet(&requete(requete_evenements(e, depuis))).await?)?)?;
        let dernier = page.last().map_or(depuis, |x| x.recu_ms);
        curseur = curseur.max(dernier);
        ctx.store.lot(|| {
            ctx.store.evenements_app(&b.id, &page)?;
            ctx.store.regler(&cle_curseur, Some(&curseur.to_string()))
        })?;
        n += page.len();
        if page.len() < PAGE_EVENEMENTS {
            break;
        }
        // une page pleine d'événements arrivés à la même milliseconde ne bloque pas le curseur
        depuis = if dernier > depuis { dernier } else { depuis + 1 };
    }
    Ok(n)
}

/// La même heure de Paris 7 jours plus tôt, en secondes Unix (7 × 24 h si ce jour-là n'a pas cette heure).
fn semaine_avant(d: &Date) -> i64 {
    d.with_timezone(&Paris)
        .checked_sub_days(chrono::Days::new(7))
        .map_or(d.timestamp() - 7 * 86_400, |x| x.timestamp())
}

// requêtes HogQL

/// Événements envoyés sans que l'user ouvre l'app (même liste que tools/suivi/hebdo.py de Roleo).
pub const BRUIT: &str = "('$identify', '$set', '$feature_flag_called', '$opt_in', '$screen_view_exit')";

/// Jours de séries relus quand l'historique a un trou : les 90 jours complets de la plus longue période de l'en-tête, plus aujourd'hui.
pub const JOURS_HISTORIQUE: i64 = 91;

/// Les événements `commence` et `usage` ne partent de façon fiable chez Roleo que depuis la 2.0.1 : avant, ronds et retours
/// ne sont pas mesurés.
pub const RONDS_DEPUIS: &str = "2026-10-15";

/// Les écrans du tunnel de Roleo dans l'ordre, clé PostHog et nom affiché ; `paywall_viewed` y tient lieu de l'événement `paywall`.
pub const ECRANS: [(&str, &str); 17] = [
    ("welcome", "Accueil"), ("language", "Langue apprise"), ("native_language", "Langue maternelle"),
    ("acquisition_source", "Connu comment"), ("first_name", "Prénom"), ("age", "Âge"),
    ("motivation", "Motivation"), ("pain", "Ce qui bloque"), ("interests", "Centres d'intérêt"),
    ("level", "Niveau"), ("social_proof", "Preuve sociale"), ("app_tour", "Présentation"),
    ("projection", "Projection"), ("account_setup", "Compte"), ("persona", "Choix du tuteur"),
    ("paywall_viewed", "Paywall vu"), ("paywall", "Essai touché"),
];

/// Les séries se lisent pour le total puis pour chaque store, dans cet ordre de colonnes.
pub const SEGMENTS: [Option<Boutique>; 3] = [None, Some(Boutique::AppStore), Some(Boutique::PlayStore)];

/// Les jours d'install recalculés à chaque heure : 90 jours, plus de quoi comparer une semaine.
pub const JOURS_INSTALLS: i64 = 97;
/// Suivi de l'id du business : le premier jour que la table `evenement` couvre entier.
pub const CLE_EVENEMENTS_DEPUIS: &str = "posthog.evenements.depuis.";

/// Une page d'événements, et le nombre de pages lues par passage (le rattrapage du premier passage en prend plusieurs).
pub const PAGE_EVENEMENTS: usize = 10_000;
pub const PAGES_EVENEMENTS: usize = 30;

/// Chaque passage relit la dernière heure d'arrivées : un événement peut devenir lisible après un plus récent.
pub const MARGE_EVENEMENTS_MS: i64 = 3_600_000;

/// Un événement arrivé chez PostHog plus de 30 jours après son instant est perdu : la borne garde la lecture sur la clé de tri.
pub const RETARD_MAX_JOURS: i64 = 30;

const OS: &str = "properties.$os";

/// Secondes Unix de minuit à Paris.
pub fn minuit(j: &str) -> Res<i64> {
    jour::date(j).map(|d| d.timestamp()).ok_or_else(|| format_err(format!("jour illisible : {j}")))
}

/// La métrique d'un segment : la base pour le total, suffixée pour un store.
pub fn nom(b: Option<Boutique>, m: &str) -> String { b.map_or_else(|| m.to_string(), |b| b.metrique(m)) }

/// Le segment en HogQL, d'après l'`$os` de l'event (le total garde tout, web et sans `$os` compris).
pub fn filtre(b: Option<Boutique>, os: &str) -> String {
    match b {
        None => "true".into(),
        Some(Boutique::AppStore) => format!("{os} in ('iOS', 'iPadOS')"),
        Some(Boutique::PlayStore) => format!("{os} = 'Android'"),
    }
}

/// Par jour de Paris et pour chaque segment : actifs, essais, ronds finis, ronds commencés, chacun en entier puis avant la coupe.
pub fn requete_jours(e: &Evenements, depuis: i64, coupe: i64) -> String {
    let avant = format!("timestamp < fromUnixTimestamp({coupe})");
    let (essai, usage, commence) = (&e.essai, &e.usage, &e.commence);
    let colonnes: Vec<String> = SEGMENTS
        .iter()
        .map(|b| {
            let s = filtre(*b, OS);
            format!(
                "uniqExactIf(person_id, {s}), uniqExactIf(person_id, {s} and {avant}),
    countIf(event = '{essai}' and {s}), countIf(event = '{essai}' and {s} and {avant}),
    countIf(event = '{usage}' and {s}), countIf(event = '{usage}' and {s} and {avant}),
    countIf(event = '{commence}' and {s}), countIf(event = '{commence}' and {s} and {avant})"
            )
        })
        .collect();
    format!(
        "select toString(toDate(toTimeZone(timestamp, 'Europe/Paris'))) as jour,
    {}
from events
where timestamp >= fromUnixTimestamp({depuis}) and event not in {BRUIT}
group by jour order by jour limit 100",
        colonnes.join(",\n    ")
    )
}

/// Par jour de Paris et pour chaque segment : installs (premier événement `installation` sur iPhone, iPad ou Android),
/// installs avant la coupe. Le total est la somme des deux stores.
pub fn requete_installs(e: &Evenements, depuis: i64, coupe: i64) -> String {
    let colonnes: Vec<String> = SEGMENTS
        .iter()
        .map(|b| filtre(*b, "os"))
        .map(|s| format!("countIf({s}), countIf({s} and f < fromUnixTimestamp({coupe}))"))
        .collect();
    format!(
        "select toString(toDate(toTimeZone(f, 'Europe/Paris'))) as jour, {}
from (select person_id, min(timestamp) as f, argMin(properties.$os, timestamp) as os
      from events where event = '{}'
      group by person_id having f >= fromUnixTimestamp({depuis}))
where os in ('iOS', 'iPadOS', 'Android')
group by jour order by jour limit 100",
        colonnes.join(", "),
        e.installation
    )
}

/// Par jour et heure de Paris : actifs de chaque segment.
pub fn requete_heures(depuis: i64) -> String {
    let colonnes: Vec<String> = SEGMENTS.iter().map(|b| format!("uniqExactIf(person_id, {})", filtre(*b, OS))).collect();
    format!(
        "select toString(toDate(toTimeZone(timestamp, 'Europe/Paris'))) as jour,
    toHour(toTimeZone(timestamp, 'Europe/Paris')) as heure,
    {}
from events
where timestamp >= fromUnixTimestamp({depuis}) and event not in {BRUIT}
group by jour, heure order by jour, heure limit 1000",
        colonnes.join(", ")
    )
}

/// Les essais : uuid, instant, produit, porte du paywall, pays.
pub fn requete_essais(e: &Evenements, depuis: i64) -> String {
    format!(
        "select toString(uuid), toUnixTimestamp(timestamp), properties.product, properties.source,
    properties.$geoip_country_code
from events
where event = '{}' and timestamp >= fromUnixTimestamp({depuis})
order by timestamp desc limit 200",
        e.essai
    )
}

/// Les personnes installées depuis `depuis`, sur iPhone, iPad ou Android.
fn installes(e: &Evenements, depuis: i64) -> String {
    format!(
        "select person_id from events where event = '{}' group by person_id
having min(timestamp) >= fromUnixTimestamp({depuis}) and argMin(properties.$os, timestamp) in ('iOS', 'iPadOS', 'Android')",
        e.installation
    )
}

/// Une ligne par install : jour d'install, rang de l'écran le plus loin atteint, paywall vu, essai gratuit, achat sans essai,
/// rond commencé (un rond fini compte, si son début s'est perdu), rond fini, rond fini entre N et N+1 jours après l'install (1, 7, 14, 30), l'id RevenueCat s'il a converti,
/// la personne PostHog, ronds commencés, ronds finis, jours de Paris avec un rond fini, secondes de l'install au premier rond
/// (-1 sans rond), jour du dernier rond fini, puis la valeur de chaque `Dimension`.
pub fn requete_par_install(e: &Evenements, depuis: i64) -> String {
    let cles = ECRANS.iter().map(|(c, _)| format!("'{c}'")).collect::<Vec<_>>().join(", ");
    let retours = [1, 7, 14, 30]
        .map(|n| format!("arrayExists(t -> t >= fu + {n} * 86400 and t < fu + {} * 86400, finis) as r{n}", n + 1))
        .join(", ");
    let valeurs = Dimension::lues().map(|d| format!("{} as v_{}", valeur(d, e), d.brut())).collect::<Vec<_>>();
    let noms = Dimension::lues().map(|d| format!("v_{}", d.brut())).collect::<Vec<_>>();
    let Evenements { installation, etape, paywall, essai, achat, commence, usage, .. } = e;
    format!(
        "select toString(toDate(toTimeZone(f, 'Europe/Paris'))) as jour, loin, vu, essai, direct, commence, rond, r1, r7, r14, r30, client,
    toString(person_id), commences, length(finis), jours_ronds, premier_rond, dernier_rond,
    {noms}
from (select person_id, minIf(timestamp, event = '{installation}') as f, toUnixTimestamp(f) as fu,
        arrayMax(arrayMap(x -> indexOf([{cles}], x),
            groupUniqArrayIf(if(event = '{etape}', toString(properties.step), 'paywall_viewed'),
                             event in ('{etape}', '{paywall}')))) as loin,
        countIf(event = '{paywall}') > 0 as vu, countIf(event = '{essai}') > 0 as essai,
        countIf(event = '{achat}') > 0 and countIf(event = '{essai}') = 0 as direct,
        groupArrayIf(toUnixTimestamp(timestamp), event = '{usage}') as finis, length(finis) > 0 as rond,
        (countIf(event = '{commence}') > 0 or rond) as commence,
        {retours},
        countIf(event = '{commence}') as commences,
        uniqExactIf(toDate(toTimeZone(timestamp, 'Europe/Paris')), event = '{usage}') as jours_ronds,
        if(commence, greatest(toUnixTimestamp(minIf(timestamp, event in ('{commence}', '{usage}'))) - fu, 0), -1) as premier_rond,
        if(rond, toString(toDate(toTimeZone(maxIf(timestamp, event = '{usage}'), 'Europe/Paris'))), '') as dernier_rond,
        if(essai or direct, ifNull(argMaxIf(distinct_id, timestamp, event in ('{essai}', '{achat}')), ''), '')
            as client,
        {valeurs}
      from events
      where timestamp >= fromUnixTimestamp({depuis}) and person_id in ({installes})
          and event in ('{installation}', '{etape}', '{paywall}', '{essai}', '{achat}', '{commence}', '{usage}')
      group by person_id)
order by f limit 50000",
        noms = noms.join(", "),
        valeurs = valeurs.join(",\n                "),
        installes = installes(e, depuis),
    )
}

/// La valeur d'une dimension pour un install, texte vide s'il n'a pas répondu ; plusieurs valeurs séparées
/// par des virgules pour les intérêts et les blocages.
fn valeur(d: Dimension, e: &Evenements) -> String {
    // la dernière réponse à un écran du tunnel
    let reponse = |prop: &str, etapes: &[&str]| {
        let dans = etapes.iter().map(|x| format!("'{x}'")).collect::<Vec<_>>().join(", ");
        format!("ifNull(argMaxIf(toString(properties.{prop}), timestamp, event = '{}' and properties.step in ({dans})), '')", e.etape)
    };
    // la propriété au premier événement de la liste
    let premiere = |prop: &str, evenements: &str| {
        format!("ifNull(argMinIf(toString(properties.{prop}), timestamp, event in ({evenements})), '')")
    };
    let installation = format!("'{}'", e.installation);
    match d {
        Dimension::Loin => "toString(loin)".into(),
        Dimension::Langue => reponse("language", &["language"]),
        Dimension::Maternelle => reponse("native_language", &["native_language"]),
        Dimension::Source => reponse("source", &["acquisition_source"]),
        Dimension::Age => reponse("age_range", &["age"]),
        Dimension::Motivation => reponse("motivation", &["motivation"]),
        // `pains` (écran à cocher) a remplacé `pain` (un seul blocage)
        Dimension::Blocage => {
            let pains = reponse("pains", &["pain"]);
            format!("if({pains} != '', {pains}, {})", reponse("pain", &["pain"]))
        }
        Dimension::Interet => reponse("interests", &["interests"]),
        Dimension::Niveau => reponse("start_level", &["level", "projection"]),
        Dimension::Tuteur => reponse("character", &["persona"]),
        Dimension::Notifs => reponse("notifs", &["reminder"]),
        Dimension::Compte => reponse("method", &["account_link"]),
        Dimension::Pays => premiere("$geoip_country_code", &installation),
        Dimension::Version => premiere("$app_version", &installation),
        Dimension::Build => premiere("$app_build", &installation),
        Dimension::Produit => premiere("product", &format!("'{}', '{}'", e.essai, e.achat)),
        Dimension::Plateforme => premiere("$os", &installation),
        Dimension::Zone => unreachable!("la zone se déduit du pays, hors de Dimension::lues"),
    }
}

/// Les événements `detail` arrivés chez PostHog depuis `depuis_ms`, dans l'ordre d'arrivée (`created_at`), propriétés `$…` à part.
pub fn requete_evenements(e: &Evenements, depuis_ms: i64) -> String {
    let noms = e.detail.iter().map(|x| format!("'{x}'")).collect::<Vec<_>>().join(", ");
    let instant = depuis_ms / 1000 - RETARD_MAX_JOURS * 86_400;
    format!(
        "select toString(uuid), toString(person_id), toUnixTimestamp(timestamp), toUnixTimestamp64Milli(created_at), event,
    ifNull(toString({OS}), ''), ifNull(toString(properties.$app_build), ''), toString(toDate(toTimeZone(timestamp, 'Europe/Paris'))),
    arrayFilter(x -> not startsWith(x.1, '$'), JSONExtractKeysAndValuesRaw(properties))
from events
where created_at >= fromUnixTimestamp64Milli({depuis_ms}) and timestamp >= fromUnixTimestamp({instant}) and event in ({noms})
order by created_at, uuid limit {PAGE_EVENEMENTS}"
    )
}

/// Par jour de Paris et par porte du paywall (`source`) : paywalls vus, essais gratuits, achats sans essai.
/// L'achat part aussi au début d'un essai gratuit : la différence avec les essais est l'achat direct.
pub fn requete_portes(e: &Evenements, depuis: i64) -> String {
    let Evenements { paywall, essai, achat, .. } = e;
    format!(
        "select toString(toDate(toTimeZone(timestamp, 'Europe/Paris'))) as jour, ifNull(toString(properties.source), '') as porte,
    countIf(event = '{paywall}'), countIf(event = '{essai}'),
    greatest(countIf(event = '{achat}') - countIf(event = '{essai}'), 0)
from events
where timestamp >= fromUnixTimestamp({depuis}) and event in ('{paywall}', '{essai}', '{achat}')
group by jour, porte order by jour, porte limit 10000"
    )
}

/// Lignes HogQL, une valeur JSON par colonne.
pub type Lignes = Vec<Vec<Value>>;
