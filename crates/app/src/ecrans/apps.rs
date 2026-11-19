//! L'écran Apps de potes : nos apps et celles des potes dans le top 200 de l'App Store, pays par pays, la courbe de
//! l'app choisie sur 30 jours et tous ses classements. Suivre une app interroge l'App Store (seule action réseau).
use super::sav::Etiquette;
use crate::commandes::{texte, Rep};
use crate::Etat;
use noyau::jour;
use noyau::lecture::format as F;
use noyau::lecture::{CleRang, LigneApp};
use noyau::modeles::{Appli, Liste};
use noyau::sources::classements::{self, nom_genre, nom_pays, TOUTES_CATEGORIES};
use serde::Serialize;
use tauri::State;

/// Les jours de la courbe.
const JOURS: i64 = 30;
/// Un repère de date tous les 7 jours.
const PAS_AXE: usize = 7;
/// Les places ajoutées au-dessus et au-dessous de la courbe, pour qu'un rang stable ait des graduations.
const MARGE: f64 = 5.0;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Apps {
    sous_titre: String,
    listes: Vec<Choix>,
    /// Les pays des business affichés, dans l'ordre de leur config.
    pays: Vec<Choix>,
    /// Faux tant que Classements n'a rien lu.
    lu: bool,
    lignes: Vec<Ligne>,
    choisie: Option<String>,
    /// La courbe et le détail de l'app choisie, quand il y en a une, lue, et au moins un pays.
    courbe: Option<Courbe>,
    detail: Option<Detail>,
}

#[derive(Serialize)]
pub struct Choix {
    id: String,
    nom: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ligne {
    id: String,
    nom: String,
    a_nous: bool,
    /// « Tony Sanchez · Éducation · ★ 4,7 · 73 notes ».
    infos: String,
    /// Une cellule par pays.
    rangs: Vec<Cellule>,
}

/// Le rang et le gain depuis la veille, sinon « hors top » (ou rien si Classements n'a jamais lu).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cellule {
    texte: String,
    classee: bool,
    gain: Option<Etiquette>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Courbe {
    /// Le pays de la courbe.
    pays: String,
    /// « Roleo, gratuit, Éducation, France ».
    legende: String,
    /// Les jours classés, positions en fractions (0 à gauche, 0 en haut) ; un tronçon par suite de jours consécutifs.
    points: Vec<Point>,
    reperes: Vec<Repere>,
    axe: Vec<Repere>,
}

#[derive(Serialize)]
pub struct Point {
    x: f64,
    y: f64,
    rang: String,
    jour: String,
    troncon: usize,
}

/// Une graduation : son texte et sa position en fraction.
#[derive(Serialize)]
pub struct Repere {
    texte: String,
    position: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    genre: String,
    listes: Vec<String>,
    lignes: Vec<LigneDetail>,
    /// « Ne plus suivre » : une app de pote seulement.
    retirable: Option<String>,
}

#[derive(Serialize)]
pub struct LigneDetail {
    pays: String,
    rangs: Vec<Cellule>,
}

/// Ce que l'ajout d'une app affiche sous le champ ; `id` est l'app suivie, à choisir.
#[derive(Serialize)]
pub struct Ajout {
    texte: String,
    erreur: bool,
    id: Option<String>,
}

/// L'écran ; `liste` (gratuit, payant, recettes), `toutes` : toutes catégories plutôt que la sienne ; `app` et `pays`
/// choisissent la courbe.
#[tauri::command]
pub async fn apps(
    etat: State<'_, Etat>,
    business: Option<String>,
    liste: String,
    toutes: bool,
    app: Option<String>,
    pays: Option<String>,
) -> Rep<Apps> {
    let liste = Liste::depuis(&liste).ok_or_else(|| format!("liste inconnue : {liste}"))?;
    let l = crate::ecrans::lectures(&etat.store, business.as_deref())?;
    let tous = etat.store.businesses().map_err(texte)?;
    let lignes = l.apps().map_err(texte)?;
    let mut suivis: Vec<String> = vec![];
    for p in l.businesses.iter().flat_map(|b| &b.config.pays) {
        if !suivis.contains(p) {
            suivis.push(p.clone());
        }
    }

    // le classement d'une app dans un pays : sa catégorie (celle du business pour les nôtres) ou toutes
    let cle = |a: &Appli, pays: &str| {
        let sienne = tous.iter().find(|b| Some(&b.id) == a.business.as_ref()).map(|b| b.config.genre).or(a.genre);
        CleRang { pays: pays.into(), genre: if toutes { TOUTES_CATEGORIES } else { sienne.unwrap_or(TOUTES_CATEGORIES) }, liste }
    };

    let ligne = lignes.iter().find(|x| Some(x.id()) == app.as_deref()).or(lignes.first());
    let lu = lignes.first().and_then(|x| x.lu);
    let (courbe, detail) = match ligne {
        Some(ligne) if lu.is_some() && !suivis.is_empty() => {
            let p = pays.filter(|p| suivis.contains(p)).unwrap_or_else(|| suivis[0].clone());
            let c = cle(&ligne.app, &p);
            let valeurs = l.rang_jours(ligne.id(), &c, JOURS).map_err(texte)?;
            let jours = jour::derniers(JOURS, &jour::cle(&l.maintenant));
            let legende = format!("{}, {}, {}, {}", ligne.app.nom, liste.brut(), nom_genre(c.genre), nom_pays(&p));
            let detail = Detail {
                genre: nom_genre(c.genre),
                listes: Liste::TOUTES.iter().map(|l| nom_liste(*l)).collect(),
                lignes: suivis.iter().map(|p| LigneDetail {
                    pays: nom_pays(p),
                    rangs: Liste::TOUTES.iter().map(|&liste| cellule(ligne, &CleRang { pays: p.clone(), genre: c.genre, liste })).collect(),
                }).collect(),
                retirable: (!ligne.est_a_nous()).then(|| format!("Retire {} du suivi ; ses rangs passés restent en base.", ligne.app.nom)),
            };
            (Some(courbe(&valeurs, &jours, p, legende)), Some(detail))
        }
        _ => (None, None),
    };

    Ok(Apps {
        sous_titre: sous_titre(&lignes, suivis.len(), &l.maintenant),
        listes: Liste::TOUTES.iter().map(|l| Choix { id: l.brut().into(), nom: nom_liste(*l) }).collect(),
        pays: suivis.iter().map(|p| Choix { id: p.clone(), nom: nom_pays(p) }).collect(),
        lu: lu.is_some(),
        lignes: lignes.iter().map(|x| Ligne {
            id: x.id().into(),
            nom: x.app.nom.clone(),
            a_nous: x.est_a_nous(),
            infos: infos(&x.app),
            rangs: suivis.iter().map(|p| cellule(x, &cle(&x.app, p))).collect(),
        }).collect(),
        choisie: ligne.map(|x| x.id().to_string()),
        courbe,
        detail,
    })
}

/// Suit une app de pote d'après son id ou son lien App Store : ses détails sont cherchés, ses rangs arrivent au passage
/// suivant de Classements. Le résultat s'affiche sous le champ, l'erreur comprise.
#[tauri::command]
pub async fn apps_suivre(etat: State<'_, Etat>, saisie: String) -> Rep<Ajout> {
    let Some(id) = classements::id_app(&saisie) else {
        return Ok(Ajout {
            texte: "Ni un id ni un lien App Store : colle l'id (123456789) ou le lien apps.apple.com/…/id123456789.".into(),
            erreur: true,
            id: None,
        });
    };
    let http = etat.collecteur.contexte().map_err(texte)?.http;
    let r = classements::ajouter_app(&id, &etat.store, &http).await;
    etat.store.a_change();
    Ok(match r {
        Ok(a) => Ajout {
            texte: format!("{} suivie : ses rangs arrivent au prochain passage de Classements, ou tout de suite avec {}.", a.nom, if cfg!(target_os = "macos") { "⌘R" } else { "Ctrl+R" }),
            erreur: false,
            id: Some(a.id),
        },
        Err(e) => Ajout { texte: e.to_string(), erreur: true, id: None },
    })
}

/// Arrête de suivre une app de pote ; ses rangs passés restent en base.
#[tauri::command]
pub async fn apps_ne_plus_suivre(etat: State<'_, Etat>, id: String) -> Rep<()> {
    etat.store.suivre(&id, false).map_err(texte)?;
    etat.store.a_change();
    Ok(())
}

/// « 3 apps suivies dont 1 à nous, 4 pays, lu il y a 2 h ».
fn sous_titre(lignes: &[LigneApp], pays: usize, maintenant: &jour::Date) -> String {
    if lignes.is_empty() {
        return "Aucune app suivie".into();
    }
    let nous = lignes.iter().filter(|l| l.est_a_nous()).count();
    let apps = if lignes.len() == 1 { "1 app suivie".to_string() } else { format!("{} apps suivies", lignes.len()) };
    let lu = lignes[0].lu.map(|d| format!("lu {}", F::il_ya(&d, maintenant))).unwrap_or_else(|| "jamais lu".into());
    format!("{apps} dont {nous} à nous, {pays} pays, {lu}")
}

/// « Tony Sanchez · Éducation · ★ 4,7 · 73 notes ».
fn infos(a: &Appli) -> String {
    [
        a.editeur.clone(),
        a.genre.map(nom_genre),
        a.note.map(|n| format!("★ {}", F::decimal(n, 1))),
        a.nb_notes.map(|n| format!("{} notes", F::entier(n as f64))),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ")
}

/// `Gratuit`, `Payant`, `Recettes`.
fn nom_liste(l: Liste) -> String {
    let b = l.brut();
    b[..1].to_uppercase() + &b[1..]
}

/// Le rang et, dessous, les places gagnées depuis la veille : vert si l'app monte, rouge si elle descend.
fn cellule(ligne: &LigneApp, cle: &CleRang) -> Cellule {
    match ligne.rangs.get(cle) {
        Some(&p) => Cellule {
            texte: F::rang(p),
            classee: true,
            gain: ligne.gain(cle).map(|g| Etiquette {
                texte: if g == 0 { "= 0".into() } else { F::delta(g as f64, false) },
                couleur: match g.signum() { 1 => "succes", -1 => "danger", _ => "secondaire" },
            }),
        },
        None => Cellule { texte: if ligne.lu.is_none() { F::VIDE.into() } else { "hors top".into() }, classee: false, gain: None },
    }
}

/// La meilleure place du jour, 1er en haut, avec au moins `MARGE` places de part et d'autre ; un jour hors top coupe
/// la courbe. Sans aucun jour classé, pas de points (l'écran dit « hors top »).
fn courbe(valeurs: &[Option<f64>], jours: &[String], pays: String, legende: String) -> Courbe {
    let n = valeurs.len().max(2) - 1;
    let classes = valeurs.iter().flatten().copied();
    let (bas, haut) = classes.fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(v), b.max(v)));
    let (debut, fin) = if bas <= haut { ((bas - MARGE).max(1.0), haut + MARGE) } else { (1.0, 1.0 + 2.0 * MARGE) };
    let pas = pas_rond((fin - debut) / 4.0);
    let y = |v: f64| (v - debut) / (fin - debut);

    let mut points = vec![];
    let mut troncon = 0;
    for (i, (v, j)) in valeurs.iter().zip(jours).enumerate() {
        match v {
            Some(v) => points.push(Point { x: i as f64 / n as f64, y: y(*v), rang: F::rang(*v as i64), jour: F::jour_court_cle(j), troncon }),
            None => troncon += 1,
        }
    }
    let premier = (debut / pas).ceil() * pas;
    let reperes = (0..)
        .map(|k| premier + k as f64 * pas)
        .take_while(|&r| r <= fin)
        .filter(|&r| r >= 1.0)
        .map(|r| Repere { texte: F::rang(r as i64), position: y(r) })
        .collect();
    let axe = jours.iter().enumerate().step_by(PAS_AXE).map(|(i, j)| Repere { texte: F::jour_court_cle(j), position: i as f64 / n as f64 }).collect();
    Courbe { pays, legende, points, reperes, axe }
}

/// Un pas de graduation rond (1, 2, 2,5 ou 5 × 10ⁿ), une place entière, au moins une.
fn pas_rond(brut: f64) -> f64 {
    let p = 10f64.powf(brut.max(1.0).log10().floor());
    [1.0, 2.0, 2.5, 5.0, 10.0].iter().map(|m| m * p).filter(|x| x.fract() == 0.0).find(|&x| x >= brut).unwrap_or(10.0 * p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_courbe_coupee_par_un_jour_hors_top() {
        let jours: Vec<String> = (1..=4).map(|d| format!("2026-11-0{d}")).collect();
        let c = courbe(&[Some(12.0), None, Some(10.0), Some(14.0)], &jours, "fr".into(), String::new());
        assert_eq!(c.points.iter().map(|p| p.troncon).collect::<Vec<_>>(), vec![0, 1, 1]);
        // 5e en haut, 19e en bas : le 1er tout en haut n'entre pas
        assert_eq!(c.points[1].y, (10.0 - 5.0) / 14.0);
        assert_eq!(c.reperes.iter().map(|r| r.texte.as_str()).collect::<Vec<_>>(), vec!["5e", "10e", "15e"]);
        assert_eq!(c.axe.len(), 1);
    }

    #[test]
    fn un_pas_rond() {
        assert_eq!(pas_rond(0.4), 1.0);
        assert_eq!(pas_rond(3.3), 5.0);
        assert_eq!(pas_rond(14.0), 20.0);
        assert_eq!(pas_rond(21.75), 25.0);
    }
}
