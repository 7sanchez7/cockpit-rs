//! Les sessions de Safari (TikTok, Instagram, Facebook), lues dans ses fichiers `Cookies.binarycookies` : l'app doit
//! avoir l'accès complet au disque. Aucune valeur de cookie n'est affichée, écrite ou mise dans une erreur.
//! Hors du Mac, il n'y a pas de Safari : la session vient de la fenêtre de connexion de l'app (`Hote::cookies`).
use crate::erreur::{Erreur, Res};
use crate::jour::{self, Date};
use chrono::{Duration, TimeZone, Utc};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct Cookie {
    pub domaine: String,
    pub nom: String,
    pub valeur: String,
    pub expire: Date,
    pub cree: Date,
}

fn donnees() -> PathBuf { dirs::home_dir().unwrap_or_default().join("Library/Containers/com.apple.Safari/Data/Library") }

/// Les cookies d'un domaine (`tiktok.com`) dans le profil Safari où l'on s'est connecté le plus récemment.
pub fn cookies(domaine: &str, requis: &str) -> Res<HashMap<String, String>> {
    if !cfg!(target_os = "macos") {
        return Err(Erreur::NonConfigure(format!("pas de session : se connecter à {domaine} dans Réglages, Sessions")));
    }
    let mut profils = vec![];
    for f in fichiers()? {
        match std::fs::read(&f) {
            Ok(d) => profils.push(lire(&d)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return Err(sans_acces()),
            Err(e) => return Err(e.into()),
        }
    }
    plus_recente(&profils, domaine, requis, &jour::maintenant())
        .ok_or_else(|| Erreur::NonConfigure(format!("pas de session dans Safari : se connecter sur {domaine} dans Safari")))
}

fn sans_acces() -> Erreur {
    Erreur::NonConfigure("Safari illisible : donner l'accès complet au disque à Cockpit (Réglages Système, Confidentialité et sécurité)".into())
}

/// D'où vient la session : pour chaque fichier lu, son profil, le nombre de cookies du domaine et les dates du
/// cookie `requis`. Des dates et des comptes, jamais une valeur.
pub fn provenance(domaine: &str, requis: &str) -> String {
    if !cfg!(target_os = "macos") {
        return "pas de Safari".into();
    }
    let jour = |d: &Date| d.format("%Y-%m-%d").to_string();
    fichiers()
        .unwrap_or_default()
        .iter()
        .map(|f| {
            let s = f.to_string_lossy();
            let profil = if s.contains("WebsiteDataStore") {
                f.parent().and_then(|p| p.parent()).and_then(|p| p.file_name()).map(|n| n.to_string_lossy().chars().take(8).collect()).unwrap_or_default()
            } else {
                "défaut".to_string()
            };
            let Some(cookies) = std::fs::read(f).ok().and_then(|d| lire(&d).ok()) else { return format!("{profil} illisible") };
            let du: Vec<&Cookie> = cookies.iter().filter(|c| dans(c, domaine)).collect();
            let r: Vec<String> = du.iter().filter(|c| c.nom == requis).map(|c| format!("{requis} créé {} expire {}", jour(&c.cree), jour(&c.expire))).collect();
            format!("{profil} : {} cookies", du.len()) + &if r.is_empty() { String::new() } else { format!(", {}", r.join(", ")) }
        })
        .collect::<Vec<_>>()
        .join(" ; ")
}

/// La session du profil dont le cookie `requis` a été créé en dernier : un vieux profil garde parfois une session
/// que le site refuse.
pub fn plus_recente(profils: &[Vec<Cookie>], domaine: &str, requis: &str, maintenant: &Date) -> Option<HashMap<String, String>> {
    profils
        .iter()
        .filter_map(|cookies| {
            let s = session(cookies, domaine, maintenant);
            s.get(requis)?;
            let cree = cookies.iter().filter(|c| c.nom == requis && dans(c, domaine)).map(|c| c.cree).max().unwrap_or_else(jour::loin_passe);
            Some((s, cree))
        })
        .max_by_key(|(_, cree)| *cree)
        .map(|(s, _)| s)
}

/// L'en-tête `Cookie` d'une session.
pub fn entete(cookies: &HashMap<String, String>) -> String {
    let mut v: Vec<String> = cookies.iter().map(|(k, v)| format!("{k}={v}")).collect();
    v.sort();
    v.join("; ")
}

/// Les cookies encore valides de `domaine` et de ses sous-domaines, par nom ; pour un nom en double, le plus récent.
pub fn session(cookies: &[Cookie], domaine: &str, maintenant: &Date) -> HashMap<String, String> {
    let mut retenus: HashMap<String, &Cookie> = HashMap::new();
    for c in cookies.iter().filter(|c| dans(c, domaine) && c.expire > *maintenant && !c.valeur.is_empty()) {
        match retenus.get(&c.nom) {
            Some(x) if x.cree >= c.cree => {}
            _ => {
                retenus.insert(c.nom.clone(), c);
            }
        }
    }
    retenus.into_iter().map(|(k, c)| (k, c.valeur.clone())).collect()
}

fn dans(c: &Cookie, domaine: &str) -> bool { c.domaine == domaine || c.domaine.ends_with(&format!(".{domaine}")) }

/// Le format `cook` : nombre et tailles des pages en big-endian ; dans chaque page, les décalages des cookies, puis
/// chaque cookie en little-endian (taille, décalages de ses chaînes terminées par un zéro, expiration puis création
/// en secondes depuis 2001).
pub fn lire(o: &[u8]) -> Res<Vec<Cookie>> {
    let illisible = || Erreur::Format("Cookies.binarycookies illisible".into());
    if o.len() < 8 || &o[..4] != b"cook" {
        return Err(illisible());
    }
    let entier = |i: usize, grand: bool| -> Res<usize> {
        let b: [u8; 4] = o.get(i..i + 4).ok_or_else(illisible)?.try_into().map_err(|_| illisible())?;
        Ok(if grand { u32::from_be_bytes(b) } else { u32::from_le_bytes(b) } as usize)
    };
    let date = |i: usize| -> Res<Date> {
        let b: [u8; 8] = o.get(i..i + 8).ok_or_else(illisible)?.try_into().map_err(|_| illisible())?;
        let s = f64::from_le_bytes(b);
        let reference = Utc.with_ymd_and_hms(2001, 1, 1, 0, 0, 0).unwrap();
        Ok(if s.is_finite() && s.abs() < 1e11 { reference + Duration::milliseconds((s * 1000.0) as i64) } else { jour::loin_passe() })
    };
    let chaine = |i: usize, fin: usize| -> Res<String> {
        let t = o.get(i..fin).ok_or_else(illisible)?;
        let zero = t.iter().position(|&b| b == 0).ok_or_else(illisible)?;
        Ok(String::from_utf8_lossy(&t[..zero]).into_owned())
    };
    let pages = entier(4, true)?;
    let mut page = 8 + 4 * pages;
    let mut cookies = vec![];
    for p in 0..pages {
        for c in 0..entier(page + 4, false)? {
            let debut = page + entier(page + 8 + 4 * c, false)?;
            let fin = debut + entier(debut, false)?;
            if fin > o.len() {
                return Err(illisible());
            }
            cookies.push(Cookie {
                domaine: chaine(debut + entier(debut + 16, false)?, fin)?,
                nom: chaine(debut + entier(debut + 20, false)?, fin)?,
                valeur: chaine(debut + entier(debut + 28, false)?, fin)?,
                expire: date(debut + 40)?,
                cree: date(debut + 48)?,
            });
        }
        page += entier(8 + 4 * p, true)?;
    }
    Ok(cookies)
}

/// Le profil par défaut, puis les profils créés dans Safari.
fn fichiers() -> Res<Vec<PathBuf>> {
    let profils = donnees().join("WebKit/WebsiteDataStore");
    let mut autres: Vec<PathBuf> = match std::fs::read_dir(&profils) {
        Ok(d) => d.filter_map(|e| e.ok()).map(|e| e.path().join("Cookies/Cookies.binarycookies")).collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec![],
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return Err(sans_acces()),
        Err(e) => return Err(e.into()),
    };
    autres.sort();
    let mut tous = vec![donnees().join("Cookies/Cookies.binarycookies")];
    tous.extend(autres);
    Ok(tous)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un fichier `cook` d'une page, construit comme Safari l'écrit.
    pub fn fichier(cookies: &[(&str, &str, &str, f64, f64)]) -> Vec<u8> {
        let mut corps_cookies: Vec<Vec<u8>> = vec![];
        for (domaine, nom, valeur, expire, cree) in cookies {
            let mut c = vec![0u8; 56];
            let mut chaines = vec![];
            let mut decalage = 56usize;
            let mut offsets = vec![];
            for s in [domaine, nom, "/", valeur] {
                offsets.push(decalage);
                chaines.extend_from_slice(s.as_bytes());
                chaines.push(0);
                decalage += s.len() + 1;
            }
            let taille = 56 + chaines.len();
            c[0..4].copy_from_slice(&(taille as u32).to_le_bytes());
            c[16..20].copy_from_slice(&(offsets[0] as u32).to_le_bytes());
            c[20..24].copy_from_slice(&(offsets[1] as u32).to_le_bytes());
            c[24..28].copy_from_slice(&(offsets[2] as u32).to_le_bytes());
            c[28..32].copy_from_slice(&(offsets[3] as u32).to_le_bytes());
            c[40..48].copy_from_slice(&expire.to_le_bytes());
            c[48..56].copy_from_slice(&cree.to_le_bytes());
            c.extend(chaines);
            corps_cookies.push(c);
        }
        let entete_page = 8 + 4 * corps_cookies.len() + 4;
        let mut page = vec![0, 0, 1, 0];
        page.extend((corps_cookies.len() as u32).to_le_bytes());
        let mut pos = entete_page;
        for c in &corps_cookies {
            page.extend((pos as u32).to_le_bytes());
            pos += c.len();
        }
        page.extend([0, 0, 0, 0]);
        for c in corps_cookies {
            page.extend(c);
        }
        let mut f = b"cook".to_vec();
        f.extend(1u32.to_be_bytes());
        f.extend((page.len() as u32).to_be_bytes());
        f.extend(page);
        f
    }

    #[test]
    fn lit_et_choisit() {
        let futur = 2_000_000_000.0;
        let a = lire(&fichier(&[(".tiktok.com", "sessionid", "A", futur, 100.0), (".autre.com", "x", "y", futur, 1.0)])).unwrap();
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].nom, "sessionid");
        assert_eq!(jour::iso(&a[0].cree), "2001-01-01T00:01:40Z");
        let b = lire(&fichier(&[("www.tiktok.com", "sessionid", "B", futur, 200.0), ("tiktok.com", "vieux", "v", 10.0, 1.0)])).unwrap();
        let s = plus_recente(&[a, b], "tiktok.com", "sessionid", &jour::maintenant()).unwrap();
        assert_eq!(s.get("sessionid").map(String::as_str), Some("B"));
        assert!(!s.contains_key("vieux"));
        assert!(lire(b"pas un fichier").is_err());
    }
}
