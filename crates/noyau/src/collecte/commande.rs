//! Lance un programme local (gcloud, yt-dlp, ffmpeg, transcrire) sans bloquer l'interface.
//! Une app lancée depuis le Finder ou le menu Démarrer n'a pas le PATH du terminal : on cherche aussi dans les dossiers
//! où Homebrew, pip, winget et scoop installent.
use crate::erreur::{Erreur, Res};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// Les dossiers parcourus après le PATH.
fn dossiers() -> Vec<PathBuf> {
    let mut d: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    let maison = dirs::home_dir().unwrap_or_default();
    if cfg!(windows) {
        if let Some(local) = dirs::data_local_dir() {
            d.push(local.join("Microsoft").join("WinGet").join("Links"));
            d.push(local.join("Programs").join("Python").join("Scripts"));
            d.push(local.join("Google").join("Cloud SDK").join("google-cloud-sdk").join("bin"));
        }
        d.push(maison.join("scoop").join("shims"));
        d.push(PathBuf::from(r"C:\Program Files (x86)\Google\Cloud SDK\google-cloud-sdk\bin"));
        d.push(PathBuf::from(r"C:\Program Files\Google\Cloud SDK\google-cloud-sdk\bin"));
    } else {
        for x in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"] {
            d.push(PathBuf::from(x));
        }
        d.push(maison.join(".local/bin"));
        d.push(maison.join("google-cloud-sdk/bin"));
    }
    d
}

/// Le chemin d'un programme (`yt-dlp`), ou `None` s'il n'est pas installé.
pub fn trouver(nom: &str) -> Option<PathBuf> {
    let p = Path::new(nom);
    if p.is_absolute() {
        return p.is_file().then(|| p.to_path_buf());
    }
    let extensions: &[&str] = if cfg!(windows) { &["exe", "cmd", "bat"] } else { &[""] };
    for d in dossiers() {
        for ext in extensions {
            let f = if ext.is_empty() { d.join(nom) } else { d.join(format!("{nom}.{ext}")) };
            if f.is_file() {
                return Some(f);
            }
        }
    }
    None
}

/// Lance `programme` et rend sa sortie standard ; rend `Erreur::Commande` avec la sortie d'erreur si le code n'est
/// pas 0, ou si le programme n'est pas installé.
pub async fn lancer<S: AsRef<OsStr>>(programme: &str, arguments: &[S]) -> Res<Vec<u8>> {
    let chemin = trouver(programme).ok_or_else(|| Erreur::Commande(format!("{programme} introuvable : l'installer et le mettre dans le PATH")))?;
    let mut c = tokio::process::Command::new(&chemin);
    c.args(arguments).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    if let Some(p) = chemin.parent() {
        // ffmpeg et yt-dlp se trouvent l'un l'autre par le PATH
        let mut path = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect::<Vec<_>>();
        path.insert(0, p.to_path_buf());
        if let Ok(p) = std::env::join_paths(path) {
            c.env("PATH", p);
        }
    }
    #[cfg(windows)]
    c.creation_flags(0x0800_0000); // pas de fenêtre de console
    let sortie = c.output().await.map_err(|e| Erreur::Commande(format!("{programme} : {e}")))?;
    if !sortie.status.success() {
        let code = sortie.status.code().map(|c| c.to_string()).unwrap_or_else(|| "signal".into());
        let message = super::http::extrait(&sortie.stderr, 300);
        return Err(Erreur::Commande(format!("{programme} a échoué ({code}) : {message}")));
    }
    Ok(sortie.stdout)
}

/// La sortie en texte.
pub async fn texte<S: AsRef<OsStr>>(programme: &str, arguments: &[S]) -> Res<String> {
    Ok(String::from_utf8_lossy(&lancer(programme, arguments).await?).into_owned())
}
