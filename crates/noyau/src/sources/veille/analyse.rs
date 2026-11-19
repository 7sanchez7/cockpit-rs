//! L'analyse d'une vidéo par Claude Haiku : Claude ne lit pas la vidéo, on lui donne des images clés (ffmpeg) et ce
//! qui est dit (`transcrire`, whisper.cpp en local, facultatif). Seulement les outliers, 3 par passage, sous un
//! plafond de dépense ; un business sans clé n'a pas d'analyse, sans erreur.
use super::ANALYSES_PAR_PASSAGE;
use crate::collecte::http::{Http, Requete, UA_NAVIGATEUR};
use crate::collecte::{commande, Contexte};
use crate::erreur::{format_err, Erreur, Res};
use crate::jour::Date;
use crate::lecture::veille::PLAFOND_VEILLE_USD;
use crate::modeles_veille::{AnalyseVeille, VideoVeille};
use crate::p;
use base64::Engine;
use chrono::Duration;
use regex::Regex;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

pub const MODELE: &str = "claude-haiku-5-5";
/// Prix de Haiku 5.5 en dollars par million de tokens, prompt sous 100 k tokens.
pub const PRIX_ENTREE_USD: f64 = 0.10;
pub const PRIX_SORTIE_USD: f64 = 0.50;
/// Au-delà, plus aucune analyse : 50 $ restent sous 50 €.
/// Les secondes des images clés : le hook de près, puis le reste de la vidéo.
pub const INSTANTS: [f64; 3] = [0.3, 1.5, 3.0];
pub const SUITE: [f64; 3] = [0.35, 0.6, 0.85];
/// Les vidéos téléchargées sont effacées après 14 jours.
pub const GARDER_VIDEOS_JOURS: i64 = 14;

/// Analyse les outliers des 14 derniers jours qui n'en ont pas encore ; rend les échecs.
pub async fn analyser(ctx: &Contexte, maintenant: &Date) -> Vec<String> {
    let mut a_faire: Vec<VideoVeille> = match ctx.store.videos_veille("where a.video is null and v.trouvee >= ?", p![*maintenant - Duration::days(14)]) {
        Ok(v) => v.into_iter().filter(VideoVeille::outlier).collect(),
        Err(e) => return vec![format!("analyses : {e}")],
    };
    a_faire.sort_by(|a, b| b.ratio().unwrap_or(0.0).total_cmp(&a.ratio().unwrap_or(0.0)));
    a_faire.truncate(ANALYSES_PAR_PASSAGE);
    let mut echecs = vec![];
    for v in &a_faire {
        let Some(b) = ctx.businesses.iter().find(|b| b.id == v.business) else { continue };
        let depense = match ctx.store.depense_veille_usd() {
            Ok(d) => d,
            Err(e) => {
                echecs.push(format!("analyse {} : {e}", v.id));
                continue;
            }
        };
        if depense >= PLAFOND_VEILLE_USD {
            echecs.push(format!("analyses arrêtées : {} $ dépensés, plafond {} $", depense as i64, PLAFOND_VEILLE_USD as i64));
            return echecs;
        }
        let cle = match ctx.secrets.valeur(&b.config.claude, b).await {
            Err(Erreur::NonConfigure(_)) => continue,
            x => x,
        };
        let r = match cle {
            Ok(cle) => analyser_une(ctx, v, &cle, &formats(b.config.contenu_dir.as_deref()), maintenant).await,
            Err(e) => Err(e),
        };
        if let Err(e) = r {
            echecs.push(format!("analyse {} : {e}", v.id));
        }
    }
    echecs
}

async fn analyser_une(ctx: &Contexte, v: &VideoVeille, cle: &str, formats: &[String], maintenant: &Date) -> Res<()> {
    let (images, transcription) = extraits(v, &ctx.http).await?;
    let r = Requete::new("https://api.anthropic.com/v1/messages", "POST")
        .entete("x-api-key", cle)
        .entete("anthropic-version", "2023-06-01")
        .delai(120)
        .json(&corps(v, &images, &transcription, formats));
    let o = ctx.http.objet(&r).await?;
    let (entree, sortie) = usage(&o);
    ctx.store.cout_veille(&v.id, maintenant, MODELE, entree, sortie, cout(entree, sortie))?;
    ctx.store.analyse_veille(&v.id, maintenant, MODELE, &lire(&o)?)
}

// extraits

/// Les vidéos téléchargées, pour l'analyse et le lecteur de l'écran.
pub fn videos() -> PathBuf { crate::chemins::cache().join("veille-videos") }

/// Le fichier de la vidéo, téléchargé s'il manque : l'URL du fichier quand la page l'a donnée (Instagram, elle
/// expire en quelques heures), sinon `yt-dlp` en 720 px au plus.
pub async fn fichier(v: &VideoVeille, http: &Http) -> Res<PathBuf> {
    let f = videos().join(format!("{}.mp4", v.id.replace(':', "_")));
    if f.exists() {
        return Ok(f);
    }
    std::fs::create_dir_all(videos())?;
    // écrit à côté puis renomme : un téléchargement coupé ne laisse pas de vidéo tronquée
    let part = videos().join(format!("{}.mp4.part", v.id.replace(':', "_")));
    let direct = match &v.media {
        // un lien expiré se rattrape par yt-dlp
        Some(media) => http.donnees(&Requete::get(media.as_str()).entete("User-Agent", UA_NAVIGATEUR)).await.ok().filter(|d| !d.is_empty()),
        None => None,
    };
    match direct {
        Some(d) => std::fs::write(&part, d)?,
        None => {
            // le CDN de TikTok rend un 403 environ une fois sur deux, l'essai suivant passe
            let arguments = ["-q".as_ref(), "--no-warnings".as_ref(), "--force-overwrites".as_ref(), "-f".as_ref(),
                             "best[height<=720]/best".as_ref(), "-o".as_ref(), part.as_os_str(), v.url.as_ref()];
            let mut essai = commande::lancer("yt-dlp", &arguments).await;
            for _ in 1..3 {
                if essai.is_ok() {
                    break;
                }
                essai = commande::lancer("yt-dlp", &arguments).await;
            }
            essai?;
        }
    }
    if f.exists() {
        std::fs::remove_file(&f)?;
    }
    std::fs::rename(&part, &f)?;
    Ok(f)
}

/// Efface les vidéos téléchargées depuis plus de 14 jours. Une vidéo qui résiste est retentée au passage suivant.
pub fn nettoyer_videos(maintenant: &Date) {
    let Ok(fichiers) = std::fs::read_dir(videos()) else { return };
    let limite = std::time::SystemTime::from(*maintenant - Duration::days(GARDER_VIDEOS_JOURS));
    for f in fichiers.flatten() {
        if f.metadata().and_then(|m| m.created()).is_ok_and(|c| c < limite) {
            let _ = std::fs::remove_file(f.path());
        }
    }
}

/// Les images clés en JPEG 480 px de haut et la transcription (vide si `transcrire` manque ou échoue).
pub async fn extraits(v: &VideoVeille, http: &Http) -> Res<(Vec<Vec<u8>>, String)> {
    let dossier = std::env::temp_dir().join(format!("cockpit-veille-{}", uuid::Uuid::new_v4().to_string().to_uppercase()));
    std::fs::create_dir_all(&dossier)?;
    let r = extraire(v, http, &dossier).await;
    std::fs::remove_dir_all(&dossier)?;
    r
}

async fn extraire(v: &VideoVeille, http: &Http, dossier: &std::path::Path) -> Res<(Vec<Vec<u8>>, String)> {
    let video = fichier(v, http).await?;
    let duree = commande::texte("ffprobe", &["-v".as_ref(), "error".as_ref(), "-show_entries".as_ref(), "format=duration".as_ref(),
                                             "-of".as_ref(), "csv=p=0".as_ref(), video.as_os_str()]).await?;
    let duree = duree.trim().parse::<f64>().ok().or(v.duree_s).unwrap_or(30.0);
    let mut images = vec![];
    for (i, t) in moments(duree).into_iter().enumerate() {
        let jpg = dossier.join(format!("{i}.jpg"));
        let t = t.to_string();
        commande::lancer("ffmpeg", &["-v".as_ref(), "error".as_ref(), "-ss".as_ref(), t.as_ref(), "-i".as_ref(), video.as_os_str(),
                                     "-frames:v".as_ref(), "1".as_ref(), "-vf".as_ref(), "scale=-2:480".as_ref(), "-q:v".as_ref(),
                                     "6".as_ref(), "-y".as_ref(), jpg.as_os_str()]).await?;
        if let Some(d) = std::fs::read(&jpg).ok().filter(|d| !d.is_empty()) {
            images.push(d);
        }
    }
    if images.is_empty() {
        return Err(format_err("aucune image tirée de la vidéo"));
    }
    Ok((images, transcription(&video, &dossier.join("t.json")).await))
}

/// Ce qui est dit, par `transcrire` ; vide s'il manque ou échoue : l'analyse se fait alors sur les images.
async fn transcription(video: &std::path::Path, sortie: &std::path::Path) -> String {
    let arguments = [video.as_os_str(), "--langue".as_ref(), "auto".as_ref(), "--plat".as_ref(), "--sortie".as_ref(), sortie.as_os_str()];
    if commande::lancer("transcrire", &arguments).await.is_err() {
        return String::new();
    }
    let segments: Vec<Value> = std::fs::read(sortie).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default();
    let texte = segments.iter().filter_map(|s| s.get("text")?.as_str()).collect::<Vec<_>>().join(" ");
    texte.chars().take(4_000).collect()
}

/// Les instants des images clés d'une vidéo de `duree` secondes, sans dépasser sa fin.
pub fn moments(duree: f64) -> Vec<f64> {
    let fin = (duree - 0.2).max(0.0);
    let mut m: Vec<f64> = INSTANTS.iter().copied().chain(SUITE.iter().map(|s| s * duree)).map(|t| t.min(fin)).collect();
    m.sort_by(f64::total_cmp);
    m.dedup();
    m
}

// Claude

pub fn corps(v: &VideoVeille, images: &[Vec<u8>], transcription: &str, formats: &[String]) -> Value {
    let mut contenu: Vec<Value> = images.iter().map(|i| json!({
        "type": "image",
        "source": {"type": "base64", "media_type": "image/jpeg", "data": base64::engine::general_purpose::STANDARD.encode(i)},
    })).collect();
    contenu.push(json!({"type": "text", "text": consigne(v, transcription)}));
    json!({
        "model": MODELE,
        "max_tokens": 2_000,
        "thinking": {"type": "disabled"},
        "output_config": {"effort": "low", "format": {"type": "json_schema", "schema": schema(formats)}},
        "messages": [{"role": "user", "content": contenu}],
    })
}

/// Les tokens facturés : (entrée, sortie).
pub fn usage(o: &Map<String, Value>) -> (i64, i64) {
    let u = o.get("usage");
    let n = |cle: &str| u.and_then(|u| u.get(cle)).and_then(Value::as_i64).unwrap_or(0);
    (n("input_tokens"), n("output_tokens"))
}

pub fn cout(entree: i64, sortie: i64) -> f64 { (entree as f64 * PRIX_ENTREE_USD + sortie as f64 * PRIX_SORTIE_USD) / 1_000_000.0 }

pub fn lire(o: &Map<String, Value>) -> Res<AnalyseVeille> {
    let arret = o.get("stop_reason").and_then(Value::as_str).unwrap_or("aucun");
    let texte = o.get("content").and_then(Value::as_array)
        .and_then(|c| c.iter().find(|b| b.get("type").and_then(Value::as_str) == Some("text")))
        .and_then(|b| b.get("text")?.as_str());
    let Some(texte) = texte.filter(|_| arret == "end_turn") else {
        return Err(format_err(format!("Claude sans réponse ({arret})")));
    };
    let a: AnalyseVeille = serde_json::from_str(texte).map_err(|e| format_err(format!("analyse illisible : {e}")))?;
    if !(0..=3).contains(&a.adaptable) {
        return Err(format_err(format!("analyse : adaptable = {}", a.adaptable)));
    }
    Ok(a)
}

pub fn consigne(v: &VideoVeille, transcription: &str) -> String {
    let mesures = [
        v.vues.map(|n| format!("{n} vues")),
        v.ratio().map(|r| format!("{r:.1} fois la médiane du compte")),
        v.pull().map(|p| format!("pull {:.1} %", p * 100.0)),
    ].into_iter().flatten().collect::<Vec<_>>().join(", ");
    let dit = if transcription.is_empty() { "transcription indisponible" } else { transcription };
    format!("\
Tu analyses une vidéo courte de @{} sur {} qui a marché : {mesures}.
Elle a été trouvée en cherchant « {} ». Légende : {}.
Nous voulons comprendre pourquoi elle a marché pour refaire la même mécanique dans notre niche.
Les images sont des moments de la vidéo dans l'ordre : les trois premières couvrent les 3 premières secondes.
Ce qui est dit : {dit}
Réponds en français, sauf `hook` et `texteEcran` recopiés mot pour mot dans leur langue.
`blocs` découpe la vidéo en 5 blocs : hook (0-3 s), confirmation (ce qui confirme qu'on va avoir ce qu'on attend),
valeur (le cœur), placement (la place laissée à un produit ou un appel), fin (ce qui fait revoir ou commenter) ;
une phrase par bloc, « absent » s'il manque.
`ecart` : ce qui distingue cette vidéo d'une vidéo ordinaire du même compte, d'après ce que tu vois.
`adaptable` : 0 = intransposable, 3 = refaisable telle quelle dans notre niche.
`idee` : la vidéo à tourner pour nous, en une phrase concrète.",
        v.compte, v.plateforme.nom(), v.mot.as_deref().unwrap_or(""), v.legende.as_deref().unwrap_or("aucune"))
}

pub fn schema(formats: &[String]) -> Value {
    fn texte(d: &str) -> Value { json!({"type": "string", "description": d}) }
    fn objet(champs: Vec<(&str, Value)>) -> Value {
        let requis: Vec<&str> = champs.iter().map(|c| c.0).collect();
        let proprietes: Map<String, Value> = champs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
        json!({"type": "object", "properties": proprietes, "required": requis, "additionalProperties": false})
    }
    let format = if formats.is_empty() {
        texte("Le format en quelques mots")
    } else {
        json!({"type": "string", "enum": formats.iter().map(String::as_str).chain(["nouveau"]).collect::<Vec<_>>()})
    };
    objet(vec![
        ("hook", texte("La première phrase dite ou écrite, mot pour mot")),
        ("hookType", json!({"type": "string", "enum": AnalyseVeille::HOOKS})),
        ("hookVisuel", json!({"type": "string", "enum": AnalyseVeille::VISUELS})),
        ("texteEcran", texte("Le texte incrusté pendant les 3 premières secondes, vide s'il n'y en a pas")),
        ("probleme", texte("Le problème ou le désir du spectateur que la vidéo touche")),
        ("format", format),
        ("blocs", objet(["hook", "confirmation", "valeur", "placement", "fin"].map(|b| (b, texte(b))).to_vec())),
        ("ecart", texte("Ce qui change par rapport aux vidéos habituelles du compte")),
        ("adaptable", json!({"type": "integer", "description": "0 à 3"})),
        ("idee", texte("La vidéo à tourner pour nous")),
    ])
}

// formats

/// Les formats de `<contenu_dir>/formats/INDEX.md` (lignes `| 3 | [nom](nom/) | …`) ; vide sans dossier.
pub fn formats(contenu_dir: Option<&str>) -> Vec<String> {
    let Some(dir) = contenu_dir else { return vec![] };
    let racine = match dir.strip_prefix('~') {
        Some(reste) => dirs::home_dir().unwrap_or_default().join(reste.trim_start_matches(['/', '\\'])),
        None => PathBuf::from(dir),
    };
    std::fs::read_to_string(racine.join("formats").join("INDEX.md")).map(|i| formats_index(&i)).unwrap_or_default()
}

pub fn formats_index(index: &str) -> Vec<String> {
    let re = Regex::new(r"^\|\s*\d+\s*\|\s*\[([^\]]+)\]\(").expect("regex des formats");
    index.split('\n').filter_map(|l| re.captures(l).map(|c| c[1].to_string())).collect()
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::modeles_veille::Blocs;

    pub fn analyse() -> AnalyseVeille {
        AnalyseVeille {
            hook: "Arrête de dire « estoy aburrido »".into(), hook_type: "erreur courante".into(),
            hook_visuel: "visage face caméra".into(), texte_ecran: "".into(), probleme: "Parler sans faire de fautes".into(),
            format: "4-erreurs".into(),
            blocs: Blocs { hook: "a".into(), confirmation: "b".into(), valeur: "c".into(), placement: "absent".into(), fin: "d".into() },
            ecart: "Plus court".into(), adaptable: 3, idee: "Les 3 erreurs de ser et estar".into(),
        }
    }

    #[test]
    fn images_cles() {
        assert_eq!(moments(30.0), [0.3, 1.5, 3.0, 10.5, 18.0, 25.5]);
        assert_eq!(moments(2.0), [0.3, 0.7, 1.2, 1.5, 1.7, 1.8]);
        assert_eq!(moments(0.1), [0.0]);
    }

    #[test]
    fn cout_et_schema() {
        assert!((cout(1_000_000, 1_000_000) - 0.60).abs() < 1e-9);
        let s = schema(&["4-erreurs".into()]);
        assert_eq!(s["additionalProperties"], false);
        let blocs = &s["properties"]["blocs"];
        assert_eq!(blocs["additionalProperties"], false);
        assert_eq!(blocs["required"].as_array().unwrap().len(), 5);
        assert_eq!(s["properties"]["format"]["enum"], json!(["4-erreurs", "nouveau"]));
    }

    #[test]
    fn lire_la_reponse() {
        let texte = serde_json::to_string(&analyse()).unwrap();
        let reponse = |texte: &str, arret: &str| -> Map<String, Value> {
            serde_json::from_value(json!({"stop_reason": arret, "content": [{"type": "text", "text": texte}],
                                          "usage": {"input_tokens": 3_900, "output_tokens": 700}})).unwrap()
        };
        assert_eq!(lire(&reponse(&texte, "end_turn")).unwrap(), analyse());
        assert_eq!(usage(&reponse(&texte, "end_turn")), (3_900, 700));
        assert!(lire(&reponse(&texte, "refusal")).is_err());
        assert!(lire(&reponse(&texte, "max_tokens")).is_err());
        assert!(lire(&reponse("{}", "end_turn")).is_err());
        let hors_borne = texte.replace(r#""adaptable":3"#, r#""adaptable":7"#);
        assert_ne!(hors_borne, texte);
        assert!(lire(&reponse(&hors_borne, "end_turn")).is_err());
    }

    #[test]
    fn formats_de_l_index() {
        let index = "| # | Format | Statut |\n|---|---|---|\n| 1 | [comment-dire](comment-dire/) | actif |\n| 12 | [4-erreurs](4-erreurs/) | actif |\nTexte [lien](x/)";
        assert_eq!(formats_index(index), ["comment-dire", "4-erreurs"]);
        assert!(formats(None).is_empty());
    }
}
