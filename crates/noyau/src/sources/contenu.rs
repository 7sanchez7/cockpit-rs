//! Voir docs/sources/contenu.md.
use crate::collecte::{Contexte, Source};
use crate::erreur::{Erreur, Res};
use crate::jour::{self, Date};
use crate::modeles::{FicheVideo, Tache, Ton};
use crate::p;
use crate::store::Store;
use async_trait::async_trait;
use chrono::Datelike;
use chrono_tz::Europe::Paris;
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub struct Contenu;

/// Le verdict d'une vidéo est attendu 7 jours après sa sortie ; 3 jours de retard de plus, la tâche passe en alerte.
const DELAI_VERDICT: i64 = 7;
const RETARD_ALERTE: i64 = 3;

/// Ce qu'une fiche `contenu/bank` dit de sa vidéo.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Fiche {
    pub chemin: String,
    /// Le dossier du format (`comment-dire`).
    pub dossier: String,
    /// Le frontmatter, sans les commentaires ni les champs vides.
    pub champs: HashMap<String, String>,
    pub titre: Option<String>,
    pub verdict: bool,
    /// La date du nom de fichier (`2026-10-13-…`), sinon sa création.
    pub date: Option<Date>,
}

#[async_trait]
impl Source for Contenu {
    fn id(&self) -> &'static str { "contenu" }
    fn nom(&self) -> &'static str { "Contenu" }
    fn cadence(&self) -> Option<i64> { Some(15 * 60) }

    async fn collecte(&self, ctx: &Contexte) -> Res<usize> {
        let suivis: Vec<_> = ctx.businesses.iter().filter(|b| b.config.contenu_dir.as_deref().is_some_and(|d| !d.is_empty())).collect();
        if suivis.is_empty() {
            return Err(Erreur::NonConfigure("aucun dossier de fiches dans les réglages".into()));
        }
        let mut lignes = 0;
        for b in suivis {
            let dossier = b.config.contenu_dir.as_deref().unwrap_or_default();
            match etendre(dossier).and_then(|d| lire(&d)).and_then(|f| ecrire(&f, &b.id, &ctx.store, &jour::aujourdhui())) {
                Ok(n) => lignes += n,
                Err(Erreur::NonConfigure(m)) => return Err(Erreur::NonConfigure(format!("{} : {m}", b.nom))),
                Err(e) => return Err(Erreur::Commande(format!("{} : {e}", b.nom))),
            }
        }
        Ok(lignes)
    }
}

/// Le dossier des réglages, `~` remplacé par le dossier personnel, sur Mac comme sous Windows.
fn etendre(dossier: &str) -> Res<PathBuf> {
    let reste = match dossier.strip_prefix('~') {
        Some(r) if r.is_empty() || r.starts_with(['/', '\\']) => r,
        _ => return Ok(PathBuf::from(dossier)),
    };
    let maison = dirs::home_dir().ok_or_else(|| Erreur::NonConfigure("dossier personnel introuvable".into()))?;
    Ok(reste.split(['/', '\\']).filter(|p| !p.is_empty()).fold(maison, |c, p| c.join(p)))
}

/// Les fiches `<famille>/<format>/*.md` et `<famille>/<format>/fait/*.md` du dossier.
pub fn lire(racine: &Path) -> Res<Vec<Fiche>> {
    if !racine.exists() {
        return Err(Erreur::NonConfigure(format!("dossier de fiches introuvable : {}", racine.display())));
    }
    let mut relatifs = vec![];
    parcourir(racine, &mut vec![], &mut relatifs)?;
    relatifs.sort_by_key(|p| p.join("/"));
    let mut fiches = vec![];
    for parties in relatifs {
        let n = parties.len();
        if !parties[n - 1].ends_with(".md") || !(n == 3 || (n == 4 && parties[2] == "fait")) {
            continue;
        }
        let chemin = parties.iter().fold(racine.to_path_buf(), |c, p| c.join(p));
        let texte = std::fs::read_to_string(&chemin).map_err(|e| Erreur::Commande(format!("fiche {} illisible : {e}", chemin.display())))?;
        let mut f = fiche(&texte);
        let creation = std::fs::metadata(&chemin)?.created().ok().map(Date::from);
        f.chemin = chemin.to_string_lossy().into_owned();
        f.dossier = parties[1].clone();
        f.date = jour::date(&parties[n - 1].chars().take(10).collect::<String>()).or(creation);
        fiches.push(f);
    }
    Ok(fiches)
}

/// Les fichiers sous `dossier`, chacun par ses parties relatives à la racine.
fn parcourir(dossier: &Path, parties: &mut Vec<String>, sortie: &mut Vec<Vec<String>>) -> Res<()> {
    for entree in std::fs::read_dir(dossier)? {
        let entree = entree?;
        parties.push(entree.file_name().to_string_lossy().into_owned());
        if entree.file_type()?.is_dir() {
            parcourir(&entree.path(), parties, sortie)?;
        } else {
            sortie.push(parties.clone());
        }
        parties.pop();
    }
    Ok(())
}

fn regex(cellule: &'static OnceLock<Regex>, motif: &str) -> &'static Regex {
    cellule.get_or_init(|| Regex::new(motif).expect("motif valide"))
}

/// Le frontmatter, le titre et la présence d'un `## Verdict`. Le titre est la première ligne `# ` sans la date
/// de tête ; si c'est un slug (`4-astuces-draguer`), la ligne `Titre : ` du script, sinon le slug sans tirets.
pub fn fiche(texte: &str) -> Fiche {
    static COMMENTAIRE: OnceLock<Regex> = OnceLock::new();
    static DATE: OnceLock<Regex> = OnceLock::new();
    static TITRE: OnceLock<Regex> = OnceLock::new();
    let mut f = Fiche::default();
    let mut lignes = texte.split(['\n', '\u{B}', '\u{C}', '\r', '\u{85}', '\u{2028}', '\u{2029}']).peekable();
    if lignes.peek() == Some(&"---") {
        lignes.next();
        while let Some(l) = lignes.next_if(|l| *l != "---") {
            let sans_commentaire = regex(&COMMENTAIRE, r"\s+#.*$").replace(l, "");
            let Some((cle, valeur)) = sans_commentaire.split_once(':') else { continue };
            if !valeur.trim().is_empty() {
                f.champs.insert(cle.trim().into(), valeur.trim().into());
            }
        }
    }
    let mut entete: Option<String> = None;
    let mut ligne_titre: Option<String> = None;
    for l in lignes {
        if entete.is_none() {
            if let Some(reste) = l.strip_prefix("# ") {
                entete = Some(regex(&DATE, r"^\d{4}-\d{2}-\d{2}\s+[—–-]\s+").replace(reste, "").trim().to_string());
            }
        }
        if ligne_titre.is_none() {
            ligne_titre = regex(&TITRE, r"^Titre ?: *(.+)$").captures(l).map(|c| c[1].trim().to_string());
        }
        if l.starts_with("## Verdict") {
            f.verdict = true;
        }
    }
    f.titre = match entete {
        Some(e) if !e.contains(' ') => ligne_titre.or(Some(e.replace('-', " "))),
        e => e,
    };
    f
}

/// L'id et le compte d'un lien `https://www.tiktok.com/@compte/video/123`.
pub fn video(url: &str) -> Option<(String, String)> {
    static LIEN: OnceLock<Regex> = OnceLock::new();
    let c = regex(&LIEN, r"tiktok\.com/@([A-Za-z0-9._]+)/video/(\d+)").captures(url)?;
    Some((c[2].to_string(), c[1].to_string()))
}

/// Écrit les vidéos des fiches et synchronise les tâches de verdict. Rend le nombre de lignes écrites.
pub fn ecrire(fiches: &[Fiche], business: &str, store: &Store, aujourdhui: &str) -> Res<usize> {
    let mut taches = vec![];
    let mut videos = 0;
    store.lot(|| {
        for f in fiches {
            let Some((url, (id, compte))) = f.champs.get("tiktok").and_then(|u| Some((u, video(u)?))) else { continue };
            let titre = f.titre.clone().unwrap_or_else(|| id.clone());
            store.fiche_video(&FicheVideo {
                id: id.clone(),
                business: business.into(),
                compte,
                url: url.clone(),
                titre: titre.clone(),
                format: Some(f.champs.get("format").unwrap_or(&f.dossier).clone()),
                fiche: f.chemin.clone(),
                verdict: f.verdict,
            })?;
            videos += 1;
            let publiee = store.db.query("select publiee_le from video where id = ?", p![&id])?.first().and_then(|l| l.date("publiee_le"));
            let Some(sortie) = publiee.or(f.date).filter(|_| !f.verdict) else { continue };
            let echeance = jour::decale(&jour::cle(&sortie), DELAI_VERDICT);
            if echeance.as_str() > aujourdhui {
                continue;
            }
            taches.push(Tache {
                cle: format!("verdict:{id}"),
                texte: format!("Verdict : {titre}"),
                etiquette: Some(date_courte(&echeance)),
                ton: if jour::decale(&echeance, RETARD_ALERTE).as_str() < aujourdhui { Ton::Alerte } else { Ton::Neutre },
                lien: Some(lien_fichier(&f.chemin)),
                echeance: Some(echeance),
            });
        }
        Ok(())
    })?;
    store.synchroniser_taches("contenu", business, &taches)?;
    Ok(videos + taches.len())
}

/// `2026-11-17` → « 17 nov. »
pub fn date_courte(jour: &str) -> String {
    const MOIS: [&str; 12] = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];
    match jour::date(jour) {
        Some(d) => {
            let d = d.with_timezone(&Paris);
            format!("{} {}", d.day(), MOIS[d.month0() as usize])
        }
        None => jour.into(),
    }
}

/// Le lien `file://` d'une fiche, comme Foundation l'écrit ; un chemin Windows y passe avec des `/`.
fn lien_fichier(chemin: &str) -> String {
    let chemin = chemin.replace('\\', "/");
    let chemin = if chemin.starts_with('/') { chemin } else { format!("/{chemin}") };
    let mut lien = String::from("file://");
    for o in chemin.bytes() {
        if o.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@/".contains(&o) {
            lien.push(o as char);
        } else {
            lien.push_str(&format!("%{o:02X}"));
        }
    }
    lien
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modeles::{Plateforme, Video};

    #[test]
    fn fiche_lue() {
        let f = fiche("---
format: 3-apps
statut: publiée          # écrite → tournée → montée → publiée → analysée
groupe: 30
export:                  # ~/Movies/tiktok/<slug>.mp4
tiktok: https://www.tiktok.com/@holaestony/video/7696188146974739745
variable: première vidéo du format
---

# 2026-10-13 — 3 apps pour apprendre l'espagnol

## Script

```
# pas un titre
```

## Verdict

Le format tient.");
        let attendus = [("format", "3-apps"), ("statut", "publiée"), ("groupe", "30"),
                        ("tiktok", "https://www.tiktok.com/@holaestony/video/7696188146974739745"), ("variable", "première vidéo du format")];
        assert_eq!(f.champs, attendus.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect());
        assert_eq!(f.titre.as_deref(), Some("3 apps pour apprendre l'espagnol"));
        assert!(f.verdict);
        assert!(!fiche("---\nformat: x\n---\n\n# Titre\n\n## Stats\n").verdict);
    }

    #[test]
    fn titre_d_un_slug() {
        assert_eq!(fiche("# 2026-09-11 — espagnol-learn\n\n```\nTitre : 4 astuces pour apprendre\n```").titre.as_deref(), Some("4 astuces pour apprendre"));
        assert_eq!(fiche("# 2026-09-17 — 4-astuces-draguer\n").titre.as_deref(), Some("4 astuces draguer"));
        assert_eq!(fiche("# Tu me manques trop\n\nTitre : autre chose").titre.as_deref(), Some("Tu me manques trop"));
    }

    #[test]
    fn id_du_lien() {
        assert_eq!(video("https://www.tiktok.com/@holaestony/video/7696188146974739745?lang=fr"),
                   Some(("7696188146974739745".into(), "holaestony".into())));
        assert_eq!(video("https://www.tiktok.com/@holaestony"), None);
    }

    #[test]
    fn taches_de_verdict() {
        let store = Store::memoire().unwrap();
        let fiche = |id: Option<&str>, verdict: bool, date: &str| {
            let mut f = Fiche {
                chemin: format!("/bank/format-scripte/4-astuces/fait/{}.md", id.unwrap_or("x")),
                dossier: "4-astuces".into(),
                titre: Some(format!("Vidéo {}", id.unwrap_or("x"))),
                verdict,
                date: jour::date(date),
                ..Default::default()
            };
            if let Some(id) = id {
                f.champs.insert("tiktok".into(), format!("https://www.tiktok.com/@holaestony/video/{id}"));
            }
            f
        };
        // la date de TikTok passe avant celle de la fiche
        store.video(&Video {
            id: "444".into(), business: "roleo".into(), compte: "holaestony".into(), legende: None,
            publiee_le: jour::depuis_iso("2026-11-05T10:00:00Z"), duree_s: None, url: None, plateforme: Plateforme::Tiktok,
        }).unwrap();
        let fiches = [fiche(Some("111"), false, "2026-11-01"), fiche(Some("222"), true, "2026-10-01"),
                      fiche(Some("333"), false, "2026-11-10"), fiche(Some("444"), false, "2026-11-12"), fiche(None, false, "2026-10-01")];
        assert_eq!(ecrire(&fiches, "roleo", &store, "2026-11-13").unwrap(), 6);

        let taches = store.db.query("select * from a_traiter where fait_le is null order by id", p![]).unwrap();
        let colonne = |c: &str| taches.iter().map(|t| t.texte(c).unwrap_or_default()).collect::<Vec<_>>();
        assert_eq!(colonne("id"), ["contenu:verdict:111", "contenu:verdict:444"]);
        assert_eq!(colonne("ton"), ["alerte", "neutre"]);
        assert_eq!(colonne("etiquette"), ["8 nov.", "12 nov."]);
        assert_eq!(taches[0].texte("texte").as_deref(), Some("Verdict : Vidéo 111"));
        assert_eq!(taches[0].texte("lien").as_deref(), Some("file:///bank/format-scripte/4-astuces/fait/111.md"));
        let videos = store.db.query("select id, format, verdict from video where titre is not null order by id", p![]).unwrap();
        assert_eq!(videos.iter().map(|v| v.texte("id").unwrap_or_default()).collect::<Vec<_>>(), ["111", "222", "333", "444"]);
        assert!(videos[1].booleen("verdict") && videos[0].texte("format").as_deref() == Some("4-astuces"));

        // le verdict écrit, la tâche se ferme
        ecrire(&[fiche(Some("111"), true, "2026-11-01")], "roleo", &store, "2026-11-13").unwrap();
        assert!(store.db.query("select id from a_traiter where fait_le is null", p![]).unwrap().is_empty());
    }

    #[test]
    fn fichiers_lus() {
        let racine = std::env::temp_dir().join(format!("contenu-{}", uuid::Uuid::new_v4()));
        for (chemin, texte) in [("_fiche.md", "# modèle"), ("famille/comment-dire/a.md", "# A"),
                                ("famille/comment-dire/fait/2026-10-13-b.md", "# B"), ("famille/comment-dire/brouillons/c.md", "# C")] {
            let u = chemin.split('/').fold(racine.clone(), |c, p| c.join(p));
            std::fs::create_dir_all(u.parent().unwrap()).unwrap();
            std::fs::write(&u, texte).unwrap();
        }
        let fiches = lire(&racine).unwrap();
        assert_eq!(fiches.iter().map(|f| f.titre.as_deref()).collect::<Vec<_>>(), [Some("A"), Some("B")]);
        assert_eq!(fiches.iter().map(|f| f.dossier.as_str()).collect::<Vec<_>>(), ["comment-dire", "comment-dire"]);
        assert_eq!(fiches[1].date, jour::date("2026-10-13"));
        assert!(lire(&racine.join("absent")).is_err());
        std::fs::remove_dir_all(&racine).unwrap();
    }
}
