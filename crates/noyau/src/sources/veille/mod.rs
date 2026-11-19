//! Les vidéos qui ont marché sur des mots-clés, sur TikTok et Instagram. Voir docs/sources/veille.md.
pub mod analyse;
pub mod classement;
pub mod demande;
pub mod instagram;
pub mod parseurs;

use crate::collecte::http::{encoder, Requete, UA_NAVIGATEUR};
use crate::collecte::fournisseurs::Fournisseurs;
use crate::collecte::{miniatures, ytdlp, Contexte, Navigateur, Source};
use crate::erreur::{format_err, Erreur, Res};
use crate::jour::{self, Date};
use crate::modeles::{Evenement, Plateforme, TypeFil};
use crate::modeles_veille::{MotVeille, ScoreVeille, Trouvaille, VideoVeille};
use crate::p;
use crate::sources::tiktok;
use crate::store::Store;
use async_trait::async_trait;
use chrono::Duration;
use std::collections::{HashMap, HashSet};
use std::time::Duration as Delai;

/// Les mots cherchés par plateforme et par passage, les moins récemment cherchés d'abord : reste sous ~30 recherches par jour.
pub const MOTS_PAR_PASSAGE: usize = 4;
/// Les profils visités pour leur médiane, par plateforme et par passage.
pub const COMPTES_PAR_PASSAGE: i64 = 8;
/// Les analyses Claude par passage, les plus récentes d'abord : 40 par jour au plus.
pub const ANALYSES_PAR_PASSAGE: usize = 10;
/// Le curseur de l'écran Veille : les vidéos gardées par mot-clé et par plateforme, les plus récentes d'abord.
pub const REGLAGE_VIDEOS: &str = "veille.videos";
pub const VIDEOS_MIN: i64 = 10;
pub const VIDEOS_MAX: i64 = 100;
pub const VIDEOS_PAS: i64 = 10;
pub const VIDEOS_DEFAUT: i64 = 40;
/// Les vidéos voulues par défilement de la recherche : ni TikTok ni Instagram ne trient par date sur le web, on lit
/// une douzaine de vidéos par défilement pour en garder les plus récentes.
const VIDEOS_PAR_DEFILEMENT: i64 = 5;
/// La pause entre deux pages d'une page cachée, en secondes : un humain qui cherche, pas une rafale.
const PAUSE: (f64, f64) = (20.0, 40.0);
/// La pause entre deux recherches DuckDuckGo : un site qui limite, pas un site qui surveille.
const PAUSE_SANS_PAGE: (f64, f64) = (3.0, 6.0);
/// Au-delà, une vidéo n'entre plus dans la veille : la recherche ne trie pas par date, on écarte les vieilles.
pub const AGE_MAX_JOURS: i64 = 365;
pub const PLATEFORMES: [Plateforme; 2] = [Plateforme::Tiktok, Plateforme::Instagram];

/// L'id de la source, que réserve aussi une demande à Claude : l'une attend que l'autre ait fini.
pub const ID: &str = "veille";

pub struct Veille;

#[async_trait]
impl Source for Veille {
    fn id(&self) -> &'static str { ID }
    fn nom(&self) -> &'static str { "Veille" }
    fn cadence(&self) -> i64 { 6 * 3_600 }
    fn reprise(&self) -> i64 { 3 * 3_600 }

    async fn collecte(&self, ctx: &Contexte) -> Res<usize> {
        let store = &ctx.store;
        let maintenant = jour::maintenant();
        let suivis: HashSet<&str> = ctx.businesses.iter().map(|b| b.id.as_str()).collect();
        let mut mots: Vec<MotVeille> = store.mots_veille()?.into_iter().filter(|m| m.actif && suivis.contains(m.business.as_str())).collect();
        if mots.is_empty() {
            return Err(Erreur::NonConfigure("aucun mot-clé de veille, en ajouter dans l'écran Veille".into()));
        }
        mots.sort_by_key(|m| m.cherche.unwrap_or_else(jour::loin_passe));
        let a_chercher = &mots[..mots.len().min(MOTS_PAR_PASSAGE)];
        let voulues = videos_par_mot(store)?;

        let mut echecs: Vec<String> = vec![];
        if let Err(e) = ytdlp::tenir_a_jour(&ctx.http).await {
            echecs.push(format!("yt-dlp nightly : {e}"));
        }
        // une clé LurkAPI ou TikHub remplace la page cachée de TikTok et la session d'Instagram
        let fournisseurs = Fournisseurs::resoudre(ctx).await;
        // TikTok et Instagram en même temps : deux sites, chacun à son rythme, le passage dure le plus long des deux
        let bilans = futures::future::join_all(PLATEFORMES.map(|p| passer(ctx, p, a_chercher, voulues, &fournisseurs, &maintenant))).await;
        echecs.extend(bilans.iter().flat_map(|b| b.echecs.clone()));
        let lignes = bilans.iter().map(|b| b.lignes).sum();
        let mut images: HashMap<String, String> = HashMap::new();
        for (id, url) in bilans.into_iter().flat_map(|b| b.images) {
            images.entry(id).or_insert(url);
        }
        store.lot(|| {
            for m in a_chercher {
                store.cherche(&m.business, &m.mot, &maintenant)?;
            }
            Ok(())
        })?;

        for v in store.videos_veille("where v.publiee >= ?", p![maintenant - Duration::seconds(ScoreVeille::JEUNE_S)])? {
            if ScoreVeille::percee(&v, &maintenant) {
                store.fil(&Evenement {
                    id: format!("veille:{}", v.id), business: v.business.clone(), at: maintenant, type_: TypeFil::Veille,
                    texte: percee(&v), lien: Some(v.url.clone()),
                })?;
            }
        }
        echecs.extend(miniatures::telecharger(&images, &ctx.http).await);
        analyse::nettoyer_videos(&maintenant);
        echecs.extend(analyse::analyser(ctx, &maintenant).await);
        echecs.extend(classement::classer(ctx, &maintenant).await);
        // tous les échecs : l'écran coupe, la base garde de quoi diagnostiquer
        if !echecs.is_empty() {
            return Err(format_err(format!("{} échecs : {}", echecs.len(), echecs.join(" ; "))));
        }
        Ok(lignes)
    }
}

/// Ce qu'une plateforme rend d'un passage.
#[derive(Default)]
pub struct Bilan {
    pub lignes: usize,
    pub images: HashMap<String, String>,
    pub echecs: Vec<String>,
}

/// Le curseur de l'écran Veille, `VIDEOS_DEFAUT` sans réglage.
pub fn videos_par_mot(store: &Store) -> Res<i64> {
    Ok(store.reglage(REGLAGE_VIDEOS)?.and_then(|v| v.parse().ok()).unwrap_or(VIDEOS_DEFAUT).clamp(VIDEOS_MIN, VIDEOS_MAX))
}

/// Ce qui répond à une recherche : un fournisseur d'API, ou une page cachée, ou rien (DuckDuckGo d'Instagram).
struct Acces<'a> {
    fournisseurs: &'a Fournisseurs,
    nav: Option<&'a dyn Navigateur>,
}

/// Une plateforme : ses recherches, puis les profils à baser. Avec une clé LurkAPI ou TikHub, tout passe par l'API,
/// sans session ni navigateur ; sinon, la recherche TikTok exige une session (page cachée) et celle d'Instagram passe
/// par DuckDuckGo et les embeds.
pub async fn passer(ctx: &Contexte, p: Plateforme, a_chercher: &[MotVeille], voulues: i64, fournisseurs: &Fournisseurs, maintenant: &Date) -> Bilan {
    let mut b = Bilan::default();
    if !fournisseurs.configure() && p == Plateforme::Tiktok {
        match ouvrir(ctx, p).await {
            Ok(nav) => {
                chercher_mots(ctx, p, a_chercher, voulues, Acces { fournisseurs, nav: Some(nav.as_ref()) }, maintenant, &mut b).await;
                nav.fermer().await;
            }
            // la médiane des comptes passe par yt-dlp : elle se fait sans session
            Err(e) => b.echecs.push(format!("{} : {e}", p.nom())),
        }
    } else {
        chercher_mots(ctx, p, a_chercher, voulues, Acces { fournisseurs, nav: None }, maintenant, &mut b).await;
    }
    baser_comptes(ctx, p, fournisseurs, maintenant, &mut b).await;
    b
}

async fn chercher_mots(ctx: &Contexte, p: Plateforme, a_chercher: &[MotVeille], voulues: i64, acces: Acces<'_>, maintenant: &Date, b: &mut Bilan) {
    for (i, m) in a_chercher.iter().enumerate() {
        // un humain qui cherche, pas une rafale : la pause n'a de sens que devant une page ou un site qui limite
        if i > 0 && (acces.nav.is_some() || !acces.fournisseurs.configure()) {
            pause(acces.nav).await;
        }
        match chercher(ctx, p, m, voulues, acces.fournisseurs, acces.nav, maintenant).await {
            Ok(gardees) => {
                b.lignes += gardees.len();
                for t in gardees {
                    if let Some(image) = t.image.clone() {
                        b.images.insert(t.id(), image);
                    }
                }
            }
            Err(e) => b.echecs.push(format!("{} « {} » : {e}", p.nom(), m.mot)),
        }
    }
}

/// Les médianes des comptes à relire. Avec un fournisseur, l'API suffit. Sinon, Instagram lit la page Reels du profil
/// dans une page cachée : sans session, rien à faire, et rien à dire.
async fn baser_comptes(ctx: &Contexte, p: Plateforme, fournisseurs: &Fournisseurs, maintenant: &Date, b: &mut Bilan) {
    let comptes = match ctx.store.comptes_a_baser(p, COMPTES_PAR_PASSAGE, maintenant) {
        Ok(c) => c,
        Err(e) => {
            b.echecs.push(format!("{} comptes : {e}", p.nom()));
            return;
        }
    };
    if comptes.is_empty() {
        return;
    }
    let nav = if !fournisseurs.configure() && p == Plateforme::Instagram {
        match ouvrir(ctx, p).await {
            Ok(n) => Some(n),
            Err(Erreur::NonConfigure(_)) => return,
            Err(e) => {
                b.echecs.push(format!("{} comptes : {e}", p.nom()));
                return;
            }
        }
    } else {
        None
    };
    let mut de_suite = 0;
    for handle in comptes {
        match baser(ctx, p, &handle, fournisseurs, nav.as_deref(), maintenant).await {
            Ok(()) => {
                b.lignes += 1;
                de_suite = 0;
            }
            Err(e) => {
                b.echecs.push(format!("{} @{handle} : {e}", p.nom()));
                de_suite += 1;
            }
        }
        // une page qui échoue deux fois de suite (session morte, limite) échouera pour les suivantes
        if nav.is_some() && de_suite == 2 {
            break;
        }
    }
    if let Some(n) = nav {
        n.fermer().await;
    }
}

/// Une recherche : les vidéos gardées, écrites en base. Avec un fournisseur, l'API ; sinon, la page de recherche de
/// TikTok (`nav`) ou les reels d'Instagram trouvés par DuckDuckGo.
async fn chercher(ctx: &Contexte, p: Plateforme, m: &MotVeille, voulues: i64, fournisseurs: &Fournisseurs, nav: Option<&dyn Navigateur>, maintenant: &Date) -> Res<Vec<Trouvaille>> {
    let trouvees = if fournisseurs.configure() {
        parseurs::trouvailles_objets(p, &fournisseurs.chercher(&ctx.http, p, &m.mot, voulues).await?)
    } else {
        match nav {
            Some(nav) => {
                let defilements = (voulues / VIDEOS_PAR_DEFILEMENT) as u32;
                let trouvees = parseurs::trouvailles(p, &nav.lire(&recherche(&m.mot), 6.0, defilements).await?);
                if trouvees.is_empty() {
                    return Err(format_err(format!("aucune réponse capturée (page : {})", nav.ou().await)));
                }
                trouvees
            }
            None => instagram::chercher(&ctx.http, &m.mot).await?,
        }
    };
    let gardees = retenir(trouvees, m.vues_min, voulues, maintenant);
    ctx.store.lot(|| {
        for t in &gardees {
            ctx.store.trouvaille(t, &m.business, Some(&m.mot), maintenant)?;
        }
        Ok(())
    })?;
    Ok(gardees)
}

/// La médiane d'un compte relue sur son profil, et le relevé de ses vidéos déjà en base.
async fn baser(ctx: &Contexte, p: Plateforme, handle: &str, fournisseurs: &Fournisseurs, nav: Option<&dyn Navigateur>, maintenant: &Date) -> Res<()> {
    // sans fournisseur, Instagram passe par la page ; yt-dlp ne passe par aucune page
    if nav.is_some() {
        pause(nav).await;
    }
    let siennes = dernieres(ctx, p, handle, fournisseurs, nav, maintenant).await?;
    if siennes.is_empty() {
        return Err(format_err("aucune vidéo sur le profil"));
    }
    let vues: Vec<(Option<Date>, i64)> = siennes.iter().filter_map(|t| t.vues.map(|v| (t.publiee, v))).collect();
    let mediane = ScoreVeille::mediane(&vues, maintenant);
    let mut abonnes = siennes.iter().find_map(|t| t.abonnes);
    if abonnes.is_none() && p == Plateforme::Tiktok {
        abonnes = abonnes_tiktok(ctx, handle, maintenant).await;
    }
    ctx.store.lot(|| {
        ctx.store.base(p, handle, mediane, siennes.len() as i64, abonnes, maintenant)?;
        for t in &siennes {
            ctx.store.relever(t, maintenant)?;
        }
        Ok(())
    })
}

/// Les abonnés d'un compte TikTok, lus sur sa page publique (yt-dlp ne les donne pas) ; `None` si la page a changé.
async fn abonnes_tiktok(ctx: &Contexte, handle: &str, maintenant: &Date) -> Option<i64> {
    let page = ctx.http.donnees(&Requete::get(Plateforme::Tiktok.profil(handle)).entete("User-Agent", UA_NAVIGATEUR)).await.ok()?;
    tiktok::profil(&page, handle, maintenant).ok()?.0.abonnes
}

/// Une attente au hasard : longue devant une page cachée (`nav`), courte sans.
async fn pause(nav: Option<&dyn Navigateur>) {
    let (min, max) = if nav.is_some() { PAUSE } else { PAUSE_SANS_PAGE };
    let hasard = uuid::Uuid::new_v4().as_u128() as f64 / u128::MAX as f64;
    tokio::time::sleep(Delai::from_secs_f64(min + (max - min) * hasard)).await;
}

/// Un navigateur caché avec la session de la plateforme.
pub async fn ouvrir(ctx: &Contexte, p: Plateforme) -> Res<Box<dyn Navigateur>> {
    let domaine = if p == Plateforme::Tiktok { "tiktok.com" } else { "instagram.com" };
    let cookies = ctx.hote.cookies(domaine, "sessionid")?;
    let filtres: Vec<String> = if p == Plateforme::Tiktok { vec!["/api/search/".into()] } else { vec!["/api/v1/".into(), "/graphql".into()] };
    ctx.hote.navigateur(&filtres, &cookies, &format!(".{domaine}")).await
}

/// La page de recherche de vidéos de TikTok.
pub fn recherche(mot: &str) -> String { format!("https://www.tiktok.com/search/video?q={}", encoder(mot)) }

/// Les dernières vidéos d'un compte, pour sa médiane. Avec un fournisseur, l'API. Sinon : TikTok par `yt-dlp` (sa page
/// de profil ne rend pas la liste à une page cachée) ; Instagram, sa page Reels dans la page cachée (`clips_connection`,
/// 12 reels avec leurs vues). `web_profile_info` répond 404 depuis le 18 nov. 2026, même à la page.
pub async fn dernieres(ctx: &Contexte, p: Plateforme, handle: &str, fournisseurs: &Fournisseurs, nav: Option<&dyn Navigateur>, maintenant: &Date) -> Res<Vec<Trouvaille>> {
    if fournisseurs.configure() {
        return Ok(parseurs::trouvailles_objets(p, &fournisseurs.dernieres(&ctx.http, p, handle, 20).await?));
    }
    match p {
        Plateforme::Instagram => {
            let nav = nav.ok_or_else(|| Erreur::NonConfigure("la médiane d'Instagram demande une session".into()))?;
            let corps = nav.lire(&format!("https://www.instagram.com/{handle}/reels/"), 5.0, 1).await?;
            // la page charge aussi le fil d'accueil : seulement les reels du compte
            let handle = handle.to_lowercase();
            Ok(parseurs::trouvailles(p, &corps).into_iter().filter(|t| t.compte.to_lowercase() == handle).collect())
        }
        _ => {
            let l = ytdlp::lire_liste(&ytdlp::liste(&p.profil(handle), Some(20)).await?, p, "", handle, maintenant)?;
            Ok(parseurs::trouvailles_liste(&l, p, handle, l.abonnes))
        }
    }
}

/// « @compte · 120 k vues, 6× sa médiane · « mot » ».
pub fn percee(v: &VideoVeille) -> String {
    let mot = v.mot.as_ref().map(|m| format!(" · « {m} »")).unwrap_or_default();
    format!("@{} · {} vues, {}× sa médiane{mot}", v.compte, compact(v.vues.unwrap_or(0)), decimale(v.ratio().unwrap_or(0.0)))
}

/// Un nombre au plus à une décimale, à la française : `6`, `6,5`.
fn decimale(x: f64) -> String {
    let r = (x * 10.0).round() / 10.0;
    if r.fract() == 0.0 { format!("{r:.0}") } else { format!("{r:.1}").replace('.', ",") }
}

/// Un compteur abrégé comme le fait macOS en français : `999`, `1,2 k`, `120 k`, `1,5 M`, `2 Md`.
fn compact(n: i64) -> String {
    const UNITES: [&str; 4] = ["", "k", "M", "Md"];
    let arrondi = |v: f64| if v.abs() < 10.0 { (v * 10.0).round() / 10.0 } else { v.round() };
    let mut v = n as f64;
    let mut i = 0;
    while v.abs() >= 1_000.0 && i < UNITES.len() - 1 {
        v /= 1_000.0;
        i += 1;
    }
    if i > 0 && arrondi(v).abs() >= 1_000.0 && i < UNITES.len() - 1 {
        v /= 1_000.0;
        i += 1;
    }
    if i == 0 {
        return n.to_string();
    }
    format!("{}\u{a0}{}", decimale(arrondi(v)), UNITES[i])
}

/// Les `voulues` vidéos les plus récentes de la recherche parmi celles que `garde` retient, de moins d'un an ; une date
/// inconnue passe après les autres.
pub fn retenir(trouvees: Vec<Trouvaille>, vues_min: i64, voulues: i64, maintenant: &Date) -> Vec<Trouvaille> {
    let limite = *maintenant - Duration::days(AGE_MAX_JOURS);
    let mut gardees: Vec<Trouvaille> = trouvees.into_iter().filter(|t| garde(t, vues_min) && t.publiee.is_none_or(|d| d >= limite)).collect();
    gardees.sort_by_key(|t| std::cmp::Reverse(t.publiee));
    gardees.truncate(voulues as usize);
    gardees
}

/// Garde une vidéo à partir de `vues_min` vues ; sans vues (recherche Instagram), à partir de `vues_min / 50` likes.
pub fn garde(t: &Trouvaille, vues_min: i64) -> bool {
    match t.vues {
        Some(vues) => vues >= vues_min,
        None => t.likes.unwrap_or(0) >= vues_min / 50,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn t0() -> Date { Utc.timestamp_opt(1_800_000_000, 0).unwrap() }

    #[test]
    fn garde_les_vues_ou_les_likes() {
        let mut t = Trouvaille::nouvelle(Plateforme::Instagram, "A", "x");
        t.vues = Some(19_999);
        assert!(!garde(&t, 20_000));
        t.vues = None;
        t.likes = Some(400);
        assert!(garde(&t, 20_000));
        t.likes = Some(399);
        assert!(!garde(&t, 20_000));
    }

    #[test]
    fn retient_les_plus_recentes() {
        let video = |brut: &str, jours: Option<i64>, vues: i64| {
            let mut t = Trouvaille::nouvelle(Plateforme::Tiktok, brut, "x");
            t.publiee = jours.map(|j| t0() - Duration::days(j));
            t.vues = Some(vues);
            t
        };
        let trouvees = vec![video("vieille", Some(400), 90_000), video("sans-date", None, 90_000), video("faible", Some(1), 100), video("recente", Some(2), 30_000), video("moyenne", Some(40), 50_000)];
        let ids = |v: Vec<Trouvaille>| v.into_iter().map(|t| t.brut).collect::<Vec<_>>();
        assert_eq!(ids(retenir(trouvees.clone(), 20_000, 2, &t0())), ["recente", "moyenne"]);
        // la vidéo de 400 jours est écartée, la date inconnue passe en dernier
        assert_eq!(ids(retenir(trouvees, 20_000, 10, &t0())), ["recente", "moyenne", "sans-date"]);
    }

    #[test]
    fn curseur_borne_et_par_defaut() {
        let s = Store::memoire().unwrap();
        assert_eq!(videos_par_mot(&s).unwrap(), VIDEOS_DEFAUT);
        s.regler(REGLAGE_VIDEOS, Some("1000")).unwrap();
        assert_eq!(videos_par_mot(&s).unwrap(), VIDEOS_MAX);
        s.regler(REGLAGE_VIDEOS, Some("x")).unwrap();
        assert_eq!(videos_par_mot(&s).unwrap(), VIDEOS_DEFAUT);
    }

    #[test]
    fn mediane_sans_les_jeunes_videos() {
        let vieux = Some(t0() - Duration::days(3));
        let jeune = Some(t0() - Duration::hours(1));
        let videos = vec![(vieux, 100), (vieux, 300), (vieux, 200), (vieux, 500), (jeune, 9_000_000)];
        assert_eq!(ScoreVeille::mediane(&videos, &t0()), None);
        assert_eq!(ScoreVeille::mediane(&[videos.clone(), vec![(vieux, 400)]].concat(), &t0()), Some(300));
        assert_eq!(ScoreVeille::mediane(&[videos, vec![(vieux, 400), (None, 600)]].concat(), &t0()), Some(350));
    }

    #[test]
    fn pull_et_outlier() {
        assert_eq!(ScoreVeille::pull(Some(100_000), Some(200), Some(50), Some(25), Some(25)), Some(0.005));
        assert!(ScoreVeille::outlier(Some(3.0), Some(10_000), Some(0.005)));
        assert!(!ScoreVeille::outlier(Some(2.9), Some(50_000), Some(0.05)));
        assert!(!ScoreVeille::outlier(Some(10.0), Some(9_999), Some(0.05)));
        assert!(!ScoreVeille::outlier(Some(10.0), Some(50_000), Some(0.004)));
        assert_eq!(ScoreVeille::pull(Some(0), Some(1), None, None, None), None);
    }

    #[test]
    fn aller_retour_et_vitesse() {
        let store = Store::memoire().unwrap();
        let mut t = Trouvaille::nouvelle(Plateforme::Tiktok, "1", "profe.ana");
        t.publiee = Some(t0() - Duration::hours(1));
        (t.vues, t.likes, t.commentaires, t.partages, t.enregistrements) = (Some(40_000), Some(2_000), Some(10), Some(10), Some(10));
        store.ajouter_mot("roleo", "espagnol", 20_000).unwrap();
        store.trouvaille(&t, "roleo", Some("espagnol"), &t0()).unwrap();
        store.base(Plateforme::Tiktok, "profe.ana", Some(10_000), 20, Some(3_000), &t0()).unwrap();
        t.vues = Some(100_000);
        store.relever(&t, &(t0() + Duration::hours(2))).unwrap();
        let mut inconnue = Trouvaille::nouvelle(Plateforme::Tiktok, "inconnue", "x");
        inconnue.vues = Some(5);
        store.relever(&inconnue, &t0()).unwrap();

        let vs = store.videos_veille("", p![]).unwrap();
        assert_eq!(vs.len(), 1);
        let v = &vs[0];
        assert_eq!(v.vues, Some(100_000));
        assert_eq!(v.vitesse, Some(30_000.0));
        assert_eq!(v.ratio(), Some(10.0));
        assert_eq!(v.abonnes, Some(3_000));
        assert_eq!(v.mot.as_deref(), Some("espagnol"));
        assert!(v.outlier());
        assert!(ScoreVeille::percee(v, &(t0() + Duration::hours(2))));
        assert_eq!(percee(v), "@profe.ana · 100\u{a0}k vues, 10× sa médiane · « espagnol »");
        assert!(store.comptes_a_baser(Plateforme::Tiktok, 8, &t0()).unwrap().is_empty());
        assert_eq!(store.comptes_a_baser(Plateforme::Tiktok, 8, &(t0() + Duration::days(8))).unwrap(), ["profe.ana"]);

        let a = analyse::tests::analyse();
        store.analyse_veille(&v.id, &t0(), analyse::HAIKU.modele, &a).unwrap();
        assert_eq!(store.videos_veille("", p![]).unwrap()[0].analyse, Some(a));
        for _ in 0..2 {
            store.cout_veille(&v.id, &t0(), analyse::HAIKU.modele, 4_000, 800, 0.0008).unwrap();
        }
        assert!((store.depense_veille_usd().unwrap() - 0.0016).abs() < 1e-9);
    }

    #[test]
    fn mots_cles() {
        let store = Store::memoire().unwrap();
        store.ajouter_mot("roleo", "  espagnol  ", 5_000).unwrap();
        store.ajouter_mot("roleo", "", 20_000).unwrap();
        store.activer_mot("roleo", "espagnol", false).unwrap();
        store.ajouter_mot("roleo", "espagnol", 8_000).unwrap();
        let mots = store.mots_veille().unwrap();
        assert_eq!(mots.iter().map(|m| m.mot.as_str()).collect::<Vec<_>>(), ["espagnol"]);
        assert_eq!(mots[0].vues_min, 8_000);
        assert!(mots[0].actif);
        store.retirer_mot("roleo", "espagnol").unwrap();
        assert!(store.mots_veille().unwrap().is_empty());
    }

    #[test]
    fn nombres_abreges() {
        assert_eq!(compact(999), "999");
        assert_eq!(compact(1_234), "1,2\u{a0}k");
        assert_eq!(compact(120_000), "120\u{a0}k");
        assert_eq!(compact(999_999), "1\u{a0}M");
        assert_eq!(compact(1_500_000), "1,5\u{a0}M");
        assert_eq!(decimale(6.0), "6");
        assert_eq!(decimale(6.04), "6");
        assert_eq!(recherche("ser o estar"), "https://www.tiktok.com/search/video?q=ser%20o%20estar");
    }
}
