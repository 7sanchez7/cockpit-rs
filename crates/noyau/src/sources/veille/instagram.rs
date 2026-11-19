//! Instagram sans session : DuckDuckGo (version lite) donne les reels d'un mot, l'embed de chaque reel donne ses
//! chiffres. Aucun cookie, aucun navigateur ; la recherche d'Instagram, elle, exige une session connectée.
use super::parseurs;
use crate::collecte::http::{Http, Requete, UA_SAFARI};
use crate::erreur::{format_err, Erreur, Res};
use crate::modeles::Plateforme;
use crate::modeles_veille::Trouvaille;

const RECHERCHE: &str = "https://lite.duckduckgo.com/lite/";

/// Les reels du dernier mois pour un mot, avec leurs chiffres : une page de résultats DuckDuckGo (une dizaine de
/// reels, triés par pertinence), chaque reel lu dans son embed.
pub async fn chercher(http: &Http, mot: &str) -> Res<Vec<Trouvaille>> {
    let requete = format!("site:instagram.com/reel {mot}");
    // `df=m` : le dernier mois, les vidéos plus vieilles n'ont rien à faire dans une veille
    let page = http.texte(&Requete::new(RECHERCHE, "POST").entete("User-Agent", UA_SAFARI)
        .entete("Accept-Language", "fr-FR,fr;q=0.9").formulaire(&[("q", requete.as_str()), ("df", "m")])).await?;
    let codes = lire_codes(&page);
    if codes.is_empty() {
        return Err(format_err("DuckDuckGo n'a donné aucun reel (limite atteinte ou mot sans résultat)"));
    }
    let mut trouvees = vec![];
    let mut derniere: Option<Erreur> = None;
    for code in &codes {
        match embed(http, code).await {
            Ok(Some(t)) => trouvees.push(t),
            Ok(None) => {}
            // Instagram freine : inutile d'insister sur les reels suivants
            Err(e @ Erreur::Http(429, _)) => {
                derniere = Some(e);
                break;
            }
            Err(e) => derniere = Some(e),
        }
    }
    if trouvees.is_empty() {
        return Err(derniere.unwrap_or_else(|| format_err(format!("aucun des {} reels n'est lisible dans son embed", codes.len()))));
    }
    Ok(trouvees)
}

/// Le reel `code` lu dans son embed ; `None` s'il est supprimé, privé, ou si la page change de forme.
async fn embed(http: &Http, code: &str) -> Res<Option<Trouvaille>> {
    let page = http.texte(&Requete::get(format!("https://www.instagram.com/reel/{code}/embed/captioned/"))
        .entete("User-Agent", UA_SAFARI)).await?;
    Ok(lire_embed(&page, code))
}

/// Les codes des reels et posts d'une page DuckDuckGo lite, dans l'ordre, une fois chacun. Les pubs passent par
/// `duckduckgo.com/y.js` : leur lien n'est jamais celui d'Instagram.
fn lire_codes(page: &str) -> Vec<String> {
    const DEBUT: &str = "href=\"https://www.instagram.com/";
    let mut codes: Vec<String> = vec![];
    for morceau in page.split(DEBUT).skip(1) {
        let chemin = morceau.split(['"', '?', '#']).next().unwrap_or("");
        let segments: Vec<&str> = chemin.split('/').collect();
        // `/reel/<code>/` ou `/<compte>/reel/<code>/`
        let code = match segments.as_slice() {
            [genre, code, ..] if est_genre(genre) => *code,
            [_, genre, code, ..] if est_genre(genre) => *code,
            _ => continue,
        };
        if code.len() >= 8 && code.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') && !codes.iter().any(|c| c == code) {
            codes.push(code.into());
        }
    }
    codes
}

fn est_genre(s: &str) -> bool { matches!(s, "reel" | "reels" | "p") }

/// Le reel `code` dans une page d'embed : le JSON de `"contextJSON":"…"` (un JSON dans une chaîne JSON).
fn lire_embed(page: &str, code: &str) -> Option<Trouvaille> {
    const CLE: &str = "\"contextJSON\":";
    let apres = &page[page.find(CLE)? + CLE.len()..];
    let chaine = &apres[..fin_de_chaine(apres)?];
    let json: String = serde_json::from_str(chaine).ok()?;
    parseurs::trouvailles(Plateforme::Instagram, &[json]).into_iter().find(|t| t.brut == code)
}

/// La fin (guillemet fermant compris) de la chaîne JSON qui commence au début de `s`.
fn fin_de_chaine(s: &str) -> Option<usize> {
    let mut octets = s.bytes().enumerate();
    if octets.next()?.1 != b'"' {
        return None;
    }
    while let Some((i, b)) = octets.next() {
        match b {
            b'\\' => {
                octets.next();
            }
            b'"' => return Some(i + 1),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une page lite réduite : deux pubs, un reel, le même reel sous le compte, un post, un profil, un reel au code trop court.
    const DDG: &str = r#"
        <a rel="nofollow" href="https://duckduckgo.com/y.js?ad_domain=maformation.fr&amp;u3=https%3A%2F%2Fwww.bing.com" class='result-link'>Pub</a>
        <a rel="nofollow" href="https://www.instagram.com/reel/Df4_lgTP6G-/" class='result-link'>Reel</a>
        <a rel="nofollow" href="https://www.instagram.com/appendreespaglo/reel/Df4_lgTP6G-/?hl=fr" class='result-link'>Même reel</a>
        <a rel="nofollow" href="https://www.instagram.com/reels/DervGPmx6VO/" class='result-link'>Reels</a>
        <a rel="nofollow" href="https://www.instagram.com/p/DfJd05ZMs6H/#x" class='result-link'>Post</a>
        <a rel="nofollow" href="https://www.instagram.com/appendreespaglo/" class='result-link'>Profil</a>
        <a rel="nofollow" href="https://www.instagram.com/appendreespaglo/reels/" class='result-link'>Onglet</a>
        <a rel="nofollow" href="https://www.instagram.com/reel/court/" class='result-link'>Court</a>
        <a rel="nofollow" href="https://www.instagram.com/explore/tags/espagnol/" class='result-link'>Tag</a>
        "#;

    /// Le reel réel de la fixture, réduit : id, compteurs en `edge_*`, pas de date.
    const REEL: &str = r#"{"context":{"type":"GraphVideo","shortcode":"Df6pm8UM-mC"},"gql_data":{"shortcode_media":{
        "id":"4033719412274227586","shortcode":"Df6pm8UM-mC","is_video":true,"video_view_count":22,"video_duration":64.505,
        "owner":{"username":"appendreespaglo","edge_followed_by":{"count":260}},"edge_liked_by":{"count":1},
        "edge_media_to_comment":{"count":0},"edge_media_to_caption":{"edges":[{"node":{"text":"Apprends \"l'espagnol\"\nvite"}}]}}}}"#;

    /// La page d'embed telle qu'Instagram la sert : le JSON du contexte, échappé dans une chaîne, au milieu de la page.
    fn page(contexte: &str) -> String {
        format!(r#"<html><script>window.__x={{"a":1,"contextJSON":{},"b":"c"}};</script></html>"#, serde_json::to_string(contexte).unwrap())
    }

    #[test]
    fn codes_des_reels_sans_les_pubs() {
        assert_eq!(lire_codes(DDG), ["Df4_lgTP6G-", "DervGPmx6VO", "DfJd05ZMs6H"]);
        assert!(lire_codes("<html>No results.</html>").is_empty());
    }

    #[test]
    fn reel_dans_l_embed() {
        let t = lire_embed(&page(REEL), "Df6pm8UM-mC").unwrap();
        assert_eq!((t.compte.as_str(), t.vues, t.likes, t.abonnes), ("appendreespaglo", Some(22), Some(1), Some(260)));
        assert_eq!(t.legende.as_deref(), Some("Apprends \"l'espagnol\"\nvite"));
        assert_eq!(t.publiee, crate::jour::Date::from_timestamp(1_795_076_847, 0));
        // un autre reel, une page sans contexte, un reel supprimé (`shortcode_media` nul), un JSON coupé
        assert!(lire_embed(&page(REEL), "Autre123456").is_none());
        assert!(lire_embed("<html>Connexion</html>", "Df6pm8UM-mC").is_none());
        assert!(lire_embed(&page(r#"{"gql_data":{"shortcode_media":null}}"#), "Df6pm8UM-mC").is_none());
        assert!(lire_embed(r#"{"contextJSON":"{\"a\":"#, "Df6pm8UM-mC").is_none());
    }

    #[test]
    fn fin_d_une_chaine_json() {
        assert_eq!(fin_de_chaine(r#""a\"b\\" reste"#), Some(8));
        assert_eq!(fin_de_chaine(r#""jamais fermée\"#), None);
        assert_eq!(fin_de_chaine("pas une chaîne"), None);
    }
}
