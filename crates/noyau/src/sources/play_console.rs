//! Les installs Play par jour, lus dans les rapports groupés de la Play Console (Cloud Storage) avec le compte de service
//! du business. Voir docs/sources/play-console.md.
use crate::business::Business;
use crate::collecte::http::{encoder, objet};
use crate::collecte::{jwt, Contexte, Http, Requete, Source};
use crate::erreur::{format_err, Erreur, Res};
use crate::jour::{self, Date};
use crate::metrique::Boutique;
use async_trait::async_trait;
use jsonwebtoken::EncodingKey;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct PlayConsole;

pub const PORTEE: &str = "https://www.googleapis.com/auth/devstorage.read_only";
pub const STOCKAGE: &str = "https://storage.googleapis.com/storage/v1/b";
/// Les mois relus à chaque passage : de quoi couvrir 91 jours.
const MOIS: i64 = 4;

#[async_trait]
impl Source for PlayConsole {
    fn id(&self) -> &'static str { "playconsole" }
    fn nom(&self) -> &'static str { "Play Console" }
    fn cadence(&self) -> i64 { 3 * 3600 }

    async fn collecte(&self, ctx: &Contexte) -> Res<usize> {
        let rempli = |v: &Option<String>| v.as_deref().is_some_and(|s| !s.is_empty());
        let suivis: Vec<&Business> =
            ctx.businesses.iter().filter(|b| rempli(&b.config.play_bucket) && rempli(&b.config.play_package)).collect();
        if suivis.is_empty() {
            return Err(Erreur::NonConfigure("aucun business n'a de bucket Play".into()));
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
    let bucket = bucket(b.config.play_bucket.as_deref().unwrap_or_default());
    let paquet = b.config.play_package.as_deref().unwrap_or_default();
    let compte = compte(&dossier_cles()?.join(format!("{}-play-publisher.json", b.id)))?;
    let jeton = jeton(&compte, &ctx.http, &jour::maintenant()).await?;
    let prefixe = format!("stats/installs/installs_{paquet}_");
    // `/` reste tel quel dans la requête, comme l'écrit URLComponents
    let liste = Requete::get(format!("{STOCKAGE}/{bucket}/o?prefix={}", encoder(&prefixe).replace("%2F", "/"))).jeton(&jeton);
    let objets = ctx.http.objet(&liste).await?;
    let noms: Vec<String> = objets
        .get("items")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(|o| o.get("name").and_then(Value::as_str).map(str::to_string)).collect())
        .unwrap_or_default();
    let mut lus: HashMap<String, f64> = HashMap::new();
    for nom in a_lire(&noms, &prefixe, &jour::aujourdhui()) {
        let r = Requete::get(format!("{STOCKAGE}/{bucket}/o/{}?alt=media", alphanumerique(&nom))).jeton(&jeton);
        lus.extend(installs(&ctx.http.donnees(&r).await?)?);
    }
    ctx.store.lot(|| {
        for (jour, n) in &lus {
            Boutique::PlayStore.telechargements(&ctx.store, &b.id, jour, *n)?;
        }
        Ok(lus.len())
    })
}

/// Un nom d'objet dans un chemin d'URL : lettres et chiffres gardés, le reste en `%XX`.
fn alphanumerique(s: &str) -> String {
    s.bytes().map(|o| if o.is_ascii_alphanumeric() { (o as char).to_string() } else { format!("%{o:02X}") }).collect()
}

/// La clé JSON du compte de service : `<business>-play-publisher.json`, hors du dépôt.
fn dossier_cles() -> Res<PathBuf> {
    let maison = dirs::home_dir().ok_or_else(|| Erreur::NonConfigure("dossier personnel introuvable".into()))?;
    Ok(maison.join(".googleplay"))
}

// rapports

/// Le nom du bucket, que la config tienne le nom seul ou l'URI copiée dans la Play Console (`gs://pubsite_prod_rev_…/stats/…`).
pub fn bucket(config: &str) -> String {
    let s = config.trim_matches([' ', '\t']);
    s.strip_prefix("gs://").unwrap_or(s).split('/').next().unwrap_or_default().to_string()
}

/// Les rapports `overview` des `MOIS` derniers mois, du plus ancien au plus récent.
pub fn a_lire(noms: &[String], prefixe: &str, aujourdhui: &str) -> Vec<String> {
    let debut = jour::decale(aujourdhui, -31 * (MOIS - 1))[..7].replace('-', "");
    let mut r: Vec<String> = noms
        .iter()
        .filter(|n| {
            n.ends_with("_overview.csv")
                && n.strip_prefix(prefixe).is_some_and(|reste| reste.chars().take(6).collect::<String>() >= debut)
        })
        .cloned()
        .collect();
    r.sort();
    r
}

/// Un rapport `overview` (UTF-16, une ligne par jour) en `jour → installs` : la colonne « Daily User Installs »,
/// les personnes qui installent l'app pour la première fois sur un de leurs appareils.
pub fn installs(data: &[u8]) -> Res<HashMap<String, f64>> {
    let texte = texte(data).ok_or_else(|| format_err("rapport Play illisible"))?;
    let lignes: Vec<Vec<&str>> = texte.split(['\r', '\n']).filter(|l| !l.is_empty()).map(|l| l.split(',').collect()).collect();
    let entete: Vec<&str> = lignes.first().map(|e| e.iter().map(|c| c.trim_matches([' ', '\t'])).collect()).unwrap_or_default();
    let (Some(date), Some(colonne)) =
        (entete.iter().position(|c| *c == "Date"), entete.iter().position(|c| *c == "Daily User Installs"))
    else {
        return Err(format_err("rapport Play sans colonnes Date et Daily User Installs"));
    };
    let mut r = HashMap::new();
    for l in lignes.iter().skip(1).filter(|l| l.len() == entete.len()) {
        let n = l[colonne]
            .trim_matches([' ', '\t'])
            .parse::<f64>()
            .map_err(|_| format_err(format!("rapport Play : « {} » n'est pas un nombre", l[colonne])))?;
        r.insert(l[date].to_string(), n);
    }
    Ok(r)
}

/// UTF-16 selon sa marque d'ordre (celle des rapports Google), UTF-8 sans marque.
fn texte(data: &[u8]) -> Option<String> {
    let unites = |grand_boutien: bool| -> Vec<u16> {
        data[2..].chunks_exact(2).map(|c| if grand_boutien { u16::from_be_bytes([c[0], c[1]]) } else { u16::from_le_bytes([c[0], c[1]]) }).collect()
    };
    match data {
        [0xFF, 0xFE, ..] if data.len().is_multiple_of(2) => String::from_utf16(&unites(false)).ok(),
        [0xFE, 0xFF, ..] if data.len().is_multiple_of(2) => String::from_utf16(&unites(true)).ok(),
        _ => String::from_utf8(data.to_vec()).ok(),
    }
}

// compte de service

#[derive(Debug)]
pub struct CompteService {
    pub email: String,
    pub url_jeton: String,
    pub pem: String,
}

pub fn compte(fichier: &Path) -> Res<CompteService> {
    let nom = fichier.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let data = std::fs::read(fichier)
        .map_err(|_| Erreur::NonConfigure(format!("clé du compte de service absente (~/.googleplay/{nom})")))?;
    let o = objet(&data)?;
    let champ = |n: &str| o.get(n).and_then(Value::as_str).map(str::to_string);
    let (Some(email), Some(url_jeton), Some(pem)) = (champ("client_email"), champ("token_uri"), champ("private_key")) else {
        return Err(format_err("clé du compte de service sans client_email, token_uri ou private_key"));
    };
    EncodingKey::from_rsa_pem(pem.as_bytes()).map_err(|_| format_err("clé privée RSA refusée"))?;
    Ok(CompteService { email, url_jeton, pem })
}

/// Un jeton OAuth d'une heure, demandé avec un JWT RS256 signé par la clé du compte de service.
pub async fn jeton(c: &CompteService, http: &Http, maintenant: &Date) -> Res<String> {
    let assertion = assertion(c, maintenant)?;
    let r = Requete::new(&c.url_jeton, "POST")
        .formulaire(&[("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"), ("assertion", &assertion)]);
    http.objet(&r)
        .await?
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format_err("jeton Google absent"))
}

pub fn assertion(c: &CompteService, maintenant: &Date) -> Res<String> {
    #[derive(Serialize)]
    struct Charge<'a> { iss: &'a str, scope: &'a str, aud: &'a str, iat: i64, exp: i64 }
    let iat = maintenant.timestamp();
    jwt::rs256(&Charge { iss: &c.email, scope: PORTEE, aud: &c.url_jeton, iat, exp: iat + 3600 }, &c.pem)
}

#[cfg(test)]
mod tests;
