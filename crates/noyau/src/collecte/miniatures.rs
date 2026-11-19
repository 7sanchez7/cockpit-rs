//! Les images des vidéos, des comptes et des apps, téléchargées par les sources dans le cache : l'interface les lit
//! sans réseau. Une image déjà là n'est plus retéléchargée ; vider le dossier les fait toutes revenir au passage suivant.
use super::commande;
use super::http::{Http, Requete, UA_NAVIGATEUR};
use crate::modeles::{Plateforme, Video};
use futures::stream::{self, StreamExt};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

pub fn dossier() -> PathBuf { crate::chemins::cache().join("miniatures") }

/// Le fichier d'une vidéo (son id) ou d'un compte (`avatar:tiktok:holaestony`).
pub fn fichier(cle: &str) -> PathBuf { dossier().join(cle.replace([':', '/'], "_")) }

pub fn avatar(compte: &str) -> String { format!("avatar:{compte}") }
/// La première image d'une vidéo, extraite du flux : le titre incrusté s'y lit, contrairement à l'image choisie par
/// YouTube ou Facebook.
pub fn premiere(video: &str) -> String { format!("premiere:{video}") }
pub fn icone(app: &str) -> String { format!("icone:{app}") }

/// Les clés d'une vidéo, la meilleure d'abord : la première image extraite si la plateforme en a une.
pub fn cles(video: &str, plateforme: Plateforme) -> Vec<String> {
    match plateforme {
        Plateforme::Youtube | Plateforme::Facebook => vec![premiere(video), video.into()],
        Plateforme::Tiktok | Plateforme::Instagram => vec![video.into()],
    }
}

/// Les extractions lancées en même temps : chacune attend surtout le réseau (2 à 3 s).
pub const EXTRACTIONS_EN_PARALLELE: usize = 4;

/// Le fichier d'écriture en cours, renommé une fois complet.
fn partiel(f: &std::path::Path) -> PathBuf {
    let mut s = f.as_os_str().to_owned();
    s.push(".part");
    s.into()
}

fn existe(cle: &str) -> bool { fichier(cle).exists() }

fn creer_dossier() -> Result<(), String> {
    std::fs::create_dir_all(dossier()).map_err(|e| format!("dossier {} : {e}", dossier().display()))
}

/// Écrit à côté puis renomme : une écriture coupée ne laisse pas d'image tronquée.
fn ecrire(cle: &str, donnees: &[u8]) -> std::io::Result<()> {
    let f = fichier(cle);
    let part = partiel(&f);
    std::fs::write(&part, donnees)?;
    std::fs::rename(part, f)
}

/// Télécharge les images absentes du cache (clé → URL) et rend les échecs, une ligne par image.
pub async fn telecharger(urls: &HashMap<String, String>, http: &Http) -> Vec<String> {
    let mut manquantes: Vec<(&String, &String)> = urls.iter().filter(|(c, _)| !existe(c)).collect();
    if manquantes.is_empty() {
        return vec![];
    }
    if let Err(e) = creer_dossier() {
        return vec![e];
    }
    manquantes.sort();
    let mut echecs = vec![];
    for (cle, url) in manquantes {
        let r = match http.donnees(&Requete::get(url.as_str()).entete("User-Agent", UA_NAVIGATEUR)).await {
            Ok(image) => ecrire(cle, &image).map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        };
        if let Err(e) = r {
            echecs.push(format!("image {cle} : {e}"));
        }
    }
    echecs
}

/// Extrait la première image des vidéos qui n'en ont pas encore, dans l'ordre donné, `EXTRACTIONS_EN_PARALLELE` à
/// la fois : yt-dlp donne le flux (960 px de haut au plus), ffmpeg en lit la première image. Rend les échecs.
pub async fn extraire_premieres(videos: &[Video]) -> Vec<String> {
    let mut vues = HashSet::new();
    let a_extraire: Vec<(String, String)> = videos
        .iter()
        .filter_map(|v| v.url.clone().map(|u| (v.id.clone(), u)))
        .filter(|(id, _)| vues.insert(id.clone()) && !existe(&premiere(id)))
        .collect();
    if a_extraire.is_empty() {
        return vec![];
    }
    if let Err(e) = creer_dossier() {
        return vec![e];
    }
    stream::iter(a_extraire)
        .map(|(id, url)| async move { extraire(&id, &url).await.err().map(|e| format!("première image {id} : {e}")) })
        .buffered(EXTRACTIONS_EN_PARALLELE)
        .filter_map(|e| async move { e })
        .collect()
        .await
}

/// La première image d'une vidéo : l'URL du flux par yt-dlp, puis une image par ffmpeg, écrite à côté puis renommée.
async fn extraire(id: &str, url: &str) -> crate::erreur::Res<()> {
    let flux = commande::texte("yt-dlp", &["-g", "-f", "bv*[height<=960]/b[height<=960]/b", url]).await?;
    let flux = flux.lines().next().unwrap_or_default().trim().to_string();
    let f = fichier(&premiere(id));
    let part = partiel(&f);
    commande::lancer("ffmpeg", &[std::ffi::OsStr::new("-v"), "error".as_ref(), "-i".as_ref(), flux.as_ref(), "-frames:v".as_ref(), "1".as_ref(), "-q:v".as_ref(), "3".as_ref(), "-f".as_ref(), "mjpeg".as_ref(), "-y".as_ref(), part.as_os_str()]).await?;
    std::fs::rename(&part, &f)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cles_et_fichiers() {
        assert_eq!(cles("yt:abc", Plateforme::Youtube), vec!["premiere:yt:abc".to_string(), "yt:abc".into()]);
        assert_eq!(cles("tt:1", Plateforme::Tiktok), vec!["tt:1".to_string()]);
        assert!(fichier("avatar:tiktok:a/b").ends_with("avatar_tiktok_a_b"));
    }
}
