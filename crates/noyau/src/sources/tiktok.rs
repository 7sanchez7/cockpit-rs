//! Voir docs/sources/tiktok.md.
use crate::collecte::http::{self, encoder, extrait, UA_NAVIGATEUR};
use crate::collecte::{miniatures, ytdlp, Contexte, Http, Requete, Source};
use crate::erreur::{format_err, Erreur, Res};
use crate::jour::{self, Date};
use crate::metrique;
use crate::modeles::{Analyse, Compte, Evenement, Part, Plateforme, Releve, TypeFil, Video};
use crate::p;
use crate::store::Store;
use async_trait::async_trait;
use chrono::{Days, Duration, Utc};
use chrono_tz::Europe::Paris;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

pub struct TikTok;

pub const ANALYTICS: &str = "https://www.tiktok.com/tiktokstudio/analytics";
/// Les types de la page Analytics d'une vidéo : TikTok ne rend les `_realtime` que demandés tous ensemble.
pub const TYPES: [&str; 19] = [
    "video_info", "item_search_terms", "video_traffic_source_percent_realtime", "video_retention_rate_realtime",
    "video_view_realtime", "video_total_duration_realtime", "video_per_duration_realtime", "video_finish_rate_realtime",
    "video_new_follower_realtime", "video_vv_history_7d", "video_vv_history_48_hours", "video_uv",
    "video_viewer_new_viewer_percent", "video_viewer_follower_percent_realtime", "video_viewer_nonfollower_percent_realtime",
    "video_viewer_age_percent_realtime", "video_viewer_gender_percent_realtime", "video_viewer_location_percent_realtime",
    "video_like_distribution_realtime",
];
pub const PALIERS: [i64; 4] = [1_000, 10_000, 100_000, 1_000_000];
/// Une vidéo plus jeune est relevée à chaque passage, une plus vieille une fois par jour.
const RECENTE_S: i64 = 14 * 86_400;

#[async_trait]
impl Source for TikTok {
    fn id(&self) -> &'static str { "tiktok" }
    fn nom(&self) -> &'static str { "TikTok" }
    fn cadence(&self) -> i64 { 30 * 60 }

    async fn collecte(&self, ctx: &Contexte) -> Res<usize> {
        let store = &ctx.store;
        let maintenant = jour::maintenant();
        let comptes = comptes_suivis(ctx, Plateforme::Tiktok, "aucun compte TikTok suivi, en ajouter dans Vidéos")?;

        let mut echecs = vec![];
        let mut liste = ytdlp::Liste::default();
        let mut profils = vec![];
        let mut images = HashMap::new();
        for c in &comptes {
            let url = c.plateforme.profil(&c.handle);
            match ytdlp::liste(&url, None).await.and_then(|s| ytdlp::lire_liste(&s, Plateforme::Tiktok, &c.business, &c.handle, &maintenant)) {
                Ok(l) => {
                    liste.videos.extend(l.videos);
                    liste.releves.extend(l.releves);
                    fusionner(&mut images, l.images);
                }
                Err(e) => echecs.push(format!("@{} : {e}", c.handle)),
            }
            match ctx.http.donnees(&Requete::get(url).entete("User-Agent", UA_NAVIGATEUR)).await.and_then(|page| profil(&page, &c.id(), &maintenant)) {
                Ok((r, avatar)) => {
                    profils.push(r);
                    if let Some(a) = avatar {
                        images.insert(miniatures::avatar(&c.id()), a);
                    }
                }
                Err(e) => echecs.push(format!("profil @{} : {e}", c.handle)),
            }
        }
        let minuit = jour::date(&jour::cle(&maintenant)).unwrap_or(maintenant);
        let relevees_aujourdhui: HashSet<String> = store.db.query("select distinct video from video_releve where at >= ?", p![minuit])?
            .iter().filter_map(|l| l.texte("video")).collect();
        let mut par_id: HashMap<String, Video> = HashMap::new();
        for v in &liste.videos {
            par_id.entry(v.id.clone()).or_insert_with(|| v.clone());
        }
        let a_relever = |id: &str| {
            let publiee = par_id.get(id).and_then(|v| v.publiee_le).unwrap_or(maintenant);
            maintenant - publiee < Duration::seconds(RECENTE_S) || !relevees_aujourdhui.contains(id)
        };
        let mut releves: BTreeMap<String, Releve> = BTreeMap::new();
        for r in liste.releves.iter().filter(|r| a_relever(&r.video)) {
            releves.entry(r.video.clone()).or_insert_with(|| r.clone());
        }
        let mut analyses: BTreeMap<String, Analyse> = BTreeMap::new();

        // stats privées (watch time, abonnés gagnés) : seulement le compte connecté dans Safari
        let mut sans_studio = None;
        let mut studio: Option<(String, Vec<Option<i64>>)> = None;
        let prive: Res<()> = async {
            let cookie = http::entete_cookie(&ctx.hote.cookies("tiktok.com", "sessionid")?);
            let session = compte_session(&cookie, &ctx.http).await?.to_lowercase();
            let Some(c) = comptes.iter().find(|c| c.handle == session) else {
                sans_studio = Some(format!("stats privées : @{session} connecté dans Safari n'est pas suivi"));
                return Ok(());
            };
            for v in liste.videos.iter().filter(|v| v.compte == session && a_relever(&v.id)) {
                let lu: Res<(Releve, Analyse)> = async {
                    let demandes: Vec<Value> = TYPES.iter().map(|t| json!({"insigh_type": t, "aweme_id": v.id})).collect();
                    let r = requete(&demandes, &format!("{ANALYTICS}/{}/overview", v.id), &cookie);
                    let mut reponse = ctx.http.donnees(&r).await?;
                    if !complete(&reponse) {
                        // les types temps réel arrivent parfois vides au premier appel
                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                        reponse = ctx.http.donnees(&r).await?;
                    }
                    Ok((releve(&reponse, &v.id, &maintenant)?, analyse(&reponse)?))
                }
                .await;
                match lu {
                    Ok((r, a)) => {
                        releves.insert(v.id.clone(), r);
                        analyses.insert(v.id.clone(), a);
                    }
                    Err(e) => echecs.push(format!("stats {} : {e}", v.id)),
                }
                // pas de rafale sur l'API interne
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            }
            let demande = [json!({"insigh_type": "vv_history", "days": 7, "end_days": 1})];
            let h = historique(&ctx.http.donnees(&requete(&demande, &format!("{ANALYTICS}/overview"), &cookie)).await?)?;
            studio = Some((c.business.clone(), h));
            Ok(())
        }
        .await;
        match prive {
            Ok(()) => {}
            Err(Erreur::NonConfigure(m)) => sans_studio = Some(format!("stats privées : {m}")),
            Err(e) => echecs.push(format!("stats privées : {e}")),
        }
        echecs.extend(miniatures::telecharger(&images, &ctx.http).await);

        let titres: HashMap<String, String> = store.db.query("select id, titre from video where plateforme = 'tiktok' and titre is not null", p![])?
            .iter().filter_map(|l| l.texte("id").map(|id| (id, l.texte("titre").unwrap_or_default()))).collect();
        let titre_de = |v: &Video| titre(v.legende.as_deref(), titres.get(&v.id).map(String::as_str));
        let lignes = store.lot(|| {
            publier(store, &liste.videos, "tiktok", |v| {
                format!("Nouvelle vidéo sur @{}{}", v.compte, titre_de(v).map(|t| format!(" : {t}")).unwrap_or_default())
            })?;
            for r in releves.values() {
                let publiee = par_id.get(&r.video).and_then(|v| v.publiee_le).unwrap_or_else(jour::loin_passe);
                let recente = maintenant - publiee < Duration::seconds(86_400);
                let avant = store.db.query("select vues from video_releve where video = ? and at < ? order by at desc limit 1", p![&r.video, maintenant])?
                    .first().and_then(|l| l.entier("vues")).or(if recente { Some(0) } else { None });
                store.releve(r)?;
                let Some(avant) = avant else { continue };
                let Some(franchi) = palier(avant, r.vues) else { continue };
                let Some(v) = par_id.get(&r.video) else { continue };
                let quoi = titre_de(v).map(|t| format!("« {t} »")).unwrap_or_else(|| format!("une vidéo de @{}", v.compte));
                store.fil(&Evenement {
                    id: format!("tiktok:palier:{}:{franchi}", v.id),
                    business: v.business.clone(),
                    at: maintenant,
                    type_: TypeFil::Video,
                    texte: format!("{} vues pour {quoi}", libelle(franchi)),
                    lien: v.url.clone(),
                })?;
            }
            for p in &profils {
                store.releve_compte(p)?;
            }
            for (id, a) in analyses.iter().filter(|(_, a)| !a.est_vide()) {
                store.analyse(id, &maintenant, a)?;
            }
            let mut lignes = liste.videos.len() + releves.len() + profils.len() + analyses.len();
            let Some((b, historique)) = &studio else { return Ok(lignes) };
            // les 7 jours d'avant, exacts, pour le compte connecté ; TikTok coupe ses jours à minuit UTC
            let aujourdhui = jour::cle(&maintenant);
            let aujourdhui_utc = &jour::iso(&maintenant)[..10];
            let n = historique.len() as i64;
            for (i, vues) in historique.iter().enumerate() {
                let Some(vues) = vues else { continue };
                store.serie(b, metrique::VUES_TIKTOK, &jour::decale(aujourdhui_utc, i as i64 - n), -1, *vues as f64)?;
                lignes += 1;
            }
            if let Some(vues) = vues_gagnees(store, b, &aujourdhui, &maintenant)? {
                store.serie(b, metrique::VUES_TIKTOK, &aujourdhui, -1, vues as f64)?;
                lignes += 1;
            }
            if let Some(il_y_a_7) = maintenant.with_timezone(&Paris).checked_sub_days(Days::new(7)).map(|d| d.with_timezone(&Utc)) {
                if let Some(vues) = vues_gagnees(store, b, &jour::cle(&il_y_a_7), &il_y_a_7)? {
                    store.serie(b, metrique::VUES_TIKTOK_J7, &aujourdhui, -1, vues as f64)?;
                    lignes += 1;
                }
            }
            Ok(lignes)
        })?;
        sans_echec(&echecs)?;
        if let Some(m) = sans_studio {
            return Err(Erreur::NonConfigure(m));
        }
        Ok(lignes)
    }
}

// communs aux sources vidéo

/// Les comptes actifs d'une plateforme pour les businesses suivis ; aucun rend `NonConfigure(vide)`.
pub(super) fn comptes_suivis(ctx: &Contexte, plateforme: Plateforme, vide: &str) -> Res<Vec<Compte>> {
    let comptes: Vec<Compte> = ctx.store.comptes()?.into_iter()
        .filter(|c| c.plateforme == plateforme && c.actif && ctx.businesses.iter().any(|b| b.id == c.business))
        .collect();
    if comptes.is_empty() {
        return Err(Erreur::NonConfigure(vide.into()));
    }
    Ok(comptes)
}

/// Ajoute les images de `nouvelles` sans remplacer celles déjà là.
pub(super) fn fusionner<V>(images: &mut HashMap<String, V>, nouvelles: HashMap<String, V>) {
    for (k, v) in nouvelles {
        images.entry(k).or_insert(v);
    }
}

/// Écrit les vidéos et, pour chaque vidéo datée, sa sortie dans le fil (`<source>:pub:<id>`).
pub(super) fn publier(store: &Store, videos: &[Video], source: &str, texte: impl Fn(&Video) -> String) -> Res<()> {
    for v in videos {
        store.video(v)?;
        let Some(at) = v.publiee_le else { continue };
        store.fil(&Evenement {
            id: format!("{source}:pub:{}", v.id),
            business: v.business.clone(),
            at,
            type_: TypeFil::Video,
            texte: texte(v),
            lien: v.url.clone(),
        })?;
    }
    Ok(())
}

/// « : » suivi des 60 premiers caractères de la légende, rien sans légende.
pub(super) fn debut_legende(legende: &Option<String>) -> String {
    legende.as_ref().map(|l| format!(" : {}", l.chars().take(60).collect::<String>())).unwrap_or_default()
}

/// Un passage avec des échecs échoue sur le premier, après avoir écrit le reste.
pub(super) fn sans_echec(echecs: &[String]) -> Res<()> {
    match echecs.first() {
        Some(e) => Err(format_err(format!("{} échecs, dont {e}", echecs.len()))),
        None => Ok(()),
    }
}

// requêtes

/// Une requête à l'API interne de TikTok Studio, celle qu'appelle la page Analytics.
pub fn requete(demandes: &[Value], referer: &str, cookie: &str) -> Requete {
    let types = Value::Array(demandes.to_vec()).to_string();
    Requete::get(format!(
        "https://www.tiktok.com/aweme/v2/data/insight/?aid=1988&app_name=tiktok_creator_center&device_platform=web_pc&tz_name=Europe/Paris&type_requests={}",
        encoder(&types)
    ))
    .entete("Cookie", cookie)
    .entete("Referer", referer)
    .entete("User-Agent", UA_NAVIGATEUR)
}

/// Le nom du compte connecté dans Safari : seules ses vidéos ont des stats.
async fn compte_session(cookie: &str, http: &Http) -> Res<String> {
    let o = http
        .objet(&Requete::get("https://www.tiktok.com/passport/web/account/info/?aid=1988&app_name=tiktok_creator_center&device_platform=web_pc")
            .entete("Cookie", cookie)
            .entete("Referer", &format!("{ANALYTICS}/overview"))
            .entete("User-Agent", UA_NAVIGATEUR))
        .await?;
    match o.get("data").and_then(|d| d.get("username")).and_then(Value::as_str) {
        Some(nom) if !nom.is_empty() => Ok(nom.to_string()),
        _ => Err(format_err("session TikTok de Safari expirée, se reconnecter sur tiktok.com dans Safari")),
    }
}

// décodage

/// Les compteurs d'un compte et son avatar, lus dans le JSON de sa page publique.
pub fn profil(page: &[u8], compte: &str, at: &Date) -> Res<(crate::modeles::ReleveCompte, Option<String>)> {
    let html = String::from_utf8_lossy(page);
    let lu = || -> Option<(Map<String, Value>, Option<Value>)> {
        let marque = r#"id="__UNIVERSAL_DATA_FOR_REHYDRATION__""#;
        let debut = html.find(marque)? + marque.len();
        let ouvre = debut + html[debut..].find('>')? + 1;
        let ferme = ouvre + html[ouvre..].find("</script>")?;
        let o: Value = serde_json::from_str(&html[ouvre..ferme]).ok()?;
        let info = o.get("__DEFAULT_SCOPE__")?.get("webapp.user-detail")?.get("userInfo")?;
        Some((info.get("stats")?.as_object()?.clone(), info.get("user").cloned()))
    };
    let Some((stats, user)) = lu() else { return Err(format_err("page de profil sans compteurs")) };
    let entier = |k: &str| stats.get(k).and_then(Value::as_i64);
    let image = |k: &str| user.as_ref().and_then(|u| u.get(k)).and_then(Value::as_str).map(String::from);
    let r = crate::modeles::ReleveCompte {
        compte: compte.into(),
        at: *at,
        abonnes: entier("followerCount"),
        videos: entier("videoCount"),
        likes: entier("heartCount").or(entier("heart")),
    };
    Ok((r, image("avatarMedium").or(image("avatarThumb"))))
}

/// La réponse porte les types temps réel : sinon TikTok ne les avait pas encore prêts.
pub fn complete(reponse: &[u8]) -> bool {
    serde_json::from_slice::<Map<String, Value>>(reponse).is_ok_and(|o| o.contains_key("realtime_total_video_views"))
}

/// La valeur d'un type quand TikTok l'a calculée (`status` 0) : `{status, value}`, ou sous `value` pour un `_realtime`.
fn champ<'a>(o: &'a Map<String, Value>, cle: &str) -> Option<&'a Value> {
    let mut c = o.get(cle)?.as_object()?;
    if !c.contains_key("status") {
        if let Some(v) = c.get("value").and_then(Value::as_object) {
            c = v;
        }
    }
    if c.get("status").and_then(Value::as_i64) != Some(0) {
        return None;
    }
    c.get("value")
}

/// Le relevé d'une vidéo dans une réponse `insight`.
pub fn releve(reponse: &[u8], video: &str, at: &Date) -> Res<Releve> {
    let o = objet(reponse)?;
    let stats = o.get("video_info").and_then(|v| v.get("statistics")).and_then(Value::as_object);
    let Some((stats, vues)) = stats.and_then(|s| Some((s, s.get("play_count")?.as_i64()?))) else {
        return Err(format_err("insight : pas de statistiques"));
    };
    let entier = |k: &str| stats.get(k).and_then(Value::as_i64);
    Ok(Releve {
        video: video.into(),
        at: *at,
        vues,
        likes: entier("digg_count"),
        commentaires: entier("comment_count"),
        partages: entier("share_count"),
        enregistrements: entier("collect_count"),
        watch_moyen_s: champ(&o, "video_per_duration_realtime").and_then(Value::as_f64),
        completion: champ(&o, "video_finish_rate_realtime").and_then(Value::as_f64),
        abonnes: champ(&o, "realtime_new_followers").and_then(Value::as_i64),
    })
}

/// Ce que la page Analytics d'une vidéo montre au-delà des compteurs : rétention, sources, public.
pub fn analyse(reponse: &[u8]) -> Res<Analyse> {
    let o = objet(reponse)?;
    let parts = |v: Option<&Value>, nom: &str, part: &str| -> Vec<Part> {
        v.and_then(Value::as_array).into_iter().flatten()
            .filter_map(|p| Some(Part { nom: p.get(nom)?.as_str()?.into(), part: p.get(part)?.as_f64()? }))
            .collect()
    };
    let liste = |cle: &str, sous: &str| o.get(cle).and_then(|c| c.get("value")).and_then(|c| c.get(sous));
    let mut courbe: Vec<(f64, Option<f64>)> = liste("video_retention_rate_realtime", "list").and_then(Value::as_array).into_iter().flatten()
        .filter_map(|p| Some((p.get("timestamp")?.as_str()?.parse::<f64>().ok()?, p.get("value").and_then(Value::as_f64))))
        .collect();
    courbe.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(Analyse {
        watch_total_s: champ(&o, "video_total_duration_realtime").and_then(Value::as_f64),
        spectateurs: champ(&o, "video_uv").and_then(Value::as_i64),
        nouveaux: champ(&o, "video_viewer_new_viewer_percent").and_then(Value::as_f64),
        abonnes_part: champ(&o, "video_viewer_follower_percent_realtime").and_then(Value::as_f64),
        retention: courbe.into_iter().filter_map(|(_, v)| v).collect(),
        sources: parts(champ(&o, "video_traffic_source_percent_realtime"), "key", "value"),
        recherches: parts(champ(&o, "item_search_terms"), "key", "value"),
        ages: parts(champ(&o, "video_viewer_age_percent_realtime"), "key", "value"),
        pays: parts(liste("video_viewer_location_percent_realtime", "country_percent_list"), "country_name", "country_vv_percent"),
        jalons: vec![],
        vues_par_jour: o.get("video_vv_history_7d").and_then(Value::as_array).into_iter().flatten()
            .filter(|j| j.get("status").and_then(Value::as_i64) == Some(0))
            .filter_map(|j| j.get("value").and_then(Value::as_i64))
            .collect(),
    })
}

/// Une réponse `insight` réussie.
fn objet(reponse: &[u8]) -> Res<Map<String, Value>> {
    let o: Map<String, Value> = serde_json::from_slice(reponse).map_err(|_| format_err("insight : pas un objet JSON"))?;
    let code = o.get("status_code").and_then(Value::as_i64);
    if code != Some(0) {
        let raison = o.get("status_msg").and_then(Value::as_str).unwrap_or("refus");
        return Err(format_err(format!("insight : {raison} (status {})", code.unwrap_or(-1))));
    }
    Ok(o)
}

/// Les vues du compte jour par jour dans une réponse `vv_history`, la dernière = hier ; `None` pour un jour pas encore calculé.
pub fn historique(reponse: &[u8]) -> Res<Vec<Option<i64>>> {
    let o: Option<Map<String, Value>> = serde_json::from_slice(reponse).ok();
    let jours = o.as_ref()
        .filter(|o| o.get("status_code").and_then(Value::as_i64) == Some(0))
        .and_then(|o| o.get("vv_history")?.as_array());
    let Some(jours) = jours else { return Err(format_err(format!("vv_history : {}", extrait(reponse, 200)))) };
    Ok(jours.iter()
        .map(|j| if j.get("status").and_then(Value::as_i64) == Some(0) { j.get("value").and_then(Value::as_i64) } else { None })
        .collect())
}

// calculs

/// Vues gagnées par les vidéos du business le `jour`, de minuit à `jusqua`, d'après les relevés.
/// Départ d'une vidéo : son dernier relevé d'avant minuit, 0 si elle est sortie ce jour-là, sinon son premier relevé du jour.
/// `None` sans aucun relevé ce jour-là avant `jusqua`.
pub fn vues_gagnees(store: &Store, business: &str, jour: &str, jusqua: &Date) -> Res<Option<i64>> {
    let Some(minuit) = jour::date(jour) else { return Ok(None) };
    let lignes = store.db.query("
        select v.publiee_le,
            (select vues from video_releve where video = v.id and at >= ?1 and at <= ?2 order by at desc limit 1) as fin,
            (select vues from video_releve where video = v.id and at >= ?1 and at <= ?2 order by at limit 1) as premier,
            (select vues from video_releve where video = v.id and at < ?1 order by at desc limit 1) as veille
        from video v where v.business = ?3 and v.plateforme = 'tiktok'", p![minuit, jusqua, business])?;
    let mut total = None;
    for l in lignes {
        let Some(fin) = l.entier("fin") else { continue };
        let sortie_ce_jour = l.date("publiee_le").unwrap_or_else(jour::loin_passe) >= minuit;
        let depart = l.entier("veille").unwrap_or(if sortie_ce_jour { 0 } else { l.entier("premier").unwrap_or(fin) });
        total = Some(total.unwrap_or(0) + (fin - depart).max(0));
    }
    Ok(total)
}

/// Le plus haut palier franchi entre deux relevés.
pub fn palier(avant: i64, apres: i64) -> Option<i64> {
    PALIERS.iter().rev().find(|&&p| avant < p && p <= apres).copied()
}

/// `10 000`, groupé à la française (espace fine insécable), ou « 1 million de ».
pub fn libelle(palier: i64) -> String {
    if palier == 1_000_000 {
        return "1 million de".into();
    }
    let chiffres = palier.to_string();
    let mut s = String::new();
    for (i, c) in chiffres.chars().enumerate() {
        if i > 0 && (chiffres.len() - i).is_multiple_of(3) {
            s.push('\u{202F}');
        }
        s.push(c);
    }
    s
}

/// Le titre de la fiche, sinon le début de la légende sans les hashtags.
pub fn titre(legende: Option<&str>, fiche: Option<&str>) -> Option<String> {
    if let Some(f) = fiche.filter(|f| !f.is_empty()) {
        return Some(f.into());
    }
    let mots = legende.unwrap_or("").split_whitespace().filter(|m| !m.starts_with('#')).collect::<Vec<_>>().join(" ");
    if mots.is_empty() {
        return None;
    }
    Some(if mots.chars().count() > 60 { mots.chars().take(59).collect::<String>() + "…" } else { mots })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn a(secondes: i64) -> Date { Utc.timestamp_opt(secondes, 0).unwrap() }

    #[test]
    fn liste_yt_dlp() {
        let at = a(1794527444);
        let sortie = br##"{"id": "holaestony", "_type": "playlist", "entries": [
              {"id": "7707322854827707680", "url": "https://www.tiktok.com/@holaestony/video/7707322854827707680",
               "timestamp": 1794500946, "duration": 44, "title": "#espagnol ", "description": "#espagnol #apprendrelespagnol ",
               "view_count": 1674, "like_count": 7, "comment_count": 1, "repost_count": 0, "save_count": 2,
               "thumbnails": [{"id": "dynamicCover", "url": "https://x/anim"}, {"id": "cover", "url": "https://x/cover"}]},
              {"id": "7705857921501957409", "timestamp": 1794159866, "duration": 63.5, "title": "Quand tu dis bonjour #espagnol"}
            ]}"##;
        let l = ytdlp::lire_liste(sortie, Plateforme::Tiktok, "roleo", "holaestony", &at).unwrap();
        assert_eq!(l.videos.len(), 2);
        assert_eq!(l.videos[0], Video {
            id: "7707322854827707680".into(), business: "roleo".into(), compte: "holaestony".into(),
            legende: Some("#espagnol #apprendrelespagnol ".into()), publiee_le: Some(a(1794500946)), duree_s: Some(44.0),
            url: Some("https://www.tiktok.com/@holaestony/video/7707322854827707680".into()), plateforme: Plateforme::Tiktok,
        });
        assert_eq!(l.videos[1].legende.as_deref(), Some("Quand tu dis bonjour #espagnol"));
        assert_eq!(l.videos[1].url.as_deref(), Some("https://www.tiktok.com/@holaestony/video/7705857921501957409"));
        // compteurs publics : pas de watch time ; la seconde n'a pas de vues, pas de relevé
        assert_eq!(l.releves, vec![Releve {
            video: "7707322854827707680".into(), at, vues: 1674, likes: Some(7), commentaires: Some(1), partages: Some(0),
            enregistrements: Some(2), ..Default::default()
        }]);
        assert_eq!(l.images, HashMap::from([("7707322854827707680".to_string(), "https://x/cover".to_string())]));
        assert!(ytdlp::lire_liste(b"ERROR", Plateforme::Tiktok, "roleo", "x", &at).is_err());
    }

    #[test]
    fn page_de_profil() {
        let page = br#"<html><script id="__UNIVERSAL_DATA_FOR_REHYDRATION__" type="application/json">{"__DEFAULT_SCOPE__":
            {"webapp.user-detail": {"userInfo": {"user": {"avatarThumb": "https://x/a.jpg"},
             "stats": {"followerCount": 35, "followingCount": 2, "heart": 206, "heartCount": 206, "videoCount": 11},
             "statsV2": {"followerCount": "35"}}}}}</script></html>"#;
        let at = a(1794527444);
        let (r, avatar) = profil(page, "tiktok:tonyolingo", &at).unwrap();
        assert_eq!(r, crate::modeles::ReleveCompte {
            compte: "tiktok:tonyolingo".into(), at, abonnes: Some(35), videos: Some(11), likes: Some(206),
        });
        assert_eq!(avatar.as_deref(), Some("https://x/a.jpg"));
        assert!(profil(b"<html></html>", "x", &at).is_err());
    }

    #[test]
    fn insight() {
        let reponse = br#"{"status_code": 0, "status_msg": "", "extra": {"now": 1794527444000},
             "video_info": {"aweme_id": "7667975806622141728", "create_time": 1785339746, "desc": "Les 4 mots #espagnol",
               "statistics": {"collect_count": 83, "comment_count": 3, "digg_count": 228, "play_count": 31057, "share_count": 18},
               "video": {"duration": 43235}},
             "video_per_duration_realtime": {"is_after_7d": false, "value": {"status": 0, "value": 5.97}},
             "video_finish_rate_realtime": {"is_after_7d": false, "value": {"status": 0, "value": 0.0275}},
             "video_traffic_source_percent_realtime": null,
             "realtime_new_followers": {"is_after_7d": false, "value": {"status": 0, "value": 20}}}"#;
        let at = a(1794527444);
        assert_eq!(releve(reponse, "7667975806622141728", &at).unwrap(), Releve {
            video: "7667975806622141728".into(), at, vues: 31057, likes: Some(228), commentaires: Some(3), partages: Some(18),
            enregistrements: Some(83), watch_moyen_s: Some(5.97), completion: Some(0.0275), abonnes: Some(20),
        });

        // vidéo du jour : abonnés pas encore calculés, pas de temps réel
        let fraiche = br#"{"status_code": 0, "video_info": {"statistics": {"play_count": 1674, "digg_count": 172}},
             "realtime_new_followers": {"value": {"status": 2}}}"#;
        let r = releve(fraiche, "1", &at).unwrap();
        assert!(r.vues == 1674 && r.likes == Some(172) && r.abonnes.is_none() && r.watch_moyen_s.is_none() && r.completion.is_none());

        // vidéo d'un compte qui n'est pas celui de la session
        assert!(releve(br#"{"status_code": 5, "status_msg": "Invalid parameters"}"#, "1", &at).is_err());
    }

    #[test]
    fn analyse_tiktok() {
        let reponse = br#"{"status_code": 0,
             "video_total_duration_realtime": {"is_after_7d": false, "value": {"status": 0, "value": 198026}},
             "video_uv": {"status": 0, "value": 24000},
             "video_viewer_new_viewer_percent": {"status": 2},
             "video_traffic_source_percent_realtime": {"is_after_7d": false, "value": {"status": 0, "value": [
               {"key": "For You", "value": 0.95}, {"key": "Search", "value": 0.01}]}},
             "video_retention_rate_realtime": {"value": {"list": [
               {"timestamp": "2000", "value": 0.33}, {"timestamp": "0", "value": 1}, {"timestamp": "1000", "value": 0.57}]}},
             "video_viewer_location_percent_realtime": {"value": {"country_percent_list": [
               {"country_name": "FR", "country_vv_percent": 0.8}]}},
             "item_search_terms": null,
             "video_vv_history_7d": [{"status": 0, "value": 579}, {"status": 0, "value": 120}, {"status": 2}]}"#;
        let part = |nom: &str, part: f64| Part { nom: nom.into(), part };
        assert_eq!(analyse(reponse).unwrap(), Analyse {
            watch_total_s: Some(198026.0), spectateurs: Some(24000), retention: vec![1.0, 0.57, 0.33],
            sources: vec![part("For You", 0.95), part("Search", 0.01)], pays: vec![part("FR", 0.8)], vues_par_jour: vec![579, 120],
            ..Default::default()
        });
        // vidéo d'un autre compte : rien de privé
        assert!(analyse(br#"{"status_code": 0}"#).unwrap().est_vide());
    }

    #[test]
    fn historique_du_compte() {
        let reponse = br#"{"status_code": 0, "vv_history": [{"status": 0, "value": 24329}, {"status": 0, "value": 44092}, {"status": 2}]}"#;
        assert_eq!(historique(reponse).unwrap(), vec![Some(24329), Some(44092), None]);
        assert!(historique(br#"{"status_code": 8}"#).is_err());
    }

    #[test]
    fn vues_gagnees_du_jour() {
        let store = Store::memoire().unwrap();
        let iso = |s: &str| jour::depuis_iso(s).unwrap();
        // minuit à Paris le 13 nov. = 2026-11-12T23:00Z
        for (id, publiee) in [("a", "2026-11-01T10:00:00Z"), ("b", "2026-11-13T07:00:00Z"), ("c", "2026-10-01T10:00:00Z")] {
            store.video(&Video {
                id: id.into(), business: "roleo".into(), compte: "holaestony".into(), legende: None,
                publiee_le: Some(iso(publiee)), duree_s: None, url: None, plateforme: Plateforme::Tiktok,
            }).unwrap();
        }
        for (video, at, vues) in [("a", "2026-11-12T20:00:00Z", 1000), ("a", "2026-11-13T08:00:00Z", 1100),
                                  ("a", "2026-11-13T10:00:00Z", 1300), ("b", "2026-11-13T09:00:00Z", 500),
                                  ("c", "2026-11-13T08:00:00Z", 2000), ("c", "2026-11-13T10:00:00Z", 2050)] {
            store.releve(&Releve { video: video.into(), at: iso(at), vues, ..Default::default() }).unwrap();
        }
        // a : 1100 − 1000 de la veille ; b : sortie ce jour-là, depuis 0 ; c : depuis son premier relevé du jour
        assert_eq!(vues_gagnees(&store, "roleo", "2026-11-13", &iso("2026-11-13T09:30:00Z")).unwrap(), Some(600));
        assert_eq!(vues_gagnees(&store, "roleo", "2026-11-13", &iso("2026-11-13T11:00:00Z")).unwrap(), Some(850));
        assert_eq!(vues_gagnees(&store, "roleo", "2026-11-06", &iso("2026-11-06T11:00:00Z")).unwrap(), None);
    }

    #[test]
    fn paliers_et_titres() {
        assert_eq!(palier(900, 1200), Some(1_000));
        assert_eq!(palier(900, 12_000), Some(10_000));
        assert_eq!(palier(1200, 1500), None);
        assert_eq!(palier(999_999, 1_000_000), Some(1_000_000));
        assert_eq!(libelle(1_000_000), "1 million de");
        assert_eq!(libelle(10_000), "10\u{202F}000");
        assert_eq!(titre(Some("#espagnol #apprendrelespagnol "), None), None);
        assert_eq!(titre(Some("Quand tu dis bonjour\n#espagnol"), None).as_deref(), Some("Quand tu dis bonjour"));
        assert_eq!(titre(Some("Quand tu dis bonjour"), Some("Bonjour natif")).as_deref(), Some("Bonjour natif"));
    }
}
