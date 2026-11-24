//! Ce que `yt-dlp` dit des vidéos publiques d'un compte TikTok ou YouTube : liste, compteurs, miniatures.
use super::commande;
use super::http::{Http, Requete};
use crate::chemins;
use crate::erreur::{format_err, Erreur, Res};
use crate::jour::Date;
use crate::modeles::{Plateforme, Releve, Video};
use chrono::{TimeZone, Utc};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

const NIGHTLY: &str = "https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/latest/download";
/// TikTok casse yt-dlp en quelques semaines et la version stable arrive trop tard : un nightly plus vieux que ça est remplacé.
const FRAICHEUR: Duration = Duration::from_secs(3 * 86_400);
static MISE_A_JOUR: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Liste {
    pub videos: Vec<Video>,
    /// Les compteurs publics, sans watch time : la plateforme ne le montre qu'au propriétaire.
    pub releves: Vec<Releve>,
    /// Id de vidéo → URL de sa miniature.
    pub images: HashMap<String, String>,
    /// Les abonnés de la chaîne, quand la liste les donne (YouTube).
    pub abonnes: Option<i64>,
    /// L'image de la chaîne, quand la liste la donne (YouTube).
    pub avatar: Option<String>,
}

/// Le binaire du nightly pour un système, tel que le publie yt-dlp.
fn asset(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("macos", _) => Some("yt-dlp_macos"),
        ("windows", _) => Some("yt-dlp.exe"),
        ("linux", "aarch64") => Some("yt-dlp_linux_aarch64"),
        ("linux", _) => Some("yt-dlp_linux"),
        _ => None,
    }
}

/// Le fichier existe et a été écrit il y a moins de `FRAICHEUR`.
fn frais(f: &Path) -> bool {
    f.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age < FRAICHEUR)
}

/// Garde un yt-dlp nightly dans le dossier des outils du cockpit, renouvelé tous les 3 jours : `commande` le trouve
/// avant celui du PATH. Un binaire qui ne répond pas à `--version` ne remplace pas celui qui marche.
pub async fn tenir_a_jour(http: &Http) -> Res<()> {
    let Some(asset) = asset(std::env::consts::OS, std::env::consts::ARCH) else { return Ok(()) };
    let _un_seul = MISE_A_JOUR.lock().await;
    let ext = if cfg!(windows) { ".exe" } else { "" };
    let dossier = chemins::outils();
    let cible = dossier.join(format!("yt-dlp{ext}"));
    if frais(&cible) {
        return Ok(());
    }
    std::fs::create_dir_all(&dossier)?;
    let octets = http.donnees(&Requete::get(format!("{NIGHTLY}/{asset}")).delai(300)).await?;
    let neuf = dossier.join(format!("yt-dlp-nouveau{ext}"));
    std::fs::write(&neuf, &octets)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&neuf, std::fs::Permissions::from_mode(0o755))?;
    }
    let version = commande::texte(&neuf.to_string_lossy(), &["--version"]).await;
    if !version.as_ref().is_ok_and(|v| !v.trim().is_empty()) {
        let _ = std::fs::remove_file(&neuf);
        return Err(version.err().unwrap_or_else(|| format_err("yt-dlp nightly téléchargé mais muet sur --version")));
    }
    std::fs::rename(&neuf, &cible)?;
    Ok(())
}

/// `yt-dlp --flat-playlist -J` : les vidéos d'une page de profil, sans les ouvrir une par une.
pub async fn liste(url: &str) -> Res<Vec<u8>> {
    commande::lancer("yt-dlp", &["--flat-playlist", "-J", url]).await
}

/// `yt-dlp -j` sur quelques vidéos : une ligne JSON complète par vidéo (date, durée, likes).
pub async fn details(urls: &[String]) -> Res<Vec<u8>> {
    let mut a: Vec<String> = vec!["-j".into(), "--skip-download".into()];
    a.extend(urls.iter().cloned());
    commande::lancer("yt-dlp", &a).await
}

pub fn lire_liste(sortie: &[u8], plateforme: Plateforme, business: &str, compte: &str, at: &Date) -> Res<Liste> {
    let o: Option<Map<String, Value>> = serde_json::from_slice(sortie).ok();
    let Some((o, entrees)) = o.and_then(|o| {
        let e = o.get("entries")?.as_array()?.clone();
        Some((o, e))
    }) else {
        return Err(Erreur::Format(format!("yt-dlp {} @{compte} : pas de liste de vidéos", plateforme.nom())));
    };
    let avatar = o
        .get("thumbnails")
        .and_then(Value::as_array)
        .and_then(|t| t.iter().find(|x| x.get("id").and_then(Value::as_str) == Some("avatar_uncropped")))
        .and_then(|x| x.get("url")?.as_str().map(String::from));
    let mut l = Liste { abonnes: o.get("channel_follower_count").and_then(Value::as_i64), avatar, ..Default::default() };
    for e in entrees.iter().filter_map(Value::as_object) {
        let Some((v, r, image)) = entree(e, plateforme, business, compte, at) else { continue };
        if let Some(i) = image {
            l.images.insert(v.id.clone(), i);
        }
        l.releves.extend(r);
        l.videos.push(v);
    }
    Ok(l)
}

/// La sortie de `details` : une entrée complète par ligne.
pub fn lire_details(sortie: &[u8], plateforme: Plateforme, business: &str, compte: &str, at: &Date) -> Res<Liste> {
    let mut l = Liste::default();
    for ligne in sortie.split(|&b| b == b'\n').filter(|l| !l.is_empty()) {
        let e: Map<String, Value> = serde_json::from_slice(ligne)
            .map_err(|_| Erreur::Format(format!("yt-dlp {} : ligne illisible", plateforme.nom())))?;
        let Some((v, r, _)) = entree(&e, plateforme, business, compte, at) else { continue };
        l.releves.extend(r);
        l.videos.push(v);
        if let Some(a) = e.get("channel_follower_count").and_then(Value::as_i64) {
            l.abonnes = Some(a);
        }
    }
    Ok(l)
}

fn entree(e: &Map<String, Value>, plateforme: Plateforme, business: &str, compte: &str, at: &Date) -> Option<(Video, Option<Releve>, Option<String>)> {
    let brut = e.get("id")?.as_str()?;
    let id = plateforme.id_video(brut);
    let s = |k: &str| e.get(k).and_then(Value::as_str).map(String::from);
    let titre = s("title");
    let description = s("description").filter(|d| !d.is_empty());
    let entier = |k: &str| e.get(k).and_then(Value::as_i64);
    let v = Video {
        id: id.clone(),
        business: business.into(),
        compte: compte.into(),
        // TikTok met la légende dans `description`, YouTube le titre dans `title`
        legende: if plateforme == Plateforme::Tiktok { description.or(titre) } else { titre.or(description) },
        publiee_le: e.get("timestamp").and_then(Value::as_f64).and_then(|t| Utc.timestamp_millis_opt((t * 1000.0) as i64).single()),
        duree_s: e.get("duration").and_then(Value::as_f64),
        url: Some(s("webpage_url").or_else(|| s("url")).unwrap_or_else(|| plateforme.lien(brut, compte))),
        plateforme,
    };
    let r = entier("view_count").map(|vues| Releve {
        video: id,
        at: *at,
        vues,
        likes: entier("like_count"),
        commentaires: entier("comment_count"),
        partages: entier("repost_count"),
        enregistrements: entier("save_count"),
        ..Default::default()
    });
    let images: Vec<&Map<String, Value>> = e.get("thumbnails").and_then(Value::as_array).map(|t| t.iter().filter_map(Value::as_object).collect()).unwrap_or_default();
    // TikTok : la couverture ; YouTube : la plus grande
    let image = images
        .iter()
        .find(|i| i.get("id").and_then(Value::as_str) == Some("cover"))
        .or_else(|| images.iter().max_by_key(|i| i.get("height").and_then(Value::as_i64).unwrap_or(0)))
        .and_then(|i| i.get("url")?.as_str().map(String::from));
    Some((v, r, image))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binaire_du_nightly_par_systeme() {
        assert_eq!(asset("macos", "aarch64"), Some("yt-dlp_macos"));
        assert_eq!(asset("windows", "x86_64"), Some("yt-dlp.exe"));
        assert_eq!(asset("linux", "aarch64"), Some("yt-dlp_linux_aarch64"));
        assert_eq!(asset("linux", "x86_64"), Some("yt-dlp_linux"));
        assert_eq!(asset("freebsd", "x86_64"), None);
    }

    #[test]
    fn fraicheur_du_binaire() {
        let d = std::env::temp_dir().join(format!("cockpit-ytdlp-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("yt-dlp");
        assert!(!frais(&f));
        std::fs::write(&f, b"x").unwrap();
        assert!(frais(&f));
        let vieux = std::time::SystemTime::now() - FRAICHEUR - Duration::from_secs(60);
        std::fs::File::options().write(true).open(&f).unwrap().set_modified(vieux).unwrap();
        assert!(!frais(&f));
        std::fs::remove_dir_all(&d).unwrap();
    }
}
