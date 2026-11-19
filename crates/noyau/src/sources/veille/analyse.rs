//! L'analyse d'une vidéo par Claude Haiku : Claude ne lit pas la vidéo, on lui donne des images clés (ffmpeg) et ce
//! qui est dit (`transcrire`, whisper.cpp en local, facultatif). Les vidéos à au moins 2× la médiane de leur compte, les
//! plus récentes d'abord, 10 par passage, sous un plafond de dépense ; un business sans clé n'a pas d'analyse, sans erreur.
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

/// Le prix d'un modèle en dollars par million de tokens : entrée, sortie, cache écrit (5 min) et lu ; multiplié par
/// `facteur_long` quand le prompt dépasse `long` tokens.
pub struct Prix {
    pub modele: &'static str,
    pub entree: f64,
    pub sortie: f64,
    pub cache_ecrit: f64,
    pub cache_lu: f64,
    pub long: i64,
    pub facteur_long: f64,
}

/// Haiku 5.5 : les analyses et la boucle d'une demande.
pub const HAIKU: Prix =
    Prix { modele: "claude-haiku-5-5", entree: 0.10, sortie: 0.50, cache_ecrit: 0.125, cache_lu: 0.01, long: 100_000, facteur_long: 5.0 };
/// Sonnet 5.5 : les mots-clés d'une demande.
pub const SONNET: Prix =
    Prix { modele: "claude-sonnet-5-5", entree: 2.0, sortie: 10.0, cache_ecrit: 2.5, cache_lu: 0.10, long: i64::MAX, facteur_long: 1.0 };
/// Les secondes des images clés : le hook de près, puis le reste de la vidéo.
pub const INSTANTS: [f64; 3] = [0.3, 1.5, 3.0];
pub const SUITE: [f64; 3] = [0.35, 0.6, 0.85];
/// Les vidéos téléchargées sont effacées après 14 jours.
pub const GARDER_VIDEOS_JOURS: i64 = 14;

/// Analyse les vidéos des 14 derniers jours qui n'en ont pas encore ; rend les échecs.
pub async fn analyser(ctx: &Contexte, maintenant: &Date) -> Vec<String> {
    let a_faire = match ctx.store.videos_veille("where a.video is null and v.trouvee >= ?", p![*maintenant - Duration::days(14)]) {
        Ok(v) => a_analyser(v),
        Err(e) => return vec![format!("analyses : {e}")],
    };
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

/// Les vidéos qui valent une analyse (`VideoVeille::a_analyser`), les plus récemment publiées d'abord, puis les plus
/// gros ratios ; `ANALYSES_PAR_PASSAGE` au plus.
pub fn a_analyser(videos: Vec<VideoVeille>) -> Vec<VideoVeille> {
    let mut a_faire: Vec<VideoVeille> = videos.into_iter().filter(VideoVeille::a_analyser).collect();
    a_faire.sort_by(|a, b| b.publiee.cmp(&a.publiee).then_with(|| b.ratio().unwrap_or(0.0).total_cmp(&a.ratio().unwrap_or(0.0))));
    a_faire.truncate(ANALYSES_PAR_PASSAGE);
    a_faire
}

pub(crate) async fn analyser_une(ctx: &Contexte, v: &VideoVeille, cle: &str, formats: &[String], maintenant: &Date) -> Res<()> {
    let (images, transcription) = extraits(v, &ctx.http).await?;
    let o = appeler(&ctx.http, cle, &corps(v, &images, &transcription, formats), 120).await?;
    facturer(ctx, &v.id, &HAIKU, &o, maintenant)?;
    ctx.store.analyse_veille(&v.id, maintenant, HAIKU.modele, &lire(&o)?)
}

// extraits

/// Les vidéos téléchargées, pour l'analyse et le lecteur de l'écran.
pub fn videos() -> PathBuf { crate::chemins::cache().join("veille-videos") }

/// Le fichier de la vidéo, téléchargé s'il manque : l'URL du fichier quand la page l'a donnée (Instagram, elle
/// expire en quelques heures), sinon `yt-dlp` en 720 px au plus.
pub async fn fichier(v: &VideoVeille, http: &Http) -> Res<PathBuf> {
    let f = videos().join(format!("{}.mp4", v.id.replace(':', "_")));
    if f.exists() {
        let mut debut = [0u8; 12];
        if std::fs::File::open(&f).and_then(|mut x| std::io::Read::read_exact(&mut x, &mut debut)).is_ok() && est_mp4(&debut) {
            return Ok(f);
        }
        // une page HTML rendue à la place du fichier : on la jette et on retélécharge
        std::fs::remove_file(&f)?;
    }
    std::fs::create_dir_all(videos())?;
    // écrit à côté puis renomme : un téléchargement coupé ne laisse pas de vidéo tronquée
    let part = videos().join(format!("{}.mp4.part", v.id.replace(':', "_")));
    let direct = match &v.media {
        // un lien expiré se rattrape par yt-dlp
        Some(media) => http.donnees(&Requete::get(media.as_str()).entete("User-Agent", UA_NAVIGATEUR)).await.ok().filter(|d| est_mp4(d)),
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

/// Un fichier MP4 commence par sa boîte `ftyp` ; un lien expiré rend une page HTML.
fn est_mp4(debut: &[u8]) -> bool { debut.len() >= 12 && &debut[4..8] == b"ftyp" }

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
        "model": HAIKU.modele,
        "max_tokens": 2_000,
        "thinking": {"type": "disabled"},
        "output_config": {"effort": "low", "format": {"type": "json_schema", "schema": schema(formats)}},
        "messages": [{"role": "user", "content": contenu}],
    })
}

/// Un appel à l'API Messages ; rend la réponse entière.
pub async fn appeler(http: &Http, cle: &str, corps: &Value, delai: u64) -> Res<Map<String, Value>> {
    let r = Requete::new("https://api.anthropic.com/v1/messages", "POST")
        .entete("x-api-key", cle)
        .entete("anthropic-version", "2023-06-01")
        .delai(delai)
        .json(corps);
    http.objet(&r).await
}

/// Les tokens d'un appel : entrée hors cache, écrits dans le cache, lus dans le cache, sortie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Usage {
    pub entree: i64,
    pub cache_ecrit: i64,
    pub cache_lu: i64,
    pub sortie: i64,
}

impl Usage {
    pub fn de(o: &Map<String, Value>) -> Usage {
        let u = o.get("usage");
        let n = |cle: &str| u.and_then(|u| u.get(cle)).and_then(Value::as_i64).unwrap_or(0);
        Usage { entree: n("input_tokens"), cache_ecrit: n("cache_creation_input_tokens"), cache_lu: n("cache_read_input_tokens"),
                sortie: n("output_tokens") }
    }

    /// Tout le prompt, cache compris : la colonne `entree` de `veille_cout`.
    pub fn prompt(&self) -> i64 { self.entree + self.cache_ecrit + self.cache_lu }

    pub fn usd(&self, p: &Prix) -> f64 {
        let f = if self.prompt() > p.long { p.facteur_long } else { 1.0 };
        let n = self.entree as f64 * p.entree + self.cache_ecrit as f64 * p.cache_ecrit + self.cache_lu as f64 * p.cache_lu
            + self.sortie as f64 * p.sortie;
        n * f / 1_000_000.0
    }
}

/// Écrit le coût d'un appel au nom de `objet` (une vidéo, ou `demande:<id>`).
pub fn facturer(ctx: &Contexte, objet: &str, p: &Prix, o: &Map<String, Value>, at: &Date) -> Res<()> {
    let u = Usage::de(o);
    ctx.store.cout_veille(objet, at, p.modele, u.prompt(), u.sortie, u.usd(p))
}

/// Le texte d'une réponse finie (les blocs de texte mis bout à bout) ; une erreur si Claude a refusé ou a été coupé.
pub fn texte(o: &Map<String, Value>) -> Res<String> {
    let arret = o.get("stop_reason").and_then(Value::as_str).unwrap_or("aucun");
    let texte: String = o.get("content").and_then(Value::as_array).into_iter().flatten()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|b| b.get("text")?.as_str()).collect();
    if arret != "end_turn" || texte.is_empty() {
        return Err(format_err(format!("Claude sans réponse ({arret})")));
    }
    Ok(texte)
}

pub fn lire(o: &Map<String, Value>) -> Res<AnalyseVeille> {
    let texte = texte(o)?;
    let mut a: AnalyseVeille = serde_json::from_str(&texte).map_err(|e| format_err(format!("analyse illisible : {e}")))?;
    a.app = a.app.as_deref().and_then(app_nommee);
    if !(0..=3).contains(&a.adaptable) {
        return Err(format_err(format!("analyse : adaptable = {}", a.adaptable)));
    }
    Ok(a)
}

/// Une app nommée par Claude, sans les réponses vides (« », « aucune », « none »…).
pub fn app_nommee(app: &str) -> Option<String> {
    let app = app.trim().trim_matches(['"', '«', '»', '.']).trim();
    let vide = ["", "aucune", "aucun", "none", "null", "n/a", "-", "vide"].contains(&app.to_lowercase().as_str());
    (!vide).then(|| app.to_string())
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
une phrase par bloc, « absent » s'il manque ; `parts` donne la part de la durée de chaque bloc en %, total 100, 0 s'il manque.
`ecart` : ce qui distingue cette vidéo d'une vidéo ordinaire du même compte, d'après ce que tu vois.
`adaptable` : 0 = intransposable, 3 = refaisable telle quelle dans notre niche.
`idee` : la vidéo à tourner pour nous, en une phrase concrète.
`app` : le nom de l'app ou du produit dont la vidéo fait la promo, seulement s'il est clairement nommé (dit, écrit à
l'écran ou dans la légende) ; vide sinon, ne devine jamais.",
        v.compte, v.plateforme.nom(), v.mot.as_deref().unwrap_or(""), v.legende.as_deref().unwrap_or("aucune"))
}

const BLOCS: [&str; 5] = ["hook", "confirmation", "valeur", "placement", "fin"];

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
        ("blocs", objet(BLOCS.map(|b| (b, texte(b))).to_vec())),
        ("parts", objet(BLOCS.map(|b| (b, json!({"type": "integer", "description": "0 à 100"}))).to_vec())),
        ("ecart", texte("Ce qui change par rapport aux vidéos habituelles du compte")),
        ("adaptable", json!({"type": "integer", "description": "0 à 3"})),
        ("idee", texte("La vidéo à tourner pour nous")),
        ("app", texte("L'app ou le produit promu, clairement nommé ; vide sinon")),
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
    use crate::modeles_veille::{Blocs, Parts};

    pub fn analyse() -> AnalyseVeille {
        AnalyseVeille {
            hook: "Arrête de dire « estoy aburrido »".into(), hook_type: "erreur courante".into(),
            hook_visuel: "visage face caméra".into(), texte_ecran: "".into(), probleme: "Parler sans faire de fautes".into(),
            format: "4-erreurs".into(),
            blocs: Blocs { hook: "a".into(), confirmation: "b".into(), valeur: "c".into(), placement: "absent".into(), fin: "d".into() },
            parts: Some(Parts { hook: 10, confirmation: 15, valeur: 60, placement: 0, fin: 15 }),
            ecart: "Plus court".into(), adaptable: 3, idee: "Les 3 erreurs de ser et estar".into(), app: Some("Roleo".into()),
        }
    }

    #[test]
    fn mp4_reconnu_par_sa_boite() {
        assert!(est_mp4(b"\0\0\0\x20ftypisom\0\0\x02\0"));
        assert!(!est_mp4(b"<!doctype html><html>"));
        assert!(!est_mp4(b"ftyp"));
    }

    #[test]
    fn les_plus_recentes_d_abord() {
        use crate::modeles::Plateforme;
        use crate::modeles_veille::Trouvaille;
        use crate::store::Store;
        use chrono::{TimeZone, Utc};
        let t0 = Utc.timestamp_opt(1_800_000_000, 0).unwrap();
        let store = Store::memoire().unwrap();
        // (id, publiée il y a n jours, vues) pour une médiane de 10 000
        for (id, jours, vues) in [("vieille", 9, 90_000), ("recente", 1, 25_000), ("moyenne", 4, 40_000), ("faible", 0, 15_000)] {
            let mut t = Trouvaille::nouvelle(Plateforme::Tiktok, id, "profe.ana");
            (t.publiee, t.vues) = (Some(t0 - Duration::days(jours)), Some(vues));
            store.trouvaille(&t, "roleo", None, &t0).unwrap();
        }
        store.base(Plateforme::Tiktok, "profe.ana", Some(10_000), 20, None, &t0).unwrap();
        let ids: Vec<String> = a_analyser(store.videos_veille("", p![]).unwrap()).into_iter().map(|v| v.id).collect();
        assert_eq!(ids, ["recente", "moyenne", "vieille"]);
    }

    #[test]
    fn images_cles() {
        assert_eq!(moments(30.0), [0.3, 1.5, 3.0, 10.5, 18.0, 25.5]);
        assert_eq!(moments(2.0), [0.3, 0.7, 1.2, 1.5, 1.7, 1.8]);
        assert_eq!(moments(0.1), [0.0]);
    }

    #[test]
    fn cout_et_schema() {
        let u = Usage { entree: 50_000, cache_ecrit: 0, cache_lu: 0, sortie: 100_000 };
        assert!((u.usd(&HAIKU) - 0.055).abs() < 1e-12);
        assert!((u.usd(&SONNET) - 1.1).abs() < 1e-12);
        // le cache lu coûte un dixième de l'entrée sur Haiku ; au-delà de 100 k tokens de prompt, tout coûte 5 fois plus
        let cache = Usage { entree: 1_000, cache_ecrit: 2_000, cache_lu: 50_000, sortie: 500 };
        assert!((cache.usd(&HAIKU) - (100.0 + 250.0 + 500.0 + 250.0) / 1e6).abs() < 1e-12);
        let long = Usage { entree: 120_000, cache_ecrit: 0, cache_lu: 0, sortie: 0 };
        assert!((long.usd(&HAIKU) - 0.06).abs() < 1e-12);
        let s = schema(&["4-erreurs".into()]);
        assert_eq!(s["additionalProperties"], false);
        let blocs = &s["properties"]["blocs"];
        assert_eq!(blocs["additionalProperties"], false);
        assert_eq!(blocs["required"].as_array().unwrap().len(), 5);
        assert_eq!(s["properties"]["parts"]["required"].as_array().unwrap().len(), 5);
        assert_eq!(s["properties"]["format"]["enum"], json!(["4-erreurs", "nouveau"]));
    }

    #[test]
    fn lire_la_reponse() {
        let texte = serde_json::to_string(&analyse()).unwrap();
        let reponse = |texte: &str, arret: &str| -> Map<String, Value> {
            serde_json::from_value(json!({"stop_reason": arret, "content": [{"type": "text", "text": texte}],
                                          "usage": {"input_tokens": 3_900, "output_tokens": 700,
                                                    "cache_read_input_tokens": 100}})).unwrap()
        };
        assert_eq!(lire(&reponse(&texte, "end_turn")).unwrap(), analyse());
        assert_eq!(Usage::de(&reponse(&texte, "end_turn")), Usage { entree: 3_900, cache_ecrit: 0, cache_lu: 100, sortie: 700 });
        assert_eq!(Usage::de(&reponse(&texte, "end_turn")).prompt(), 4_000);
        assert!(lire(&reponse(&texte, "refusal")).is_err());
        assert!(lire(&reponse(&texte, "max_tokens")).is_err());
        assert!(lire(&reponse("{}", "end_turn")).is_err());
        let sans_app = texte.replace(r#""app":"Roleo""#, r#""app":" aucune ""#);
        assert_eq!(lire(&reponse(&sans_app, "end_turn")).unwrap().app, None);
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
