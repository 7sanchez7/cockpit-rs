//! Rangs top 200 de nos apps et de celles des potes, notes de nos apps. Voir docs/sources/classements-itunes.md.
use crate::business::Business;
use crate::collecte::{miniatures, Contexte, Http, Requete, Source};
use crate::erreur::{format_err, Erreur, Res};
use crate::jour::{self, Date};
use crate::metrique;
use crate::modeles::{Appli, EtatPassage, Evenement, Liste, TypeFil};
use crate::p;
use crate::store::Store;
use async_trait::async_trait;
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::Duration;

pub const ID: &str = "classements";
pub const TOUTES_CATEGORIES: i64 = 36;
pub const TAILLE: usize = 200;
/// Places gagnées ou perdues qui font une ligne du fil.
pub const SEUIL: i64 = 20;
const PAUSE: Duration = Duration::from_millis(500);

/// Storefront de chaque pays, pour l'en-tête `X-Apple-Store-Front`. Un pays absent coupe la source.
pub fn storefront(pays: &str) -> Option<i64> {
    Some(match pays {
        "fr" => 143442, "es" => 143454, "be" => 143446, "us" => 143441, "gb" => 143444,
        "de" => 143443, "it" => 143450, "ch" => 143459, "ca" => 143455,
        _ => return None,
    })
}

/// Les listes de viewTop par leur `id` (le `popId`), le même dans tous les pays. Les titres, eux, sont traduits.
fn liste_pop(pop: &str) -> Option<Liste> {
    match pop {
        "27" => Some(Liste::Gratuit),
        "30" => Some(Liste::Payant),
        "38" => Some(Liste::Recettes),
        _ => None,
    }
}

fn flux_rss(liste: Liste) -> &'static str {
    match liste {
        Liste::Gratuit => "topfreeapplications",
        Liste::Payant => "toppaidapplications",
        Liste::Recettes => "topgrossingapplications",
    }
}

/// Catégories App Store, noms de la page viewTop française.
fn genre(genre: i64) -> Option<&'static str> {
    Some(match genre {
        36 => "Toutes catégories", 6000 => "Économie et entreprise", 6001 => "Météo", 6002 => "Utilitaires",
        6003 => "Voyages", 6004 => "Sports", 6005 => "Réseaux sociaux", 6006 => "Références", 6007 => "Productivité",
        6008 => "Photo et vidéo", 6009 => "Actualités", 6010 => "Navigation", 6011 => "Musique", 6012 => "Style de vie",
        6013 => "Forme et santé", 6014 => "Jeux", 6015 => "Finance", 6016 => "Divertissement", 6017 => "Éducation",
        6018 => "Livres", 6020 => "Médecine", 6021 => "Journaux et magazines", 6023 => "Cuisine et boissons",
        6024 => "Shopping", 6026 => "Outils de développement", 6027 => "Graphisme et design",
        _ => return None,
    })
}

/// Les trois listes d'un pays et d'une catégorie, ids dans l'ordre du classement.
#[derive(Debug, Clone)]
pub struct Top {
    pub pays: String,
    pub genre: i64,
    pub listes: HashMap<Liste, Vec<String>>,
    /// Lu dans les flux RSS (top 100) faute de viewTop : pas de fil.
    pub rss: bool,
}

pub struct Classements;

#[async_trait]
impl Source for Classements {
    fn id(&self) -> &'static str { ID }
    fn nom(&self) -> &'static str { "Classements" }
    fn cadence(&self) -> i64 { 3 * 3600 }

    async fn collecte(&self, ctx: &Contexte) -> Res<usize> {
        let at = jour::maintenant();
        let Some(principal) = pays_principal(&ctx.businesses) else {
            return Err(Erreur::NonConfigure("aucun business n'a de pays".into()));
        };
        let mut pays: Vec<(String, i64)> = vec![];
        for code in ctx.businesses.iter().flat_map(|b| &b.config.pays) {
            if pays.iter().any(|(c, _)| c == code) {
                continue;
            }
            let Some(sf) = storefront(code) else {
                return Err(Erreur::NonConfigure(format!("pays {code} sans storefront connu, à ajouter à Classements.storefronts")));
            };
            pays.push((code.clone(), sf));
        }
        let nos: Vec<(&Business, String, String)> = ctx.businesses.iter()
            .filter_map(|b| b.config.app_store_id.clone()
                .map(|app| (b, app, b.config.pays.first().cloned().unwrap_or_else(|| principal.clone()))))
            .collect();
        let potes: Vec<Appli> = ctx.store.apps_suivies()?.into_iter().filter(|a| a.business.is_none()).collect();
        if nos.is_empty() && potes.is_empty() {
            return Err(Erreur::NonConfigure("aucune app à suivre".into()));
        }

        // Nos apps dans le premier pays de leur business, celles des potes dans le premier pays suivi.
        let mut a_lire: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (_, app, p) in &nos {
            a_lire.entry(p.clone()).or_default().push(app.clone());
        }
        if !potes.is_empty() {
            a_lire.entry(principal.clone()).or_default().extend(potes.iter().map(|a| a.id.clone()));
        }
        let mut applis: HashMap<String, Appli> = HashMap::new();
        for (code, ids) in &a_lire {
            for lot in ids.chunks(100) {
                for a in lookup(lot, code, &ctx.http).await? {
                    applis.insert(a.id.clone(), a);
                }
            }
        }
        let mut notres: Vec<(&Business, Appli)> = vec![];
        for (b, app, p) in &nos {
            let Some(a) = applis.get_mut(app) else {
                return Err(format_err(format!("{} : app {app} absente de l'App Store {}", b.nom, p.to_uppercase())));
            };
            a.business = Some(b.id.clone());
            notres.push((b, a.clone()));
        }

        let genres: BTreeSet<i64> = nos.iter().map(|(b, _, _)| b.config.genre)
            .chain(applis.values().filter_map(|a| a.genre))
            .chain(potes.iter().filter_map(|a| a.genre))
            .chain([TOUTES_CATEGORIES])
            .collect();
        let mut tops: Vec<Top> = vec![];
        let mut repli: Vec<String> = vec![];
        for (code, sf) in &pays {
            for &g in &genres {
                if !tops.is_empty() {
                    tokio::time::sleep(PAUSE).await;
                }
                match view_top(*sf, g, &ctx.http).await {
                    Ok(listes) => tops.push(Top { pays: code.clone(), genre: g, listes, rss: false }),
                    Err(Erreur::Format(_)) => {
                        tops.push(Top { pays: code.clone(), genre: g, listes: rss(code, g, &ctx.http).await?, rss: true });
                        repli.push(format!("{} {}", nom_genre(g), nom_pays(code)));
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        let liste_applis: Vec<Appli> = applis.values().cloned().collect();
        let icones_ko = miniatures::telecharger(&icones(&liste_applis), &ctx.http).await;
        let suivies: Vec<String> = notres.iter().map(|(_, a)| a.id.clone()).chain(potes.iter().map(|a| a.id.clone())).collect();
        let store = &ctx.store;
        let jour = jour::cle(&at);
        let lignes = store.lot(|| {
            let mut lignes = 0;
            for a in &liste_applis {
                store.app(a)?;
                lignes += 1;
            }
            for (b, a) in &notres {
                store.suivre(&a.id, true)?;
                if let Some(note) = a.note {
                    store.serie(&b.id, metrique::NOTE, &jour, -1, note)?;
                    lignes += 1;
                }
                store.serie(&b.id, metrique::NB_NOTES, &jour, -1, a.nb_notes.unwrap_or(0) as f64)?;
                lignes += 1;
                for e in fil(&a.id, &b.id, &tops, &at, store)? {
                    if store.fil(&e)? {
                        lignes += 1;
                    }
                }
            }
            for top in &tops {
                for (liste, ids) in &top.listes {
                    for app in &suivies {
                        let Some(i) = ids.iter().position(|x| x == app) else { continue };
                        store.rang(app, &top.pays, top.genre, *liste, &at, i as i64 + 1)?;
                        lignes += 1;
                    }
                }
            }
            Ok(lignes)
        })?;
        if !repli.is_empty() {
            return Err(format_err(format!("viewTop sans listes, repli RSS top 100 pour {}", repli.join(", "))));
        }
        if let Some(e) = icones_ko.first() {
            return Err(format_err(format!("{} icônes en échec, dont {e}", icones_ko.len())));
        }
        Ok(lignes)
    }
}

/// Les icônes des apps à mettre en cache (miniatures), pour que l'interface les montre sans réseau.
pub fn icones(applis: &[Appli]) -> HashMap<String, String> {
    let mut icones = HashMap::new();
    for a in applis {
        if let Some(url) = &a.icone {
            icones.entry(miniatures::icone(&a.id)).or_insert_with(|| url.clone());
        }
    }
    icones
}

/// Le fil d'une de nos apps : entrée dans une liste, sortie, ou au moins `SEUIL` places gagnées ou perdues
/// depuis le passage précédent. Rien au premier passage ni pour une liste lue en RSS.
pub fn fil(app: &str, business: &str, tops: &[Top], at: &Date, store: &Store) -> Res<Vec<Evenement>> {
    let Some(avant) = passage_precedent(store, at)? else { return Ok(vec![]) };
    let mut positions: HashMap<String, i64> = HashMap::new();
    for l in store.db.query("select pays, genre, liste, position from rang where app = ? and at = ?", p![app, &avant])? {
        let cle = format!("{}:{}:{}", l.texte("pays").unwrap_or_default(), l.entier("genre").unwrap_or(0),
                          l.texte("liste").unwrap_or_default());
        if let Some(position) = l.entier("position") {
            positions.insert(cle, position);
        }
    }
    let mut evenements = vec![];
    for top in tops.iter().filter(|t| !t.rss) {
        for liste in Liste::TOUTES {
            let Some(ids) = top.listes.get(&liste) else { continue };
            let cle = format!("{}:{}:{}", top.pays, top.genre, liste.brut());
            let lieu = format!("en {}, {} {}", liste.brut(), nom_genre(top.genre), nom_pays(&top.pays));
            let maintenant = ids.iter().position(|x| x == app).map(|i| i as i64 + 1);
            // une sortie porte le `at` où l'app a été vue la dernière fois : la même sortie ne revient pas
            let mut quand = jour::iso(at);
            let texte = match (positions.get(&cle), maintenant) {
                (None, Some(m)) => format!("{} {lieu} (entrée)", place(m)),
                (Some(&a), None) => {
                    quand = avant.clone();
                    format!("Hors top {TAILLE} {lieu} (était {})", place(a))
                }
                (Some(&a), Some(m)) if (a - m).abs() >= SEUIL => {
                    format!("{} {lieu} ({}{})", place(m), if a > m { "+" } else { "-" }, (a - m).abs())
                }
                _ => continue,
            };
            evenements.push(Evenement {
                id: format!("{ID}:{app}:{cle}:{quand}"), business: business.into(), at: *at, type_: TypeFil::Rang, texte,
                lien: Some(format!("https://apps.apple.com/{}/app/id{app}", top.pays)),
            });
        }
    }
    Ok(evenements)
}

/// Le `at` du passage précédent dans `rang`. `None` au premier passage, ou si le précédent a fini en repli RSS :
/// ses rangs s'arrêtent à 100, la comparaison inventerait des entrées.
fn passage_precedent(store: &Store, at: &Date) -> Res<Option<String>> {
    let lignes = store.db.query("select max(at) as at from rang where at < ?", p![at])?;
    let Some(avant) = lignes.first().and_then(|l| l.texte("at")) else { return Ok(None) };
    if let Some(dernier) = store.dernier_passage(ID)? {
        if dernier.etat == EtatPassage::Erreur && avant >= jour::iso(&dernier.debut) {
            return Ok(None);
        }
    }
    Ok(Some(avant))
}

// apps des potes

/// L'id App Store d'une saisie de l'écran Apps de potes : `6770304361` ou un lien `https://apps.apple.com/…/id6770304361`.
pub fn id_app(saisie: &str) -> Option<String> {
    let s = saisie.trim();
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) {
        return Some(s.into());
    }
    Regex::new(r"/id(\d+)").expect("regex d'id App Store").captures(s).map(|c| c[1].to_string())
}

/// Ajoute une app de pote : ses détails dans le premier pays suivi, puis suivie. Ses rangs arrivent au passage suivant.
pub async fn ajouter_app(id: &str, store: &Store, http: &Http) -> Res<Appli> {
    let Some(pays) = pays_principal(&store.businesses()?) else {
        return Err(Erreur::NonConfigure("aucun business n'a de pays".into()));
    };
    let Some(appli) = lookup(&[id.to_string()], &pays, http).await?.into_iter().next() else {
        return Err(format_err(format!("aucune app {id} sur l'App Store {}", pays.to_uppercase())));
    };
    store.lot(|| {
        store.app(&appli)?;
        store.suivre(id, true)
    })?;
    if let Some(e) = miniatures::telecharger(&icones(std::slice::from_ref(&appli)), http).await.first() {
        return Err(format_err(format!("{} suivie, sans son icône : {e}", appli.nom)));
    }
    Ok(appli)
}

/// Le premier pays du premier business qui en a : celui de la note et des apps des potes.
pub fn pays_principal(businesses: &[Business]) -> Option<String> {
    businesses.iter().flat_map(|b| &b.config.pays).next().cloned()
}

// iTunes

async fn view_top(storefront: i64, genre: i64, http: &Http) -> Res<HashMap<Liste, Vec<String>>> {
    let r = Requete::get(format!("https://itunes.apple.com/WebObjects/MZStore.woa/wa/viewTop?genreId={genre}&popId=27"))
        .entete("X-Apple-Store-Front", &format!("{storefront},32"));
    listes(&http.texte(&r).await?)
}

/// Les listes de la page viewTop : le JSON `its.serverData`, `pageData.topChartsPageData.topCharts`.
/// `selectedChart`, le doublon d'une des listes, est ignoré.
pub fn listes(html: &str) -> Res<HashMap<Liste, Vec<String>>> {
    let sans_listes = || format_err("viewTop sans pageData.topChartsPageData.topCharts");
    const DEBUT: &str = "its.serverData=";
    let debut = html.find(DEBUT).ok_or_else(sans_listes)? + DEBUT.len();
    let fin = html[debut..].find("</script>").ok_or_else(sans_listes)? + debut;
    let objet: Value = serde_json::from_str(&html[debut..fin]).map_err(|_| sans_listes())?;
    let charts = objet["pageData"]["topChartsPageData"]["topCharts"].as_array().ok_or_else(sans_listes)?;
    let mut listes = HashMap::new();
    for c in charts {
        let Some(liste) = c["id"].as_str().and_then(liste_pop) else { continue };
        let Some(ids) = c["adamIds"].as_array() else { continue };
        let Some(ids) = ids.iter().map(|v| v.as_str().map(String::from)).collect::<Option<Vec<_>>>() else { continue };
        listes.insert(liste, ids.into_iter().take(TAILLE).collect());
    }
    if listes.len() != Liste::TOUTES.len() {
        return Err(format_err(format!("viewTop : {} listes sur {}", listes.len(), Liste::TOUTES.len())));
    }
    Ok(listes)
}

async fn rss(pays: &str, genre: i64, http: &Http) -> Res<HashMap<Liste, Vec<String>>> {
    let mut listes = HashMap::new();
    for liste in Liste::TOUTES {
        tokio::time::sleep(PAUSE).await;
        let r = Requete::get(format!("https://itunes.apple.com/{pays}/rss/{}/limit=100/genre={genre}/json", flux_rss(liste)));
        listes.insert(liste, liste_rss(&http.donnees(&r).await?)?);
    }
    Ok(listes)
}

/// Les ids d'un flux RSS, dans l'ordre. Une seule entrée arrive en objet, pas en tableau.
pub fn liste_rss(data: &[u8]) -> Res<Vec<String>> {
    let objet: Value = serde_json::from_slice(data).map_err(|_| format_err("flux RSS sans feed"))?;
    let feed = objet.get("feed").filter(|f| f.is_object()).ok_or_else(|| format_err("flux RSS sans feed"))?;
    let entrees = match &feed["entry"] {
        Value::Array(a) => a.clone(),
        o @ Value::Object(_) => vec![o.clone()],
        _ => vec![],
    };
    Ok(entrees.iter().filter_map(|e| e["id"]["attributes"]["im:id"].as_str().map(String::from)).collect())
}

async fn lookup(ids: &[String], pays: &str, http: &Http) -> Res<Vec<Appli>> {
    let r = Requete::get(format!("https://itunes.apple.com/lookup?id={}&country={pays}", ids.join(",")));
    applis(&http.donnees(&r).await?)
}

/// Les apps d'une réponse `lookup`, sans `business`.
pub fn applis(data: &[u8]) -> Res<Vec<Appli>> {
    #[derive(Deserialize)]
    struct Lookup {
        results: Vec<Resultat>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Resultat {
        track_id: Option<i64>,
        track_name: Option<String>,
        artist_name: Option<String>,
        artwork_url100: Option<String>,
        artwork_url512: Option<String>,
        primary_genre_id: Option<i64>,
        average_user_rating: Option<f64>,
        user_rating_count: Option<i64>,
        version: Option<String>,
    }
    let reponse: Lookup = serde_json::from_slice(data).map_err(|e| format_err(format!("lookup : {e}")))?;
    Ok(reponse.results.into_iter().filter_map(|r| {
        Some(Appli {
            id: r.track_id?.to_string(), nom: r.track_name?, editeur: r.artist_name, icone: r.artwork_url512.or(r.artwork_url100),
            genre: r.primary_genre_id, note: r.average_user_rating, nb_notes: r.user_rating_count, version: r.version,
            business: None,
        })
    }).collect())
}

// textes

pub fn place(n: i64) -> String { if n == 1 { "1re".into() } else { format!("{n}e") } }

pub fn nom_genre(g: i64) -> String { genre(g).map(String::from).unwrap_or_else(|| format!("catégorie {g}")) }

/// Le nom français d'un pays suivi (ceux de `storefront`), le code en capitales sinon.
pub fn nom_pays(code: &str) -> String {
    match code {
        "fr" => "France", "es" => "Espagne", "be" => "Belgique", "us" => "États-Unis", "gb" => "Royaume-Uni",
        "de" => "Allemagne", "it" => "Italie", "ch" => "Suisse", "ca" => "Canada",
        _ => return code.to_uppercase(),
    }.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

    const ROLEO: &str = "6770304361";

    /// Une page viewTop réduite : les trois listes dans le désordre, gratuites à 210 ids, et le doublon `selectedChart`.
    fn page(gratuites: &[String]) -> String {
        let liste = |id: &str, titre: &str, ids: &[String]| {
            format!(r#"{{"adamIds":[{}],"shortTitle":"{titre}","id":"{id}","title":"Classement des apps {} (iPhone)"}}"#,
                    ids.iter().map(|i| format!("\"{i}\"")).collect::<Vec<_>>().join(","), titre.to_lowercase())
        };
        let charts = [liste("30", "Payantes", &["1".into(), "2".into()]), liste("27", "Gratuites", gratuites),
                      liste("38", "Rentables", &[ROLEO.into()])];
        format!(r#"<html><script>its.serverData={{"pageData":{{"topChartsPageData":{{"topCharts":[{}],"selectedChart":{{"adamIds":["999"],"id":"27"}}}}}}}}</script><script>autre</script></html>"#,
                charts.join(","))
    }

    #[test]
    fn listes_de_view_top() {
        let gratuites: Vec<String> = (1..=210).map(|i| i.to_string()).collect();
        let l = listes(&page(&gratuites)).unwrap();
        assert_eq!(l[&Liste::Payant], ["1", "2"]);
        assert_eq!(l[&Liste::Gratuit], gratuites[..200]);
        assert_eq!(l[&Liste::Recettes], [ROLEO]);
    }

    #[test]
    fn view_top_sans_listes() {
        assert!(listes("<html>vide</html>").is_err());
        let incomplete = page(&[]).replace(r#""id":"38""#, r#""id":"99""#);
        assert!(listes(&incomplete).is_err());
    }

    #[test]
    fn flux_rss_lu() {
        let plusieurs = r#"{"feed":{"entry":[{"id":{"attributes":{"im:id":"1"}}},{"id":{"attributes":{"im:id":"2"}}}]}}"#;
        let une = r#"{"feed":{"entry":{"id":{"attributes":{"im:id":"3"}}}}}"#;
        assert_eq!(liste_rss(plusieurs.as_bytes()).unwrap(), ["1", "2"]);
        assert_eq!(liste_rss(une.as_bytes()).unwrap(), ["3"]);
        assert!(liste_rss(br#"{"feed":{}}"#).unwrap().is_empty());
    }

    #[test]
    fn lookup_lu() {
        let json = r#"
        {"resultCount":2,"results":[
          {"trackId":6770304361,"trackName":"Roleo : Parler anglais IA","artistName":"Tony Sanchez",
           "artworkUrl100":"https://exemple/100x100bb.jpg","artworkUrl512":"https://exemple/512x512bb.jpg",
           "primaryGenreId":6017,"averageUserRating":4.69862,"userRatingCount":73,"version":"2.0.3"},
          {"wrapperType":"artist","artistName":"Sans app"}
        ]}
        "#;
        let a = applis(json.as_bytes()).unwrap();
        assert_eq!(a.len(), 1);
        let a = &a[0];
        assert_eq!(a.id, ROLEO);
        assert_eq!(a.nom, "Roleo : Parler anglais IA");
        assert_eq!(a.editeur.as_deref(), Some("Tony Sanchez"));
        assert_eq!(a.icone.as_deref(), Some("https://exemple/512x512bb.jpg"));
        assert_eq!(a.genre, Some(6017));
        assert_eq!(a.note, Some(4.69862));
        assert_eq!(a.nb_notes, Some(73));
        assert_eq!(a.version.as_deref(), Some("2.0.3"));
        assert_eq!(a.business, None);
        assert!(applis(b"pas du json").is_err());
    }

    /// Un classement où Roleo est à `position`, ou absent.
    fn top(pays: &str, genre: i64, gratuit: Option<usize>, recettes: Option<usize>, rss: bool) -> Top {
        let ids = |position: Option<usize>| {
            let mut ids: Vec<String> = (1..=200).map(|i| format!("autre{i}")).collect();
            if let Some(p) = position {
                ids[p - 1] = ROLEO.into();
            }
            ids
        };
        Top {
            pays: pays.into(), genre,
            listes: HashMap::from([(Liste::Gratuit, ids(gratuit)), (Liste::Payant, ids(None)), (Liste::Recettes, ids(recettes))]),
            rss,
        }
    }

    #[test]
    fn fil_des_rangs() {
        let store = Store::memoire().unwrap();
        let t0: Date = DateTime::<Utc>::from_timestamp(1_800_000_000, 0).unwrap();
        let t1 = t0 + chrono::Duration::hours(3);
        let tops = [top("fr", 6017, Some(152), None, false), top("fr", 36, Some(190), None, false),
                    top("es", 6017, Some(30), None, false)];

        assert!(fil(ROLEO, "roleo", &tops, &t1, &store).unwrap().is_empty());

        store.rang(ROLEO, "fr", 6017, Liste::Gratuit, &t0, 176).unwrap();
        store.rang(ROLEO, "fr", 6017, Liste::Recettes, &t0, 188).unwrap();
        store.rang(ROLEO, "es", 6017, Liste::Gratuit, &t0, 40).unwrap();
        let f = fil(ROLEO, "roleo", &tops, &t1, &store).unwrap();

        let mut textes: Vec<&str> = f.iter().map(|e| e.texte.as_str()).collect();
        textes.sort();
        assert_eq!(textes, [
            "152e en gratuit, Éducation France (+24)",
            "190e en gratuit, Toutes catégories France (entrée)",
            "Hors top 200 en recettes, Éducation France (était 188e)",
        ]);
        assert!(f.iter().all(|e| e.type_ == TypeFil::Rang && e.business == "roleo" && e.at == t1));
        let ids: BTreeSet<&str> = f.iter().map(|e| e.id.as_str()).collect();
        assert!(ids.contains(format!("classements:{ROLEO}:fr:6017:gratuit:{}", jour::iso(&t1)).as_str()));
        assert!(ids.contains(format!("classements:{ROLEO}:fr:6017:recettes:{}", jour::iso(&t0)).as_str()));
        assert_eq!(f[0].lien, Some(format!("https://apps.apple.com/fr/app/id{ROLEO}")));

        // une liste lue en RSS ne fait pas de fil
        assert!(fil(ROLEO, "roleo", &[top("fr", 6017, Some(100), None, true)], &t1, &store).unwrap().is_empty());
    }

    #[test]
    fn id_app_lu() {
        assert_eq!(id_app(ROLEO).as_deref(), Some(ROLEO));
        assert_eq!(id_app(&format!("  {ROLEO}\n")).as_deref(), Some(ROLEO));
        assert_eq!(id_app(&format!("https://apps.apple.com/fr/app/roleo-parler-anglais-ia/id{ROLEO}?l=en")).as_deref(), Some(ROLEO));
        assert_eq!(id_app("https://apps.apple.com/us/app/squid123/id456789").as_deref(), Some("456789"));
        assert_eq!(id_app("apps.apple.com/app/id42").as_deref(), Some("42"));
        assert_eq!(id_app("Roleo"), None);
        assert_eq!(id_app(""), None);
    }

    #[test]
    fn places() {
        assert_eq!(place(1), "1re");
        assert_eq!(place(152), "152e");
    }
}
