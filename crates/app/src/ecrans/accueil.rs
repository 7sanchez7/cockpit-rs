//! Les commandes de l'écran accueil : les réseaux à gauche, les chiffres de la période au centre, le plan de Notes à
//! droite. Tout arrive écrit ; l'interface n'ajoute que les couleurs.
use crate::commandes::{texte, Rep};
use crate::Etat;
use noyau::collecte::miniatures;
use noyau::jour::{self, Date};
use noyau::lecture::accueil::{self, VIDEOS_PAR_JOUR};
use noyau::lecture::fenetre::{Chiffres, Cumul, Fenetre, Periode};
use noyau::lecture::format::{self as fmt, VIDE};
use noyau::lecture::{Lectures, SEUIL_VIDEO};
use noyau::metrique::{self as m, en_euros};
use noyau::modeles::{Evenement, LignePlan, Ton};
use noyau::sources::classements::nom_genre;
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

/// Le ton d'un écart, comme `Ton` en Swift : vert s'il va dans le bon sens, rouge sinon, neutre sans écart.
pub(crate) fn ton(ecart: Option<f64>, inverse: bool) -> &'static str {
    match ecart {
        Some(e) if e != 0.0 => if (e > 0.0) != inverse { "positif" } else { "negatif" },
        _ => "neutre",
    }
}

/// Une carte KPI : `valeur` sert à faire rouler le chiffre, `texte` est ce qui s'affiche.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Carte {
    pub titre: String,
    pub valeur: Option<f64>,
    pub texte: String,
    pub delta: Option<String>,
    pub ton: &'static str,
    pub note: Option<String>,
    pub serie: Vec<Option<f64>>,
    pub inverse: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VueAccueil {
    sous_titre: String,
    boutique: Option<String>,
    reseau_ouvert: bool,
    plan_ouvert: bool,
    heros: Heros,
    kpis: Vec<Carte>,
    semaine: Semaine,
    score: VueScore,
    actifs: Actifs,
    fil: Vec<Evenement>,
    videos: PanneauVideos,
    reseau: Reseau,
    plan: Plan,
    taches: Vec<Tache>,
}

/// Le revenu brut de la période en héros, et sa courbe cumulée.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Heros {
    periode: Periode,
    periodes: Vec<Choix>,
    valeur: Option<f64>,
    texte: String,
    delta: Option<String>,
    ton: &'static str,
    detail: String,
    pied: String,
    points: Vec<PointRevenu>,
}

#[derive(Serialize)]
struct Choix {
    id: Periode,
    libelle: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PointRevenu {
    jour: String,
    euros: f64,
    texte: String,
}

/// Une série de barres contre la même d'avant, avec ses repères et ses infobulles.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Barres {
    titre: String,
    legende: [String; 2],
    valeurs: Vec<Option<f64>>,
    avant: Vec<Option<f64>>,
    courante: Option<usize>,
    axe: Vec<String>,
    aides: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MiniChiffre {
    libelle: &'static str,
    valeur: Option<f64>,
    texte: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Semaine {
    chiffres: Vec<MiniChiffre>,
    barres: Barres,
}

type Actifs = Barres;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AxeRadar {
    nom: String,
    note: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VueScore {
    axes: Vec<AxeRadar>,
    global: Option<i64>,
    faible: String,
    aide: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PanneauVideos {
    titre: &'static str,
    vide: &'static str,
    seuil: String,
    lignes: Vec<LigneVideo>,
}

/// Une ligne du tableau des vidéos, cellules écrites.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LigneVideo {
    id: String,
    titre: String,
    age: String,
    vues: String,
    gain: String,
    gagne: bool,
    partages: String,
    enreg: String,
    /// None sans compteur, sinon le seuil atteint ou non.
    seuil: Option<bool>,
    aide: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Reseau {
    /// « 4 / 9 vidéos » et la part de la jauge ; None sans compte actif.
    jauge: Option<Jauge>,
    comptes: Vec<CompteReseau>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Jauge {
    texte: String,
    part: f64,
    tenue: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CompteReseau {
    id: String,
    plateforme: &'static str,
    avatar: String,
    handle: String,
    actif: bool,
    detail: String,
    tenu: bool,
    abonnes: String,
    gain: Option<String>,
    ton_gain: &'static str,
    aide: String,
    aide_rail: String,
    profil: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Plan {
    date: String,
    vide: String,
    sections: Vec<SectionPlan>,
    restantes: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SectionPlan {
    nom: String,
    compte: Option<String>,
    lignes: Vec<LignePlan>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Tache {
    id: String,
    texte: String,
    faite: bool,
    alerte: bool,
    detail: String,
    pastille: Option<String>,
    pastille_ambre: bool,
    lien: Option<String>,
}

fn entier(v: Option<f64>) -> String { v.map(fmt::entier).unwrap_or_else(|| VIDE.into()) }
fn euros(v: Option<f64>) -> String { v.map(fmt::euros).unwrap_or_else(|| VIDE.into()) }

#[allow(clippy::too_many_arguments)]
fn carte(titre: impl Into<String>, c: &Chiffres, texte: String, delta: Option<String>, ton: &'static str,
         note: Option<String>, serie: Vec<Option<f64>>, inverse: bool) -> Carte {
    Carte { titre: titre.into(), valeur: c.valeur, texte, delta, ton, note, serie, inverse }
}

#[tauri::command]
pub async fn accueil(etat: State<'_, Etat>, business: Option<String>, periode: Periode) -> Rep<VueAccueil> {
    let l = crate::ecrans::lectures(&etat.store, business.as_deref())?;
    let f = Fenetre { periode, maintenant: l.maintenant };
    let aujourdhui = jour::cle(&l.maintenant);
    let ouvert = |cle: &str| -> Rep<bool> { Ok(etat.store.reglage(cle).map_err(texte)?.as_deref() != Some("0")) };
    let fil_au = if f.jour() { None } else { Some(f.au()) };
    let erreur_notes = l.derniers_passages().map_err(texte)?.remove("notes").and_then(|p| p.erreur);
    Ok(VueAccueil {
        sous_titre: f.sous_titre(),
        boutique: l.boutique.map(|b| b.brut().to_string()),
        reseau_ouvert: ouvert("accueil.reseau")?,
        plan_ouvert: ouvert("accueil.plan")?,
        heros: heros(&l, &f).map_err(texte)?,
        kpis: six(&l, &f).map_err(texte)?,
        semaine: semaine(&l, &aujourdhui).map_err(texte)?,
        score: score(&l, &aujourdhui).map_err(texte)?,
        actifs: actifs(&l, &f).map_err(texte)?,
        fil: l.fil(Some(&f.du()), fil_au.as_ref(), 40).map_err(texte)?,
        videos: videos(&l, &f).map_err(texte)?,
        reseau: reseau(&l).map_err(texte)?,
        plan: plan(l.plan().map_err(texte)?, erreur_notes, &l.maintenant),
        taches: taches(&l, business.is_none()).map_err(texte)?,
    })
}

/// Le revenu brut de la période en euros ; la courbe le cumule jour par jour, sur 30 jours pour aujourd'hui.
fn heros(l: &Lectures, f: &Fenetre) -> noyau::Res<Heros> {
    let revenu = f.mesure(l, m::REVENU, None, Cumul::Somme)?.map(en_euros);
    let (delta, ton) = if f.jour() {
        let hier = l.valeur(m::REVENU, &jour::decale(&f.fin(), -1), false)?;
        (hier.map(|v| format!("hier {}", fmt::euros(en_euros(v)))), "neutre")
    } else {
        (revenu.couverture().or_else(|| fmt::evolution(revenu.valeur, revenu.avant)), ton(fmt::ecart(revenu.valeur, revenu.avant), false))
    };
    let n = if f.jour() { Periode::Mois.n() } else { f.n() };
    let points = l.revenu_cumule(n, &f.fin())?.into_iter()
        .map(|p| PointRevenu { texte: fmt::euros(p.euros), jour: fmt::jour_court_cle(&p.jour), euros: p.euros })
        .collect();
    let essais = f.mesure(l, m::ESSAIS, Some(m::ESSAIS_J7), Cumul::Somme)?.valeur;
    let achats = f.mesure(l, m::ACHATS, None, Cumul::Somme)?.valeur;
    let detail: Vec<String> = [fmt::nombre(essais, "essai"), fmt::nombre(achats, "achat")].into_iter().flatten().collect();
    Ok(Heros {
        periode: f.periode,
        periodes: Periode::TOUTES.iter().map(|&p| Choix { id: p, libelle: p.texte() }).collect(),
        valeur: revenu.valeur,
        texte: euros(revenu.valeur),
        delta,
        ton,
        detail: if detail.is_empty() { VIDE.into() } else { detail.join(", ") },
        pied: format!("Brut cumulé sur {n} jours, TVA et commission Apple comprises"),
        points,
    })
}

/// Abonnés, essais, installs, actifs, vues TikTok, et le rang de l'app quand elle a un classement.
fn six(l: &Lectures, f: &Fenetre) -> noyau::Res<Vec<Carte>> {
    let mut cartes = Vec::new();
    let abonnes = f.mesure(l, m::ABONNES, None, Cumul::Dernier)?;
    let ecart = fmt::ecart(abonnes.valeur, abonnes.avant);
    let mrr = f.mesure(l, m::MRR_BRUT, None, Cumul::Dernier)?.valeur;
    cartes.push(carte("Abonnés", &abonnes, entier(abonnes.valeur), fmt::unites(ecart), ton(ecart, false),
                      Some(format!("MRR {}", euros(mrr.map(en_euros)))), abonnes.serie.clone(), false));

    let essais = f.mesure(l, m::ESSAIS, Some(m::ESSAIS_J7), Cumul::Somme)?;
    let ecart = fmt::ecart(essais.valeur, essais.avant);
    let en_cours = f.mesure(l, m::ESSAIS_ACTIFS, None, Cumul::Dernier)?.valeur;
    cartes.push(carte("Essais", &essais, entier(essais.valeur), essais.couverture().or_else(|| fmt::unites(ecart)),
                      ton(ecart, false), Some(format!("{} en cours", entier(en_cours))), essais.serie.clone(), false));

    let installs = f.mesure(l, m::INSTALLS, Some(m::INSTALLS_J7), Cumul::Somme)?;
    let conversion = installs.valeur.filter(|&i| i > 0.0).map(|i| fmt::pourcent(essais.valeur.unwrap_or(0.0) / i, 0));
    cartes.push(carte("Installs", &installs, entier(installs.valeur),
                      installs.couverture().or_else(|| fmt::evolution(installs.valeur, installs.avant)),
                      ton(fmt::ecart(installs.valeur, installs.avant), false),
                      Some(format!("{} en essai", conversion.unwrap_or_else(|| VIDE.into()))), installs.serie.clone(), false));

    let actifs = f.mesure(l, m::ACTIFS, Some(m::ACTIFS_J7), Cumul::Moyenne)?;
    let ronds = f.mesure(l, m::RONDS, Some(m::RONDS_J7), Cumul::Somme)?.valeur;
    cartes.push(carte(if f.jour() { "Actifs" } else { "Actifs / jour" }, &actifs, entier(actifs.valeur),
                      actifs.couverture().or_else(|| fmt::evolution(actifs.valeur, actifs.avant)),
                      ton(fmt::ecart(actifs.valeur, actifs.avant), false),
                      Some(fmt::nombre(ronds, "rond").unwrap_or_else(|| VIDE.into())), actifs.serie.clone(), false));

    let vues = f.mesure(l, m::VUES_TIKTOK, Some(m::VUES_TIKTOK_J7), Cumul::Somme)?;
    let publiees: f64 = l.publications(f.n(), &f.fin())?.iter().sum();
    let note = fmt::nombre(Some(publiees), "vidéo").unwrap_or_else(|| VIDE.into())
        + if publiees < 2.0 { " publiée" } else { " publiées" };
    cartes.push(carte("Vues TikTok", &vues, entier(vues.valeur),
                      vues.couverture().or_else(|| fmt::evolution(vues.valeur, vues.avant)),
                      ton(fmt::ecart(vues.valeur, vues.avant), false), Some(note), vues.serie.clone(), false));

    if let Some(r) = l.rang_principal()? {
        let gain = r.ligne.gain(&r.cle).map(|g| g as f64);
        let aujourdhui = jour::cle(&l.maintenant);
        let note = Cumul::Dernier.de(&l.jours(m::NOTE, 7, &aujourdhui, false)?);
        let avis = Cumul::Dernier.de(&l.jours(m::NB_NOTES, 7, &aujourdhui, false)?);
        let rang = r.ligne.rangs.get(&r.cle).copied();
        let texte = match rang {
            Some(p) => fmt::rang(p),
            None if r.ligne.lu.is_none() => VIDE.into(),
            None => "hors top".into(),
        };
        let note = note.map(|n| format!("note {}{}", fmt::decimal(n, 2),
                                        avis.map(|a| format!(", {} avis", fmt::entier(a))).unwrap_or_default()));
        cartes.push(Carte {
            titre: format!("Rang {} {}", nom_genre(r.cle.genre), r.cle.pays.to_uppercase()),
            valeur: rang.map(|p| p as f64),
            texte,
            delta: fmt::unites(gain),
            ton: ton(gain, false),
            note: Some(note.unwrap_or_else(|| VIDE.into())),
            serie: l.rang_jours(r.ligne.id(), &r.cle, if f.jour() { 14 } else { f.n() })?,
            inverse: true,
        });
    }
    Ok(cartes)
}

/// Le revenu brut jour par jour du lundi au dimanche contre la semaine d'avant, et les cumuls depuis lundi.
fn semaine(l: &Lectures, aujourdhui: &str) -> noyau::Res<Semaine> {
    let jours = accueil::semaine(aujourdhui);
    let courante = jours.iter().position(|j| j == aujourdhui).unwrap_or(jours.len().saturating_sub(1));
    let dimanche = jours.last().cloned().unwrap_or_else(|| aujourdhui.to_string());
    let revenu = |fin: &str| -> noyau::Res<Vec<Option<f64>>> {
        Ok(l.jours(m::REVENU, 7, fin, false)?.into_iter().map(|v| v.map(en_euros)).collect())
    };
    let depuis_lundi = |v: &[Option<f64>]| Cumul::Somme.de(&v[..(courante + 1).min(v.len())]);
    let a = revenu(&dimanche)?;
    let b = revenu(&jour::decale(&dimanche, -7))?;
    let essais = depuis_lundi(&l.jours(m::ESSAIS, 7, &dimanche, false)?);
    let achats = depuis_lundi(&l.jours(m::ACHATS, 7, &dimanche, false)?);
    let revenu_lundi = depuis_lundi(&a);
    let aides = jours.iter().enumerate()
        .map(|(i, j)| format!("{} : {}, semaine d'avant {}", fmt::jour_court_cle(j), euros(a[i]), euros(b[i])))
        .collect();
    Ok(Semaine {
        chiffres: vec![
            MiniChiffre { libelle: "Revenu", valeur: revenu_lundi, texte: euros(revenu_lundi) },
            MiniChiffre { libelle: "Essais", valeur: essais, texte: entier(essais) },
            MiniChiffre { libelle: "Achats", valeur: achats, texte: entier(achats) },
        ],
        barres: Barres {
            titre: "Cette semaine".into(),
            legende: ["Revenu".into(), "Semaine d'avant".into()],
            valeurs: a,
            avant: b,
            courante: Some(courante),
            axe: ["L", "M", "M", "J", "V", "S", "D"].map(String::from).to_vec(),
            aides,
        },
    })
}

/// Le score des sept jours complets jusqu'à hier contre les sept d'avant.
fn score(l: &Lectures, aujourdhui: &str) -> noyau::Res<VueScore> {
    let s = l.score(&jour::decale(aujourdhui, -1))?;
    let global = s.global();
    let faible = match s.faible() {
        Some(a) if a.note.is_some() => format!("Point faible : {}, {} % de la semaine d'avant",
                                               a.nom.to_lowercase(), a.note.unwrap_or_default()),
        _ if global.is_none() => "Pas encore deux semaines relevées.".into(),
        _ => "Tout tient ou progresse.".into(),
    };
    let aide = s.axes.iter()
        .map(|a| format!("{} : {}", a.nom, a.note.map(|n| n.to_string()).unwrap_or_else(|| VIDE.into())))
        .collect::<Vec<_>>().join(", ");
    Ok(VueScore { axes: s.axes.into_iter().map(|a| AxeRadar { nom: a.nom, note: a.note }).collect(), global, faible, aide })
}

/// Aujourd'hui heure par heure contre le même jour une semaine avant ; sinon jour par jour contre la fenêtre d'avant.
fn actifs(l: &Lectures, f: &Fenetre) -> noyau::Res<Actifs> {
    let fin = f.fin();
    if f.jour() {
        let semaine = fmt::jour_semaine(&l.maintenant);
        let a = l.heures(m::ACTIFS_HEURE, &fin, false)?;
        let b = l.heures(m::ACTIFS_HEURE, &jour::decale(&fin, -7), false)?;
        let aides = (0..a.len())
            .map(|h| format!("{h} h : {}, une semaine avant {}", entier(a[h]), entier(b.get(h).copied().flatten())))
            .collect();
        return Ok(Barres {
            titre: "Actifs par heure".into(),
            legende: ["Aujourd'hui".into(), format!("{} dernier", fmt::majuscule(&semaine))],
            valeurs: a,
            avant: b,
            courante: Some(jour::heure(&l.maintenant) as usize),
            axe: [0, 4, 8, 12, 16, 20, 23].map(|h| format!("{h} h")).to_vec(),
            aides,
        });
    }
    let n = f.n();
    let deux = l.jours(m::ACTIFS, 2 * n, &fin, false)?;
    let (b, a) = deux.split_at(deux.len() - n as usize);
    let jours = jour::derniers(n, &fin);
    let mut reperes: Vec<usize> = (0..(n - 1) as usize).step_by((n / 6).max(1) as usize).collect();
    reperes.push(n as usize - 1);
    let aides = jours.iter().enumerate()
        .map(|(i, j)| format!("{} : {}, {n} jours avant : {}", fmt::jour_court_cle(j), entier(a[i]), entier(b[i])))
        .collect();
    Ok(Barres {
        titre: "Actifs par jour".into(),
        legende: [format!("{n} derniers jours"), format!("{n} jours d'avant")],
        valeurs: a.to_vec(),
        avant: b.to_vec(),
        courante: None,
        axe: reperes.iter().map(|&i| fmt::jour_court_cle(&jours[i])).collect(),
        aides,
    })
}

/// Aujourd'hui, les vidéos qui ont pris des vues sur 24 h ; sur N jours, celles publiées dans la fenêtre.
fn videos(l: &Lectures, f: &Fenetre) -> noyau::Res<PanneauVideos> {
    let mut toutes = l.videos(1000)?;
    let (titre, vide) = if f.jour() {
        toutes.retain(|v| v.gain24h.unwrap_or(0) > 0);
        toutes.sort_by_key(|v| std::cmp::Reverse(v.gain24h.unwrap_or(0)));
        ("Vidéos qui prennent des vues", "Aucune vidéo n'a pris de vues sur 24 h.")
    } else {
        let (du, au) = (f.du(), f.au());
        toutes.retain(|v| v.publiee_le.is_some_and(|d| d >= du && d < au));
        ("Vidéos publiées", "Aucune vidéo publiée sur la période.")
    };
    let part = |p: Option<f64>| p.map(|p| fmt::pourcent(p, 1)).unwrap_or_else(|| VIDE.into());
    let lignes = toutes.into_iter().take(8).map(|v| LigneVideo {
        titre: if v.titre.is_empty() { v.id.clone() } else { v.titre.clone() },
        age: v.publiee_le.map(|d| fmt::age(&d, &l.maintenant)).unwrap_or_else(|| VIDE.into()),
        vues: fmt::entier(v.vues as f64),
        gain: v.gain24h.map(|g| fmt::delta(g as f64, true)).unwrap_or_else(|| VIDE.into()),
        gagne: v.gain24h.unwrap_or(0) > 0,
        partages: part(v.part_partages()),
        enreg: part(v.part_enreg()),
        seuil: (v.part_partages().is_some() || v.part_enreg().is_some()).then(|| v.seuil_atteint()),
        aide: format!("@{} · relevé {}", v.compte, fmt::il_ya(&v.releve_le, &l.maintenant)),
        id: v.id,
    }).collect();
    Ok(PanneauVideos { titre, vide, seuil: format!("seuil partages et enreg. {}", fmt::pourcent(SEUIL_VIDEO, 1)), lignes })
}

/// Chaque compte suivi, actifs d'abord, et les vidéos du jour contre trois par compte actif.
fn reseau(l: &Lectures) -> noyau::Res<Reseau> {
    let lignes = l.reseau()?;
    let (actifs, en_pause): (Vec<_>, Vec<_>) = lignes.into_iter().partition(|r| r.compte.actif);
    let faites: i64 = actifs.iter().map(|r| r.aujourdhui).sum();
    let visees = actifs.len() as i64 * VIDEOS_PAR_JOUR;
    let jauge = (visees > 0).then(|| Jauge {
        texte: format!("{faites} / {visees} vidéos"),
        part: (faites as f64 / visees as f64).clamp(0.0, 1.0),
        tenue: faites >= visees,
    });
    let comptes = actifs.into_iter().chain(en_pause).map(|r| {
        let c = &r.compte;
        let detail = if c.actif {
            format!("{} / {VIDEOS_PAR_JOUR}{}", r.aujourdhui,
                    r.derniere.map(|d| format!(" · {}", fmt::il_ya(&d, &l.maintenant))).unwrap_or_else(|| " aujourd'hui".into()))
        } else {
            "en pause".into()
        };
        let releve = c.releve_le.map(|d| format!("Relevé {}", fmt::il_ya(&d, &l.maintenant))).unwrap_or_else(|| "Pas encore relevé".into());
        CompteReseau {
            id: c.id.clone(),
            plateforme: c.plateforme.brut(),
            avatar: miniatures::avatar(&c.id),
            handle: c.handle.clone(),
            actif: c.actif,
            detail,
            tenu: r.aujourdhui >= VIDEOS_PAR_JOUR,
            abonnes: c.abonnes.map(|a| fmt::compact(a as f64)).unwrap_or_else(|| VIDE.into()),
            gain: c.gain7j.map(|g| fmt::delta(g as f64, true)),
            ton_gain: ton(c.gain7j.map(|g| g as f64), false),
            aide: releve + &r.vues24h.map(|v| format!(", {} vues sur 24 h", fmt::entier(v as f64))).unwrap_or_default(),
            aide_rail: format!("@{}, {} / {VIDEOS_PAR_JOUR} aujourd'hui", c.handle, r.aujourdhui),
            profil: c.plateforme.profil(&c.handle),
        }
    }).collect();
    Ok(Reseau { jauge, comptes })
}

/// Le plan de la note « Cockpit », regroupé par section dans l'ordre de la note.
fn plan(lignes: Vec<LignePlan>, erreur: Option<String>, maintenant: &Date) -> Plan {
    let restantes = lignes.iter().filter(|p| p.coche && !p.fait).count();
    let mut sections: Vec<SectionPlan> = Vec::new();
    for p in lignes {
        match sections.last_mut() {
            Some(s) if s.nom == p.section => s.lignes.push(p),
            _ => sections.push(SectionPlan { nom: p.section.clone(), compte: None, lignes: vec![p] }),
        }
    }
    for s in &mut sections {
        let cases = s.lignes.iter().filter(|p| p.coche).count();
        let faits = s.lignes.iter().filter(|p| p.coche && p.fait).count();
        s.compte = (cases > 0).then(|| format!("{faits} / {cases}"));
    }
    Plan {
        date: fmt::date_longue(maintenant),
        vide: erreur.unwrap_or_else(|| {
            "Crée une note « Cockpit » dans Notes : intertitres pour les sections, cases à cocher pour les tâches.".into()
        }),
        sections,
        restantes,
    }
}

/// Les tâches ouvertes ; le nom du business quand « Tous » en montre plusieurs.
pub(crate) fn taches(l: &Lectures, tous: bool) -> noyau::Res<Vec<Tache>> {
    let aujourdhui = jour::cle(&l.maintenant);
    let plusieurs = tous && l.businesses.len() > 1;
    Ok(l.taches()?.into_iter().map(|t| {
        let nom = t.business.as_ref().filter(|_| plusieurs)
            .and_then(|id| l.businesses.iter().find(|b| &b.id == id)).map(|b| b.nom.clone());
        let detail: Vec<String> = [nom, t.fait_le.map(|d| format!("fait {}", fmt::il_ya(&d, &l.maintenant)))]
            .into_iter().flatten().collect();
        let alerte = t.ton == Ton::Alerte;
        let pastille = t.fait_le.is_none().then(|| t.pastille(&aujourdhui)).flatten();
        Tache {
            pastille_ambre: alerte || pastille.as_deref() == Some("retard"),
            faite: t.fait_le.is_some(),
            alerte,
            detail: detail.join(", "),
            pastille,
            lien: t.lien,
            texte: t.texte,
            id: t.id,
        }
    }).collect())
}

/// Coche ou décoche une tâche « À traiter ».
#[tauri::command]
pub async fn cocher_tache(etat: State<'_, Etat>, id: String, fait: bool) -> Rep<()> {
    etat.store.cocher(&id, fait).map_err(texte)?;
    etat.store.a_change();
    Ok(())
}

/// Ouvre l'app Notes, où vit la note « Cockpit ».
#[tauri::command]
pub async fn ouvrir_notes(app: AppHandle) -> Rep<()> {
    if cfg!(target_os = "macos") {
        app.opener().open_path("/System/Applications/Notes.app", None::<&str>).map_err(texte)
    } else {
        Err("Notes n'existe que sur Mac.".into())
    }
}
