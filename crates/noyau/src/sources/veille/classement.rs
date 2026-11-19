//! Le classement de la bank par Claude Haiku : pour chaque vidéo, l'app ou le produit promu quand il est clairement
//! nommé, la langue dans laquelle elle parle à son public et son genre, d'après sa légende, son compte et son analyse.
//! Les vidéos pas encore classées, par lots de `LOT` ; une analyse nouvelle remet la vidéo à classer. Un business sans
//! clé Claude n'a pas de classement, sans erreur. Voir docs/sources/veille.md.
use super::analyse::{app_nommee, appeler, facturer, texte, HAIKU};
use super::demande::plafond;
use crate::collecte::Contexte;
use crate::erreur::{format_err, Erreur, Res};
use crate::jour::Date;
use crate::modeles_veille::{Classement, VideoVeille};
use crate::p;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// Les vidéos d'un appel.
pub const LOT: usize = 40;
/// Les vidéos classées par passage, au plus.
pub const PAR_PASSAGE: i64 = 2_000;
/// Le nom des appels du classement dans `veille_cout`.
pub const OBJET: &str = "classement";
const LEGENDE_MAX: usize = 300;
const DELAI_S: u64 = 120;
/// La langue quand rien ne la dit.
const INCONNUE: &str = "inconnue";

/// Classe les vidéos qui ne le sont pas encore ; rend les échecs.
pub async fn classer(ctx: &Contexte, maintenant: &Date) -> Vec<String> {
    let a_faire = match ctx.store.videos_veille("where v.classee is null order by v.trouvee desc limit ?", p![PAR_PASSAGE]) {
        Ok(v) => v,
        Err(e) => return vec![format!("classement : {e}")],
    };
    let mut echecs = vec![];
    for b in &ctx.businesses {
        let videos: Vec<&VideoVeille> = a_faire.iter().filter(|v| v.business == b.id).collect();
        if videos.is_empty() {
            continue;
        }
        let cle = match ctx.secrets.valeur(&b.config.claude, b).await {
            Ok(c) => c,
            Err(Erreur::NonConfigure(_)) => continue,
            Err(e) => {
                echecs.push(format!("classement {} : {e}", b.id));
                continue;
            }
        };
        for lot in videos.chunks(LOT) {
            if let Err(e) = classer_lot(ctx, &cle, lot, maintenant).await {
                echecs.push(format!("classement : {e}"));
                // le plafond atteint, ou Claude injoignable : les lots suivants attendent le prochain passage
                break;
            }
        }
    }
    echecs
}

async fn classer_lot(ctx: &Contexte, cle: &str, lot: &[&VideoVeille], at: &Date) -> Res<()> {
    plafond(&ctx.store)?;
    let o = appeler(&ctx.http, cle, &corps(lot), DELAI_S).await?;
    facturer(ctx, OBJET, &HAIKU, &o, at)?;
    let classes = lire(&o)?;
    ctx.store.lot(|| {
        // une vidéo oubliée par Claude reste à classer
        for (i, v) in lot.iter().enumerate() {
            if let Some(c) = classes.get(&(i + 1)) {
                ctx.store.classer_video(&v.id, c, at)?;
            }
        }
        Ok(())
    })?;
    ctx.store.a_change();
    Ok(())
}

pub fn consigne(lot: &[&VideoVeille]) -> String {
    let fiches: Vec<Value> = lot.iter().enumerate().map(|(i, v)| fiche(i + 1, v)).collect();
    format!("\
Classe ces vidéos courtes de TikTok et Instagram. Pour chacune, avec son `n` :
`app` : le nom de l'app ou du produit dont la vidéo fait la promo, seulement s'il est clairement nommé (dans la légende,
le hook, le texte à l'écran, ou le compte est celui de la marque) ; vide sinon, ne devine jamais. Un hashtag générique
(#apprendrelanglais) n'est pas une app.
`langue` : la langue dans laquelle la vidéo parle à son public (pas celle qu'elle enseigne), `{INCONNUE}` si rien ne le dit.
`genre` : cours (elle enseigne un point précis), astuces (une liste de conseils), sketch (une scène jouée, de l'humour),
storytime (une histoire vécue racontée), réaction (à une autre vidéo ou à un fait), témoignage (l'avis ou le résultat
de quelqu'un), trend (un son ou un format à la mode), pub (la publicité assumée d'un produit), autre.

{}", serde_json::to_string(&fiches).unwrap_or_default())
}

/// Une vidéo telle que Haiku la lit pour la classer : l'analyse en dit plus quand elle existe.
fn fiche(n: usize, v: &VideoVeille) -> Value {
    let a = v.analyse.as_ref();
    json!({
        "n": n,
        "compte": v.compte,
        "plateforme": v.plateforme.brut(),
        "legende": v.legende.as_deref().map(|l| l.chars().take(LEGENDE_MAX).collect::<String>()),
        "hook": a.map(|a| &a.hook),
        "texteEcran": a.map(|a| &a.texte_ecran),
        "placement": a.map(|a| &a.blocs.placement),
        "appVue": a.and_then(|a| a.app.as_deref()),
    })
}

pub fn schema() -> Value {
    let langues: Vec<&str> = VideoVeille::LANGUES.iter().map(|(c, _)| *c).chain([INCONNUE]).collect();
    json!({
        "type": "object",
        "properties": {"videos": {"type": "array", "items": {
            "type": "object",
            "properties": {
                "n": {"type": "integer"},
                "app": {"type": "string", "description": "L'app ou le produit promu, clairement nommé ; vide sinon"},
                "langue": {"type": "string", "enum": langues},
                "genre": {"type": "string", "enum": VideoVeille::GENRES},
            },
            "required": ["n", "app", "langue", "genre"],
            "additionalProperties": false,
        }}},
        "required": ["videos"],
        "additionalProperties": false,
    })
}

pub fn corps(lot: &[&VideoVeille]) -> Value {
    json!({
        "model": HAIKU.modele,
        "max_tokens": 4_000,
        "thinking": {"type": "disabled"},
        "output_config": {"effort": "low", "format": {"type": "json_schema", "schema": schema()}},
        "messages": [{"role": "user", "content": consigne(lot)}],
    })
}

/// Le classement de chaque `n` de la réponse ; une valeur hors des listes ne compte pas.
pub fn lire(o: &Map<String, Value>) -> Res<HashMap<usize, Classement>> {
    #[derive(Deserialize)]
    struct Ligne { n: usize, app: String, langue: String, genre: String }
    #[derive(Deserialize)]
    struct Reponse { videos: Vec<Ligne> }
    let r: Reponse = serde_json::from_str(&texte(o)?).map_err(|e| format_err(format!("classement illisible : {e}")))?;
    Ok(r.videos.into_iter().map(|l| (l.n, Classement {
        app: app_nommee(&l.app),
        langue: VideoVeille::LANGUES.iter().any(|(c, _)| *c == l.langue).then_some(l.langue),
        genre: VideoVeille::GENRES.contains(&l.genre.as_str()).then_some(l.genre),
    })).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modeles::Plateforme;
    use crate::modeles_veille::Trouvaille;
    use crate::store::Store;
    use chrono::{Duration, TimeZone, Utc};

    fn reponse(texte: Value) -> Map<String, Value> {
        serde_json::from_value(json!({"stop_reason": "end_turn", "content": [{"type": "text", "text": texte.to_string()}],
                                      "usage": {"input_tokens": 10, "output_tokens": 5}})).unwrap()
    }

    #[test]
    fn corps_et_lecture() {
        let s = Store::memoire().unwrap();
        let t0 = Utc.timestamp_opt(1_800_000_000, 0).unwrap();
        let mut t = Trouvaille::nouvelle(Plateforme::Tiktok, "1", "praktika.ai");
        t.legende = Some("x".repeat(500));
        s.trouvaille(&t, "roleo", Some("app"), &t0).unwrap();
        let v = s.videos_veille("", p![]).unwrap().remove(0);
        let c = corps(&[&v]);
        assert_eq!(c["model"], HAIKU.modele);
        assert_eq!(c["output_config"]["format"]["schema"]["properties"]["videos"]["items"]["properties"]["langue"]["enum"][7], INCONNUE);
        let texte = c["messages"][0]["content"].as_str().unwrap();
        assert!(texte.contains(r#""compte":"praktika.ai""#) && texte.contains(r#""n":1"#));
        assert!(!texte.contains(&"x".repeat(LEGENDE_MAX + 1)));

        let lu = lire(&reponse(json!({"videos": [
            {"n": 1, "app": " Praktika ", "langue": "fr", "genre": "pub"},
            {"n": 2, "app": "aucune", "langue": INCONNUE, "genre": "inventé"},
        ]}))).unwrap();
        assert_eq!(lu[&1], Classement { app: Some("Praktika".into()), langue: Some("fr".into()), genre: Some("pub".into()) });
        assert_eq!(lu[&2], Classement::default());
        assert!(lire(&reponse(json!({"autre": []}))).is_err());
    }

    #[test]
    fn classee_puis_reclassee_apres_analyse() {
        let s = Store::memoire().unwrap();
        let t0 = Utc.timestamp_opt(1_800_000_000, 0).unwrap();
        s.trouvaille(&Trouvaille::nouvelle(Plateforme::Tiktok, "1", "a"), "roleo", None, &t0).unwrap();
        let a_classer = |s: &Store| s.videos_veille("where v.classee is null", p![]).unwrap().len();
        assert_eq!(a_classer(&s), 1);
        let c = Classement { app: Some("Roleo".into()), langue: Some("es".into()), genre: Some("cours".into()) };
        s.classer_video("1", &c, &t0).unwrap();
        assert_eq!(a_classer(&s), 0);
        let v = s.videos_veille("", p![]).unwrap().remove(0);
        assert_eq!((v.app.as_deref(), v.langue.as_deref(), v.genre.as_deref()), (Some("Roleo"), Some("es"), Some("cours")));
        let a = crate::sources::veille::analyse::tests::analyse();
        s.analyse_veille("1", &(t0 + Duration::hours(1)), HAIKU.modele, &a).unwrap();
        assert_eq!(a_classer(&s), 1);
        // l'analyse a vu la vidéo : son app passe avant celle du classement
        let v = s.videos_veille("", p![]).unwrap().remove(0);
        assert_eq!(v.app_promue(), a.app.as_deref());
    }
}
