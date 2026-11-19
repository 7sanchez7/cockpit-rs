//! Avis, tâches « répondre à l'avis », changements d'état des versions et téléchargements par jour (rapports de ventes),
//! par l'API App Store Connect en lecture seule.
//! Voir docs/sources/app-store-connect.md. La note et le nombre de notes viennent de Classements.
use crate::business::Business;
use crate::collecte::http::{decoder, encoder};
use crate::collecte::{jwt, Contexte, Requete, Source};
use crate::erreur::{format_err, Erreur, Res};
use crate::jour::{self, Date};
use crate::metrique::{Boutique, TELECHARGEMENTS};
use crate::modeles::{Avis, Evenement, Tache, Ton, TypeFil};
use crate::p;
use crate::sources::revenuecat::nomme;
use crate::store::Store;
use async_trait::async_trait;
use chrono::Duration;
use flate2::read::GzDecoder;
use jsonwebtoken::EncodingKey;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};

pub struct AppStoreConnect;

pub const BASE: &str = "https://api.appstoreconnect.apple.com/v1";
/// Un avis entre dans le fil s'il a moins de 7 jours.
const FENETRE_FIL: i64 = 7;
/// Rapports de ventes : les jours manquants des 90 derniers ; un jour sans téléchargement n'a pas de rapport (404),
/// il vaut 0 passé 3 jours.
pub const HISTORIQUE_VENTES: i64 = 90;
const RETARD_VENTES: i64 = 3;
/// Types de produit d'un premier téléchargement ; retéléchargements (3F…) et mises à jour (7F…) exclus.
const PREMIERS_TELECHARGEMENTS: [&str; 7] = ["1", "1F", "1T", "F1", "1E", "1EP", "1EU"];

#[async_trait]
impl Source for AppStoreConnect {
    fn id(&self) -> &'static str { "asc" }
    fn nom(&self) -> &'static str { "App Store Connect" }
    fn cadence(&self) -> i64 { 60 * 60 }

    async fn collecte(&self, ctx: &Contexte) -> Res<usize> {
        let suivis: Vec<&Business> =
            ctx.businesses.iter().filter(|b| b.config.app_store_id.is_some() && b.config.asc_issuer.is_some()).collect();
        if suivis.is_empty() {
            return Err(Erreur::NonConfigure("aucun business avec appStoreId et ascIssuer".into()));
        }
        let (kid, pem) = cle(&dossier_cles()?)?;
        let premier = ctx.store.dernier_succes(self.id())?.is_none();
        let mut lignes = 0;
        for b in suivis {
            lignes += collecte(b, &kid, &pem, premier, ctx).await.map_err(|e| nomme(e, b))?;
        }
        Ok(lignes)
    }
}

async fn collecte(b: &Business, kid: &str, pem: &str, premier: bool, ctx: &Contexte) -> Res<usize> {
    let (Some(app), Some(issuer)) = (&b.config.app_store_id, &b.config.asc_issuer) else { return Ok(0) };
    let jeton = jeton(kid, issuer, pem, &jour::maintenant())?;
    let lire = |chemin: String| {
        let r = Requete::get(format!("{BASE}{chemin}")).jeton(&jeton);
        async move { ctx.http.donnees(&r).await }
    };
    // 200 avis tiennent sur une page, bien au-delà de ce que l'app reçoit
    let avis = avis(&lire(format!("/apps/{app}/customerReviews?sort=-createdDate&limit=200&include=response")).await?, &b.id)?;
    let at = (!premier).then(jour::maintenant);
    let version = version(&lire(format!("/apps/{app}/appStoreVersions?limit=5")).await?, &b.id, app, at)?;
    let lignes = ecrire(&avis, version, app, &b.id, &ctx.store, &jour::maintenant())?;
    let Some(vendeur) = &b.config.asc_vendor else { return Ok(lignes) };
    Ok(lignes + telechargements_manquants(b, app, vendeur, &jeton, ctx).await?)
}

/// Les téléchargements des jours qui manquent, un rapport de ventes quotidien par jour.
async fn telechargements_manquants(b: &Business, app: &str, vendeur: &str, jeton: &str, ctx: &Contexte) -> Res<usize> {
    let aujourdhui = jour::aujourdhui();
    let lus: HashSet<String> = ctx
        .store
        .db
        .query("select jour from serie where business = ? and metrique = ? and heure = -1",
            p![&b.id, Boutique::AppStore.metrique(TELECHARGEMENTS)])?
        .iter()
        .filter_map(|l| l.texte("jour"))
        .collect();
    let mut lignes = 0;
    for jour in a_relire(&lus, &aujourdhui) {
        let filtres = [("frequency", "DAILY"), ("reportType", "SALES"), ("reportSubType", "SUMMARY"), ("vendorNumber", vendeur),
            ("reportDate", &jour), ("version", "1_1")];
        let requete: Vec<String> =
            filtres.iter().map(|(n, v)| format!("{}={}", encoder(&format!("filter[{n}]")), encoder(v))).collect();
        let r = Requete::get(format!("{BASE}/salesReports?{}", requete.join("&"))).jeton(jeton).entete("Accept", "application/a-gzip");
        let n = match ctx.http.donnees(&r).await {
            Ok(rapport) if rapport.starts_with(&[0x1F, 0x8B]) => telechargements(&gunzip(&rapport)?, app)?,
            Ok(rapport) => telechargements(&rapport, app)?,
            Err(Erreur::Http(404, _)) if jour > jour::decale(&aujourdhui, -RETARD_VENTES) => continue,
            Err(Erreur::Http(404, _)) => 0.0,
            Err(e) => return Err(e),
        };
        Boutique::AppStore.telechargements(&ctx.store, &b.id, &jour, n)?;
        lignes += 1;
    }
    Ok(lignes)
}

/// Les jours des 90 derniers, hier compris, qui n'ont pas encore de valeur, du plus ancien au plus récent.
pub fn a_relire(lus: &HashSet<String>, aujourdhui: &str) -> Vec<String> {
    jour::derniers(HISTORIQUE_VENTES, &jour::decale(aujourdhui, -1)).into_iter().filter(|j| !lus.contains(j)).collect()
}

fn gunzip(data: &[u8]) -> Res<Vec<u8>> {
    let mut sortie = Vec::new();
    GzDecoder::new(data).read_to_end(&mut sortie).map_err(|e| format_err(format!("rapport de ventes : gzip illisible : {e}")))?;
    Ok(sortie)
}

/// Un rapport de ventes quotidien (TSV) : la somme des `Units` des premiers téléchargements de l'app.
pub fn telechargements(tsv: &[u8], app: &str) -> Res<f64> {
    let texte = String::from_utf8_lossy(tsv);
    let lignes: Vec<Vec<&str>> =
        texte.split(['\r', '\n']).filter(|l| !l.is_empty()).map(|l| l.split('\t').map(str::trim).collect()).collect();
    let colonne = |nom: &str| lignes.first().and_then(|e| e.iter().position(|c| *c == nom));
    let (Some(type_), Some(unites), Some(id)) = (colonne("Product Type Identifier"), colonne("Units"), colonne("Apple Identifier"))
    else {
        return Err(format_err("rapport de ventes sans Product Type Identifier, Units ou Apple Identifier"));
    };
    Ok(lignes
        .iter()
        .skip(1)
        .filter(|l| l.len() > type_.max(unites).max(id) && l[id] == app && PREMIERS_TELECHARGEMENTS.contains(&l[type_]))
        .map(|l| l[unites].parse::<f64>().unwrap_or(0.0))
        .sum())
}

/// Les avis, le fil (avis récents, état de version) et les tâches ; rend le nombre de lignes écrites.
pub fn ecrire(avis: &[Avis], version: Option<Evenement>, app: &str, business: &str, store: &Store, maintenant: &Date) -> Res<usize> {
    let fil: Vec<Evenement> = fil_avis(avis, app, maintenant).into_iter().chain(version).collect();
    let a_faire = taches(avis, app);
    let mut lignes = avis.len() + a_faire.len();
    store.lot(|| {
        for a in avis {
            store.avis(a)?;
        }
        for e in &fil {
            if store.fil(e)? {
                lignes += 1;
            }
        }
        Ok(())
    })?;
    store.synchroniser_taches("asc", business, &a_faire)?;
    Ok(lignes)
}

// jeton

/// Là où Apple et les outils de Roleo rangent la clé `AuthKey_<KEYID>.p8` ; elle n'en sort jamais.
fn dossier_cles() -> Res<PathBuf> {
    let maison = dirs::home_dir().ok_or_else(|| Erreur::NonConfigure("dossier personnel introuvable".into()))?;
    Ok(maison.join(".appstoreconnect").join("private_keys"))
}

/// La première clé `AuthKey_<KEYID>.p8` du dossier, en PEM ; son KEYID vient du nom du fichier.
pub fn cle(dossier: &Path) -> Res<(String, String)> {
    let absente = || Erreur::NonConfigure(format!("aucune clé AuthKey_<KEYID>.p8 dans {}", dossier.display()));
    // un dossier absent ou illisible, c'est une clé absente
    let entrees = std::fs::read_dir(dossier).map_err(|_| absente())?;
    let mut noms = Vec::new();
    for entree in entrees {
        let nom = entree.map_err(|_| absente())?.file_name();
        if let Some(nom) = nom.to_str().filter(|n| n.starts_with("AuthKey_") && n.ends_with(".p8")) {
            noms.push(nom.to_string());
        }
    }
    noms.sort();
    let nom = noms.into_iter().next().ok_or_else(absente)?;
    let kid = nom["AuthKey_".len()..nom.len() - ".p8".len()].to_string();
    let illisible = || Erreur::NonConfigure(format!("clé {nom} illisible"));
    let pem = std::fs::read_to_string(dossier.join(&nom)).map_err(|_| illisible())?;
    EncodingKey::from_ec_pem(pem.as_bytes()).map_err(|_| illisible())?;
    Ok((kid, pem))
}

/// JWT ES256 d'App Store Connect, valable 19 minutes (Apple refuse au-delà de 20).
pub fn jeton(kid: &str, issuer: &str, pem: &str, maintenant: &Date) -> Res<String> {
    #[derive(Serialize)]
    struct Charge<'a> { iss: &'a str, iat: i64, exp: i64, aud: &'a str }
    let iat = maintenant.timestamp();
    jwt::es256(kid, &Charge { iss: issuer, iat, exp: iat + 19 * 60, aud: "appstoreconnect-v1" }, pem)
}

// décodage

fn date(texte: &str) -> Res<Date> { jour::depuis_iso(texte).ok_or_else(|| format_err(format!("date illisible : {texte}"))) }

/// `customerReviews` avec `include=response` : `relationships.response.data` est nul tant qu'on n'a pas répondu.
pub fn avis(data: &[u8], business: &str) -> Res<Vec<Avis>> {
    #[derive(Deserialize)]
    struct Reponse { data: Vec<R> }
    #[derive(Deserialize)]
    struct R { id: String, attributes: A, relationships: Option<Liens> }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct A {
        rating: i64,
        title: Option<String>,
        body: Option<String>,
        reviewer_nickname: Option<String>,
        created_date: String,
        territory: Option<String>,
    }
    #[derive(Deserialize)]
    struct Liens { response: Option<Lien> }
    #[derive(Deserialize)]
    struct Lien { data: Option<Ref> }
    #[derive(Deserialize)]
    struct Ref {}
    decoder::<Reponse>(data)?
        .data
        .into_iter()
        .map(|r| {
            let a = r.attributes;
            Ok(Avis {
                id: r.id,
                business: business.to_string(),
                pays: a.territory,
                note: a.rating,
                titre: a.title,
                texte: a.body,
                auteur: a.reviewer_nickname,
                at: date(&a.created_date)?,
                repondu: r.relationships.and_then(|l| l.response).and_then(|l| l.data).is_some(),
            })
        })
        .collect()
}

pub fn lien_avis(app: &str) -> String {
    format!("https://appstoreconnect.apple.com/apps/{app}/distribution/activity/ios/ratingsResponses")
}

/// Une tâche par avis de 3 étoiles ou moins sans réponse ; 1 ou 2 étoiles en alerte.
pub fn taches(avis: &[Avis], app: &str) -> Vec<Tache> {
    avis.iter()
        .filter(|a| a.note <= 3 && !a.repondu)
        .map(|a| {
            let titre = a.titre.as_ref().map(|t| format!(", « {} »", t.chars().take(60).collect::<String>())).unwrap_or_default();
            Tache {
                cle: format!("avis:{}", a.id),
                texte: format!("Répondre à {}{titre}", a.auteur.as_deref().unwrap_or("un avis")),
                etiquette: Some(format!("{} ★", a.note)),
                ton: if a.note <= 2 { Ton::Alerte } else { Ton::Neutre },
                echeance: None,
                lien: Some(lien_avis(app)),
            }
        })
        .collect()
}

/// Les avis de moins de 7 jours, en lignes AVIS du fil.
pub fn fil_avis(avis: &[Avis], app: &str, maintenant: &Date) -> Vec<Evenement> {
    avis.iter()
        .filter(|a| a.at >= *maintenant - Duration::days(FENETRE_FIL))
        .map(|a| {
            let note = a.note.clamp(0, 5) as usize;
            let etoiles = "★".repeat(note) + &"☆".repeat(5 - note);
            Evenement {
                id: format!("asc:avis:{}", a.id),
                business: a.business.clone(),
                at: a.at,
                type_: TypeFil::Avis,
                texte: [Some(etoiles), a.titre.clone()].into_iter().flatten().collect::<Vec<_>>().join(" "),
                lien: Some(lien_avis(app)),
            }
        })
        .collect()
}

/// L'état de la version iOS la plus récente, en ligne VERSION : une ligne par état, donc une par changement.
/// `at` nul (premier passage) date la ligne de la création de la version, sinon du passage qui voit le changement.
pub fn version(data: &[u8], business: &str, app: &str, at: Option<Date>) -> Res<Option<Evenement>> {
    #[derive(Deserialize)]
    struct Reponse { data: Vec<V> }
    #[derive(Deserialize)]
    struct V { id: String, attributes: A }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct A { version_string: String, app_store_state: String, platform: Option<String>, created_date: String }
    let mut recente: Option<(Date, V)> = None;
    for v in decoder::<Reponse>(data)?.data {
        if v.attributes.platform.as_deref().unwrap_or("IOS") != "IOS" {
            continue;
        }
        let cree = date(&v.attributes.created_date)?;
        // la première des plus récentes, comme `max(by:)`
        if recente.as_ref().is_none_or(|(d, _)| cree > *d) {
            recente = Some((cree, v));
        }
    }
    let Some((cree, v)) = recente else { return Ok(None) };
    let a = v.attributes;
    Ok(Some(Evenement {
        id: format!("asc:version:{}:{}", v.id, a.app_store_state),
        business: business.to_string(),
        at: at.unwrap_or(cree),
        type_: TypeFil::Version,
        texte: format!("{} {}", a.version_string, etat(&a.app_store_state)),
        lien: Some(format!("https://appstoreconnect.apple.com/apps/{app}/distribution")),
    }))
}

pub fn etat(code: &str) -> String {
    match code {
        "PREPARE_FOR_SUBMISSION" => "en préparation",
        "READY_FOR_REVIEW" => "prête pour l'examen",
        "WAITING_FOR_REVIEW" => "en attente d'examen",
        "IN_REVIEW" => "en examen",
        "ACCEPTED" => "acceptée",
        "PENDING_DEVELOPER_RELEASE" => "validée, à publier",
        "PENDING_APPLE_RELEASE" => "validée, publication par Apple",
        "PROCESSING_FOR_APP_STORE" | "PROCESSING_FOR_DISTRIBUTION" => "en traitement",
        "READY_FOR_SALE" | "READY_FOR_DISTRIBUTION" => "en vente",
        "REJECTED" => "refusée",
        "METADATA_REJECTED" => "métadonnées refusées",
        "DEVELOPER_REJECTED" => "retirée de l'examen",
        "INVALID_BINARY" => "binaire invalide",
        "WAITING_FOR_EXPORT_COMPLIANCE" => "en attente de conformité export",
        "REMOVED_FROM_SALE" | "DEVELOPER_REMOVED_FROM_SALE" => "retirée de la vente",
        "REPLACED_WITH_NEW_VERSION" => "remplacée",
        _ => return code.to_lowercase().replace('_', " "),
    }
    .to_string()
}

#[cfg(test)]
mod tests;
