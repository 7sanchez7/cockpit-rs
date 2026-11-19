//! Les commandes de l'écran data : toute la data de l'app sur la période (l'argent, l'acquisition, la conversion,
//! la rétention, l'usage), ou les installs d'un build comparés au build d'avant ; l'arbre des installs à part, pour
//! qu'un clic dans l'arbre ne relise que lui.
use super::accueil::{ton, Carte};
use crate::commandes::{texte, Rep};
use crate::Etat;
use noyau::dimension::Dimension::{self, *};
use noyau::jour;
use noyau::lecture::argent::{plus_grosse_perte, retour, serie, Jn};
use noyau::lecture::data::{Build, Filtre, Segment};
use noyau::lecture::decoupage::{self, ECRANS, RONDS_DEPUIS};
use noyau::lecture::fenetre::{Chiffres, Cumul, Fenetre, Periode};
use noyau::lecture::format::{self as fmt, VIDE};
use noyau::lecture::Lectures;
use noyau::metrique::{self as m, en_euros, EUR_PAR_USD};
use noyau::modeles::{Cohorte, Evenement, TypeFil};
use noyau::Res;
use serde::Serialize;
use tauri::State;

/// L'ordre des découpages : où ils s'arrêtent, ce qui se vend, d'où ils viennent, puis qui ils sont.
const DIMENSIONS: [Dimension; 16] = [
    Loin, Produit, Source, Age, Motivation, Blocage, Niveau, Langue, Maternelle, Zone, Pays, Interet, Notifs, Compte,
    Tuteur, Build,
];

const COLONNES_RETOUR: [(&str, Jn); 4] = [("J1", Jn::J1), ("J7", Jn::J7), ("J14", Jn::J14), ("J30", Jn::J30)];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VueData {
    sous_titre: String,
    boutique: Option<String>,
    periodes: Vec<Choix>,
    /// None quand un build remplace la période.
    periode: Option<Periode>,
    builds: Vec<ChoixBuild>,
    build: Option<String>,
    argent: Option<Argent>,
    acquisition: Option<Section>,
    conversion: Conversion,
    arbre: Arbre,
    retention: Retention,
    usage: Option<Section>,
}

#[derive(Serialize)]
struct Choix {
    id: Periode,
    libelle: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChoixBuild {
    numero: String,
    libelle: String,
}

/// Une section : son titre, ce qu'elle mesure, ses cartes.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Section {
    titre: &'static str,
    texte: String,
    kpis: Vec<Carte>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Argent {
    section: Section,
    objectif: Option<Objectif>,
    mrr: Courbe,
    achats: BarresAchats,
    ventes: Ventes,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Objectif {
    titre: String,
    part: f64,
    pourcent: String,
    mrr: String,
    reste: String,
    rythme: Option<String>,
}

/// Une série jour par jour, un libellé par jour.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Courbe {
    valeurs: Vec<Option<f64>>,
    etiquettes: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BarresAchats {
    valeurs: Vec<Option<f64>>,
    courante: Option<usize>,
    axe: Vec<String>,
    aides: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Ventes {
    compte: String,
    evenements: Vec<Evenement>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Conversion {
    section: Section,
    /// Seulement sur une période : les portes ne se découpent pas par build.
    portes: Option<Vec<Porte>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Porte {
    nom: String,
    vues: String,
    essais: String,
    directs: String,
    conversion: Taux,
}

/// Un taux : blanc et gras s'il est le meilleur de sa colonne, gris si sa base est trop petite.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Taux {
    texte: String,
    meilleur: bool,
    fiable: bool,
    aide: String,
}

impl Taux {
    fn de(t: Option<f64>, haut: Option<f64>, base: i64) -> Taux {
        let fiable = base >= Segment::MINIMUM;
        Taux {
            texte: t.map(|t| fmt::pourcent(t, 1)).unwrap_or_else(|| VIDE.into()),
            meilleur: fiable && t.is_some() && t == haut,
            fiable,
            aide: if fiable { String::new() } else { format!("Moins de {} installs : taux fragile", Segment::MINIMUM) },
        }
    }
}

/// Où l'arbre lit ses installs : la période, ou le build sous tous ses nœuds.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Arbre {
    texte: &'static str,
    debut: String,
    fin: String,
    racine: Vec<Filtre>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Retention {
    section: Section,
    sous_titre: String,
    cohortes: Vec<LigneCohorte>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LigneCohorte {
    jour: String,
    installs: String,
    /// Pas encore de J1 : pâle et en italique.
    ouverte: bool,
    cases: Vec<CaseCohorte>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CaseCohorte {
    texte: String,
    /// L'opacité de l'accent derrière la case : un plancher, plus la part du taux le plus haut de la colonne.
    opacite: f64,
}

fn entier(v: f64) -> String { fmt::entier(v) }
fn taux1(v: f64) -> String { fmt::pourcent(v, 1) }
fn delta_euros(a: Option<f64>, b: Option<f64>) -> Option<String> { fmt::ecart(a, b).map(|e| fmt::signe(e, fmt::euros)) }
fn delta_unites(a: Option<f64>, b: Option<f64>) -> Option<String> { fmt::unites(fmt::ecart(a, b)) }

/// Une carte comme `carte` en Swift : la couverture d'abord, sinon l'écart dit par `delta`.
fn carte(titre: &str, c: Chiffres, texte: fn(f64) -> String, delta: fn(Option<f64>, Option<f64>) -> Option<String>,
         inverse: bool) -> Carte {
    Carte {
        titre: titre.into(),
        texte: c.valeur.map(texte).unwrap_or_else(|| VIDE.into()),
        delta: c.couverture().or_else(|| delta(c.valeur, c.avant)),
        ton: ton(fmt::ecart(c.valeur, c.avant), inverse),
        valeur: c.valeur,
        note: None,
        serie: c.serie,
        inverse,
    }
}

#[tauri::command]
pub async fn data(etat: State<'_, Etat>, business: Option<String>, periode: Periode, build: Option<String>)
    -> Rep<VueData> {
    let l = crate::ecrans::lectures(&etat.store, business.as_deref())?;
    vue(&l, periode, build).map_err(texte)
}

fn vue(l: &Lectures, periode: Periode, build: Option<String>) -> Res<VueData> {
    let f = Fenetre { periode, maintenant: l.maintenant };
    let builds = l.builds()?;
    let i = build.and_then(|b| builds.iter().position(|x| x.numero == b));
    let choisi = i.map(|i| &builds[i]);
    // le build d'avant assez gros pour comparer
    let avant = i.and_then(|i| builds[i + 1..].iter().find(|b| b.installs >= Segment::MINIMUM));
    let base = VueData {
        sous_titre: choisi.map(|b| sous_titre(b, avant)).unwrap_or_else(|| f.sous_titre()),
        boutique: l.boutique.map(|b| b.brut().to_string()),
        periodes: Periode::TOUTES.iter().map(|&p| Choix { id: p, libelle: p.texte() }).collect(),
        periode: choisi.is_none().then_some(periode),
        builds: builds.iter().map(|b| ChoixBuild {
            numero: b.numero.clone(),
            libelle: format!("Build {} · {} · {} installs", b.numero, b.version, fmt::entier(b.installs as f64)),
        }).collect(),
        build: choisi.map(|b| b.numero.clone()),
        argent: None,
        acquisition: None,
        conversion: Conversion { section: Section { titre: "Conversion", texte: String::new(), kpis: vec![] }, portes: None },
        arbre: Arbre { texte: TEXTE_ARBRE, debut: f.debut(), fin: f.fin(), racine: vec![] },
        retention: Retention { section: Section { titre: "Rétention", texte: String::new(), kpis: vec![] },
                               sous_titre: sous_titre_cohortes(), cohortes: vec![] },
        usage: None,
    };
    Ok(match choisi {
        Some(b) => VueData {
            conversion: conversion_build(l, b, avant)?,
            arbre: Arbre { texte: TEXTE_ARBRE, debut: b.debut.clone(), fin: b.fin.clone(), racine: vec![b.filtre()] },
            retention: retention_build(l, b, avant)?,
            ..base
        },
        None => VueData {
            argent: Some(argent(l, &f)?),
            acquisition: Some(acquisition(l, &f)?),
            conversion: conversion(l, &f)?,
            retention: retention(l, &f)?,
            usage: Some(usage(l, &f)?),
            ..base
        },
    })
}

const TEXTE_ARBRE: &str = "Clique une marche, un écran du tunnel ou une valeur pour descendre ; le chemin du haut remonte. Chaque nœud dit combien de ses installs prennent l'essai et paient, et l'écart en points avec le nœud du dessus.";

/// « Build 49 (2.0.3), 919 installs du 8 au 14 nov., comparé au build 43 ».
fn sous_titre(b: &Build, avant: Option<&Build>) -> String {
    format!("Build {} ({}), {} installs du {} au {}, {}", b.numero, b.version, fmt::entier(b.installs as f64),
            fmt::jour_court_cle(&b.debut), fmt::jour_court_cle(&b.fin),
            avant.map(|a| format!("comparé au build {}", a.numero)).unwrap_or_else(|| "aucun build d'avant à comparer".into()))
}

/// « , comparés à ceux du build 43 », ou rien.
fn compare(avant: Option<&Build>) -> String {
    avant.map(|a| format!(", comparés à ceux du build {}", a.numero)).unwrap_or_default()
}

fn sous_titre_cohortes() -> String {
    format!("rond fini N jours après l'install, depuis le {}", fmt::jour_court_cle(RONDS_DEPUIS))
}

// argent

fn argent(l: &Lectures, f: &Fenetre) -> Res<Argent> {
    let n = if f.jour() { 14 } else { f.n() };
    let pas = (n / 5).max(1) as usize;
    let fin = f.fin();
    let aujourdhui = jour::cle(&l.maintenant);
    let kpis = vec![
        carte("MRR", f.mesure(l, m::MRR_BRUT, None, Cumul::Dernier)?.map(en_euros), fmt::euros, delta_euros, false),
        carte("Revenu", f.mesure(l, m::REVENU, None, Cumul::Somme)?.map(en_euros), fmt::euros, fmt::evolution, false),
        carte("Abonnés", f.mesure(l, m::ABONNES, None, Cumul::Dernier)?, entier, delta_unites, false),
        carte("Essais actifs", f.mesure(l, m::ESSAIS_ACTIFS, None, Cumul::Dernier)?, entier, delta_unites, false),
        carte("Achats", f.mesure(l, m::ACHATS, None, Cumul::Somme)?, entier, fmt::evolution, false),
        carte("Revenu 28 j", f.mesure(l, m::REVENU_28J, None, Cumul::Dernier)?.map(en_euros), fmt::euros, fmt::evolution, false),
        carte("Coût IA", f.mesure(l, m::COUT_IA, None, Cumul::Somme)?, fmt::euros, fmt::evolution, true),
        carte("Coût voix", f.mesure(l, m::COUT_VOIX, None, Cumul::Somme)?, fmt::euros, fmt::evolution, true),
    ];
    let jours = jour::derniers(n, &fin);
    let etiquettes: Vec<String> = jours.iter().map(|j| fmt::jour_court_cle(j)).collect();
    let achats = l.jours(m::ACHATS, n, &fin, false)?;
    let (du, au) = (f.du(), f.au());
    let ventes: Vec<Evenement> = l.fil(Some(&du), if f.jour() { None } else { Some(&au) }, 1000)?
        .into_iter().filter(|e| matches!(e.type_, TypeFil::Achat | TypeFil::Essai)).collect();
    Ok(Argent {
        section: Section {
            titre: "Argent",
            texte: format!("Le brut de RevenueCat, TVA et commission Apple comprises, en euros à {} € le dollar.",
                           fmt::decimal(EUR_PAR_USD, 2)),
            kpis,
        },
        objectif: l.objectif(&aujourdhui)?.map(|o| Objectif {
            titre: format!("Objectif {} par mois", fmt::euros(o.cible_eur)),
            part: o.part(),
            pourcent: fmt::pourcent(o.part(), 1),
            mrr: fmt::euros(o.mrr_eur),
            reste: if o.reste_eur() > 0.0 { format!("Reste {} par mois", fmt::euros(o.reste_eur())) } else { "Objectif atteint".into() },
            rythme: o.rythme_par_jour_eur.map(|r| {
                let texte = format!("Au rythme des 30 derniers jours, {} de MRR par mois", fmt::signe(r * 30.0, fmt::euros));
                match o.arrivee(&l.maintenant) {
                    Some(a) => format!("{texte} : objectif atteint vers {}.", fmt::mois_annee(&a)),
                    None => format!("{texte}."),
                }
            }),
        }),
        mrr: Courbe {
            valeurs: l.jours(m::MRR_BRUT, n, &fin, false)?.into_iter().map(|v| v.map(en_euros)).collect(),
            etiquettes: etiquettes.clone(),
        },
        achats: BarresAchats {
            aides: etiquettes.iter().zip(&achats)
                .map(|(j, v)| format!("{j} : {}", v.map(fmt::entier).unwrap_or_else(|| VIDE.into()))).collect(),
            courante: jours.iter().position(|j| *j == aujourdhui),
            axe: etiquettes.iter().step_by(pas).cloned().collect(),
            valeurs: achats,
        },
        ventes: Ventes { compte: format!("{} dans le fil", ventes.len()), evenements: ventes },
    })
}

fn acquisition(l: &Lectures, f: &Fenetre) -> Res<Section> {
    Ok(Section {
        titre: "Acquisition",
        texte: "Téléchargements : comptés par le store, app ouverte ou non, avec un à trois jours de retard. Installs : premier lancement vu par PostHog. Nouveaux clients : premier lancement vu par RevenueCat.".into(),
        kpis: vec![
            carte("Téléchargements", f.mesure(l, m::TELECHARGEMENTS, None, Cumul::Somme)?, entier, fmt::evolution, false),
            carte("Installs", f.mesure(l, m::INSTALLS, Some(m::INSTALLS_J7), Cumul::Somme)?, entier, fmt::evolution, false),
            carte("Nouveaux clients", f.mesure(l, m::NOUVEAUX_CLIENTS, None, Cumul::Somme)?, entier, fmt::evolution, false),
        ],
    })
}

// conversion

fn conversion(l: &Lectures, f: &Fenetre) -> Res<Conversion> {
    let premier = l.premier_install()?;
    let tous = premier.as_ref().map(|_| l.noeud(&[], &f.debut(), &f.fin())).transpose()?;
    // la fenêtre d'avant ne se compare que si les installs la couvrent entière
    let avant = match (f.avant(), &premier) {
        (Some((debut, fin)), Some(p)) if *p <= debut => Some(l.noeud(&[], &debut, &fin)?),
        _ => None,
    };
    let du = if f.jour() { jour::decale(&f.fin(), -13) } else { f.debut() };
    let haut = |portes: &[noyau::lecture::data::LignePorte]| {
        meilleur(&portes.iter().map(|p| if p.vues >= Segment::MINIMUM { p.conversion() } else { None }).collect::<Vec<_>>())
    };
    let portes = l.portes(&f.debut(), &f.fin())?;
    let h = haut(&portes);
    Ok(Conversion {
        section: Section {
            titre: "Conversion",
            texte: "Les installs de la période et ce qu'ils ont fait depuis. Payant : RevenueCat les a vus payer, après un essai ou sans.".into(),
            kpis: cartes_conversion(tous.as_ref(), avant.as_ref(), &l.par_jour(&[], &du, &f.fin())?),
        },
        portes: Some(portes.iter().map(|p| Porte {
            nom: p.nom(),
            vues: fmt::entier(p.vues as f64),
            essais: fmt::entier(p.essais as f64),
            directs: fmt::entier(p.directs as f64),
            conversion: Taux::de(p.conversion(), h, p.vues),
        }).collect()),
    })
}

fn conversion_build(l: &Lectures, b: &Build, avant: Option<&Build>) -> Res<Conversion> {
    let tous = l.noeud(&[b.filtre()], &b.debut, &b.fin)?;
    let precedent = avant.map(|a| l.noeud(&[a.filtre()], &a.debut, &a.fin)).transpose()?;
    Ok(Conversion {
        section: Section {
            titre: "Conversion",
            texte: format!("Les installs du build et ce qu'ils ont fait depuis{}. L'argent, l'acquisition, l'usage et les portes du paywall ne se découpent pas par build.", compare(avant)),
            kpis: cartes_conversion(Some(&tous), precedent.as_ref(), &l.par_jour(&[b.filtre()], &b.debut, &b.fin)?),
        },
        portes: None,
    })
}

/// Les installs et ce qu'ils ont fait, comparés à `avant`, jour d'install par jour en sparkline.
fn cartes_conversion(tous: Option<&Segment>, avant: Option<&Segment>, jours: &[Segment]) -> Vec<Carte> {
    let chiffres = |v: &dyn Fn(&Segment) -> Option<f64>| Chiffres {
        valeur: tous.and_then(v),
        avant: avant.and_then(v),
        serie: jours.iter().map(v).collect(),
        jours: None,
    };
    let compte = |n: fn(&Segment) -> i64| chiffres(&|s| Some(n(s) as f64));
    let paywall = compte(|s| s.paywall);
    vec![
        carte("Installs", compte(|s| s.installs), entier, fmt::evolution, false),
        Carte {
            titre: "Paywall vu".into(),
            texte: paywall.valeur.map(fmt::entier).unwrap_or_else(|| VIDE.into()),
            valeur: paywall.valeur,
            delta: tous.and_then(Segment::paywall_vu).map(|t| format!("{} des installs", fmt::pourcent(t, 0))),
            ton: "neutre",
            note: None,
            serie: paywall.serie,
            inverse: false,
        },
        carte("Essais gratuits", compte(|s| s.essais), entier, fmt::evolution, false),
        carte("Payants", compte(|s| s.payants), entier, fmt::evolution, false),
        carte("Install → essai", chiffres(&Segment::essai), taux1, fmt::points, false),
        carte("Install → payant", chiffres(&Segment::payant), taux1, fmt::points, false),
        carte("Essai → payant", chiffres(&Segment::essai_vers_payant), taux1, fmt::points, false),
        carte("Retour J1", chiffres(&Segment::retour_j1), taux1, fmt::points, false),
    ]
}

/// Le meilleur taux d'une colonne, s'il y a au moins deux lignes assez grandes pour comparer et qu'il dépasse zéro.
fn meilleur(taux: &[Option<f64>]) -> Option<f64> {
    let t: Vec<f64> = taux.iter().flatten().copied().collect();
    if t.len() < 2 { return None; }
    t.into_iter().reduce(f64::max).filter(|m| *m > 0.0)
}

// rétention

fn retention(l: &Lectures, f: &Fenetre) -> Res<Retention> {
    let taille = if f.jour() { 7 } else { f.n() };
    let cohortes = l.cohortes(&[], &jour::decale(&f.fin(), -(2 * taille + 32)), &f.fin())?;
    let kpis = COLONNES_RETOUR.iter().map(|&(titre, jn)| {
        let r = retour(&cohortes, jn, taille as usize);
        carte_retour(titre, jn, &cohortes, r.map(|r| (r.taux, r.cohortes)), r.and_then(|r| r.avant))
    }).collect();
    Ok(Retention {
        section: Section {
            titre: "Rétention",
            texte: format!("Installs qui finissent un rond N jours après : les {taille} dernières cohortes complètes, comparées aux {taille} d'avant. Mesuré depuis le {}, quand les ronds ont commencé à remonter.", fmt::jour_court_cle(RONDS_DEPUIS)),
            kpis,
        },
        sous_titre: sous_titre_cohortes(),
        cohortes: table_cohortes(&cohortes[..cohortes.len().min(f.n().max(35) as usize)]),
    })
}

fn retention_build(l: &Lectures, b: &Build, avant: Option<&Build>) -> Res<Retention> {
    let cohortes = l.cohortes(&[b.filtre()], &b.debut, &b.fin)?;
    let precedentes = match avant {
        Some(a) => l.cohortes(&[a.filtre()], &a.debut, &a.fin)?,
        None => vec![],
    };
    let kpis = COLONNES_RETOUR.iter().map(|&(titre, jn)| {
        let r = retour(&cohortes, jn, cohortes.len());
        carte_retour(titre, jn, &cohortes, r.map(|r| (r.taux, r.cohortes)),
                     retour(&precedentes, jn, precedentes.len()).map(|r| r.taux))
    }).collect();
    Ok(Retention {
        section: Section {
            titre: "Rétention",
            texte: format!("Installs du build qui finissent un rond N jours après, sur toutes ses cohortes complètes{}. Mesuré depuis le {}.", compare(avant), fmt::jour_court_cle(RONDS_DEPUIS)),
            kpis,
        },
        sous_titre: sous_titre_cohortes(),
        cohortes: table_cohortes(&cohortes),
    })
}

/// Taux pondéré, écart en points avec la référence, taux de chaque cohorte en sparkline.
fn carte_retour(titre: &str, jn: Jn, cohortes: &[Cohorte], r: Option<(f64, i64)>, avant: Option<f64>) -> Carte {
    let taux = r.map(|r| r.0);
    let nombre = r.map(|(_, n)| format!("{n} cohorte{}", if n > 1 { "s" } else { "" }));
    Carte {
        titre: format!("Retour {titre}"),
        valeur: taux,
        texte: taux.map(taux1).unwrap_or_else(|| VIDE.into()),
        delta: fmt::points(taux, avant).or(nombre),
        ton: ton(fmt::ecart(taux, avant), false),
        note: None,
        serie: serie(cohortes, jn).into_iter().map(Some).collect(),
        inverse: false,
    }
}

/// Une ligne par jour d'install, la plus récente en haut.
fn table_cohortes(cohortes: &[Cohorte]) -> Vec<LigneCohorte> {
    let plafonds = COLONNES_RETOUR.map(|(_, jn)| cohortes.iter().filter_map(|c| c.taux(jn)).fold(0.0, f64::max));
    cohortes.iter().map(|c| LigneCohorte {
        jour: fmt::jour_court_cle(&c.jour),
        installs: fmt::entier(c.installs as f64),
        ouverte: c.j1.is_none(),
        cases: COLONNES_RETOUR.iter().zip(plafonds).map(|(&(_, jn), plafond)| {
            let t = c.taux(jn);
            CaseCohorte {
                texte: t.map(taux1).unwrap_or_default(),
                opacite: t.map(|t| 0.06 + 0.4 * t / plafond.max(1e-9)).unwrap_or(0.0),
            }
        }).collect(),
    }).collect()
}

// usage

fn usage(l: &Lectures, f: &Fenetre) -> Res<Section> {
    let actifs = f.mesure(l, m::ACTIFS, Some(m::ACTIFS_J7), Cumul::Moyenne)?;
    let ronds_par_actif = ratio(&f.mesure(l, m::RONDS, Some(m::RONDS_J7), Cumul::Moyenne)?, &actifs);
    let ia_par_actif = ratio(&f.mesure(l, m::COUT_IA, None, Cumul::Moyenne)?, &actifs);
    Ok(Section {
        titre: "Usage et coûts",
        texte: "Les actifs du jour, ce qu'ils font et ce qu'ils coûtent en IA.".into(),
        kpis: vec![
            carte(if f.jour() { "Actifs" } else { "Actifs / jour" }, actifs, entier, fmt::evolution, false),
            carte("Ronds finis", f.mesure(l, m::RONDS, Some(m::RONDS_J7), Cumul::Somme)?, entier, fmt::evolution, false),
            carte("Ronds par actif", ronds_par_actif, |v| fmt::decimal(v, 2), fmt::evolution, false),
            // pour 100 : par actif, il s'arrondit à 0,00 €
            carte("Coût IA / 100 actifs", ia_par_actif, |v| fmt::euros(v * 100.0), fmt::evolution, true),
        ],
    })
}

/// `a` par `b`, jour par jour et sur la période ; une période couverte en partie ne compte que les jours relevés des deux.
fn ratio(a: &Chiffres, b: &Chiffres) -> Chiffres {
    let d = |x: Option<f64>, y: Option<f64>| match (x, y) {
        (Some(x), Some(y)) if y > 0.0 => Some(x / y),
        _ => None,
    };
    let serie: Vec<Option<f64>> = a.serie.iter().zip(&b.serie).map(|(x, y)| d(*x, *y)).collect();
    if a.jours.is_none() && b.jours.is_none() {
        return Chiffres { valeur: d(a.valeur, b.valeur), avant: d(a.avant, b.avant), serie, jours: None };
    }
    let paires: Vec<(f64, f64)> = a.serie.iter().zip(&b.serie).filter_map(|(x, y)| Some(((*x)?, (*y)?))).collect();
    Chiffres {
        valeur: d(Some(paires.iter().map(|p| p.0).sum()), Some(paires.iter().map(|p| p.1).sum())),
        avant: None,
        serie,
        jours: (!paires.is_empty()).then_some(paires.len() as i64),
    }
}

// arbre des installs

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VueArbre {
    miettes: Vec<Miette>,
    entonnoir: Entonnoir,
    tunnel: Tunnel,
    decoupe: Decoupe,
    /// L'ordre des découpages, pour descendre à la dimension suivante pas encore posée.
    dimensions: Vec<Dimension>,
}

/// Un pas du chemin : le nœud et ses installs ; le premier est la racine.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Miette {
    libelle: String,
    installs: String,
    aide: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Entonnoir {
    fragile: Option<String>,
    vide: Option<&'static str>,
    pas: Vec<Pas>,
    tuiles: Vec<Tuile>,
}

/// Une marche de l'entonnoir, ou le passage d'une marche à la suivante.
#[derive(Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
enum Pas {
    #[serde(rename_all = "camelCase")]
    Marche {
        nom: &'static str,
        nombre: String,
        couleur: &'static str,
        sous: Option<String>,
        part: f64,
        /// Le taux et son écart avec le nœud du dessus ; None pour les installs.
        taux: Option<String>,
        ecart: Option<Ecart>,
        filtre: Option<Filtre>,
        posee: bool,
        aide: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Passage { texte: String, perte: Option<String> },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Ecart {
    texte: String,
    couleur: &'static str,
    aide: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Tuile {
    libelle: &'static str,
    valeur: String,
    aide: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Tunnel {
    etapes: Vec<Etape>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Etape {
    nom: String,
    nombre: String,
    part: f64,
    pourcent: String,
    perte: String,
    pire: bool,
    choisie: bool,
    /// None pour les installs, qui ne se cliquent pas.
    aide: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Decoupe {
    titre: String,
    texte: String,
    nom: &'static str,
    onglets: Vec<Onglet>,
    colonnes: Vec<EnTete>,
    lignes: Vec<LigneDecoupe>,
    total: Option<Vec<Cellule>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Onglet {
    id: Dimension,
    nom: &'static str,
    actif: bool,
    posees: Option<String>,
    aide: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EnTete {
    titre: &'static str,
    largeur: &'static str,
    actif: bool,
    aide: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LigneDecoupe {
    valeur: String,
    libelle: String,
    pas_mesure: bool,
    posee: bool,
    part: f64,
    pourcent: String,
    cellules: Vec<Cellule>,
    aide: String,
    aide_ecarter: String,
}

/// Le compte en gris puis le taux ; seul le compte quand la colonne n'a pas de taux.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Cellule {
    n: Option<String>,
    taux: Option<Taux>,
    aide: String,
}

/// Une colonne de la découpe : un compte, son taux s'il en a un, la base qui dit si le taux est fiable.
struct Colonne {
    titre: &'static str,
    aide: &'static str,
    largeur: &'static str,
    n: Option<fn(&Segment) -> i64>,
    taux: Option<fn(&Segment) -> Option<f64>>,
    base: fn(&Segment) -> i64,
}

impl Colonne {
    /// Ce qui trie la colonne : le taux, sinon le compte.
    fn cle(&self, s: &Segment) -> Option<f64> {
        match self.taux {
            Some(t) => t(s),
            None => self.n.map(|n| n(s) as f64),
        }
    }
}

const COLONNES: [Colonne; 8] = [
    Colonne { titre: "Installs", aide: "Installs de la valeur sur la période", largeur: "installs",
              n: Some(|s| s.installs), taux: None, base: |s| s.installs },
    Colonne { titre: "Paywall", aide: "Ont vu le paywall, sur les installs", largeur: "taux",
              n: Some(|s| s.paywall), taux: Some(Segment::paywall_vu), base: |s| s.installs },
    Colonne { titre: "Essai", aide: "Ont pris l'essai gratuit, sur les installs", largeur: "taux",
              n: Some(|s| s.essais), taux: Some(Segment::essai), base: |s| s.installs },
    Colonne { titre: "Payant", aide: "Payants vus par RevenueCat, essais passés à payant et achats sans essai, sur les installs",
              largeur: "taux", n: Some(|s| s.payants), taux: Some(Segment::payant), base: |s| s.installs },
    Colonne { titre: "Essai payé", aide: "Essais finis qui ont payé, sur les essais finis", largeur: "taux",
              n: Some(|s| s.essais_payes), taux: Some(Segment::essai_vers_payant), base: Segment::essais_finis },
    Colonne { titre: "Rond", aide: "Ont fini au moins un rond, sur les installs depuis la mesure des ronds", largeur: "court",
              n: None, taux: Some(Segment::rond), base: |s| s.base_rond },
    Colonne { titre: "J1", aide: "Ont fini un rond le lendemain, sur les installs assez anciens depuis la mesure des ronds",
              largeur: "court", n: None, taux: Some(Segment::retour_j1), base: |s| s.base_j1 },
    Colonne { titre: "J7", aide: "Ont fini un rond 7 jours après, sur les installs assez anciens depuis la mesure des ronds",
              largeur: "court", n: None, taux: Some(Segment::retour_j7), base: |s| s.base_j7 },
];

/// L'arbre des installs de `debut` à `fin` qui passent `racine` (le build choisi), ouvert au bout de `chemin`,
/// découpé par `decoupe` et trié par la colonne `tri`.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn arbre(etat: State<'_, Etat>, business: Option<String>, debut: String, fin: String, racine: Vec<Filtre>,
                   chemin: Vec<Filtre>, decoupe: Dimension, tri: Option<String>) -> Rep<VueArbre> {
    let l = crate::ecrans::lectures(&etat.store, business.as_deref())?;
    vue_arbre(&l, &debut, &fin, &racine, &chemin, decoupe, tri.as_deref()).map_err(texte)
}

fn vue_arbre(l: &Lectures, debut: &str, fin: &str, racine: &[Filtre], chemin: &[Filtre], decoupe: Dimension,
             tri: Option<&str>) -> Res<VueArbre> {
    let tous: Vec<Filtre> = racine.iter().chain(chemin).cloned().collect();
    let noeuds = (0..=chemin.len()).map(|k| l.noeud(&tous[..racine.len() + k], debut, fin)).collect::<Res<Vec<_>>>()?;
    let noeud = &noeuds[chemin.len()];
    let parent = chemin.len().checked_sub(1).map(|k| &noeuds[k]);
    let racine_libelle = racine.first().map(Filtre::libelle).unwrap_or_else(|| "Tous les installs".into());
    let miettes = noeuds.iter().enumerate().map(|(k, n)| {
        let installs = fmt::entier(n.installs as f64);
        Miette {
            libelle: if k == 0 { racine_libelle.clone() } else { chemin[k - 1].libelle() },
            aide: if k == chemin.len() { format!("Le nœud ouvert, {installs} installs") } else { "Remonter ici".into() },
            installs,
        }
    }).collect();
    let mut etapes = vec![("Installs", noeud.installs)];
    etapes.extend(ECRANS.iter().map(|e| e.1).zip(l.tunnel(&tous, debut, fin)?));
    Ok(VueArbre {
        miettes,
        entonnoir: entonnoir(noeud, &noeuds[0], parent, chemin),
        tunnel: tunnel(&etapes, chemin),
        decoupe: table(decoupe, &l.enfants(&tous, decoupe, debut, fin)?, noeud, chemin, tri),
        dimensions: DIMENSIONS.to_vec(),
    })
}

fn pc(t: Option<f64>) -> String { t.map(taux1).unwrap_or_else(|| VIDE.into()) }

/// Le nœud ouvert en entonnoir : installs, paywall vu, essai, payant, chacun rapporté aux installs du nœud et comparé
/// au nœud du dessus.
fn entonnoir(n: &Segment, racine: &Segment, parent: Option<&Segment>, chemin: &[Filtre]) -> Entonnoir {
    let fragile = (n.installs > 0 && n.installs < Segment::MINIMUM).then(|| {
        format!("Moins de {} installs : un install de plus bouge les taux de plusieurs points", Segment::MINIMUM)
    });
    if n.installs == 0 {
        return Entonnoir { fragile, vide: Some("Aucun install ne passe ces filtres sur la période."), pas: vec![], tuiles: vec![] };
    }
    let part = |nombre: i64| nombre as f64 / n.installs.max(1) as f64;
    let marche = |nom: &'static str, nombre: i64, couleur: &'static str, filtre: Filtre, taux: fn(&Segment) -> Option<f64>,
                  sous: Option<String>| {
        let posee = chemin.contains(&filtre);
        let ecart = parent.and_then(|p| {
            let texte = fmt::points(taux(n), taux(p))?;
            let e = fmt::ecart(taux(n), taux(p)).unwrap_or(0.0);
            let fiable = n.installs >= Segment::MINIMUM;
            Some(Ecart {
                texte,
                couleur: if !fiable || e.abs() < 0.0005 { "secondaire" } else if e > 0.0 { "succes" } else { "danger" },
                aide: format!("Le nœud du dessus : {}", pc(taux(p))),
            })
        });
        Pas::Marche {
            nom, nombre: fmt::entier(nombre as f64), couleur, sous, part: part(nombre),
            taux: Some(taux1(part(nombre))), ecart, filtre: Some(filtre), posee,
            aide: Some(if posee { "Retirer ce filtre".into() } else { format!("Garder ceux qui sont passés par « {nom} »") }),
        }
    };
    let passage = |t: Option<f64>, texte: &str, perte: i64| Pas::Passage {
        texte: format!("{} {texte}", pc(t)),
        perte: (perte > 0).then(|| fmt::delta(-perte as f64, false)),
    };
    let pas = vec![
        Pas::Marche {
            nom: "Installs", nombre: fmt::entier(n.installs as f64), couleur: "secondaire",
            sous: parent.map(|_| format!("{} de tous les installs", taux1(n.installs as f64 / racine.installs.max(1) as f64))),
            part: 1.0, taux: None, ecart: None, filtre: None, posee: false, aide: None,
        },
        passage(n.paywall_vu(), "voient le paywall", n.installs - n.paywall),
        marche("Paywall vu", n.paywall, "bleu", Filtre::Paywall, Segment::paywall_vu, None),
        passage(Segment::taux(n.essais, n.paywall), "prennent l'essai", n.paywall - n.essais),
        marche("Essai gratuit", n.essais, "violet", Filtre::Essai, Segment::essai,
               (n.en_cours > 0).then(|| format!("dont {} en cours", fmt::entier(n.en_cours as f64)))),
        passage(n.essai_vers_payant(), "des essais finis paient", n.essais_finis() - n.essais_payes),
        marche("Payant", n.payants, "succes", Filtre::Payant, Segment::payant,
               (n.payants > n.essais_payes).then(|| format!("dont {} sans essai", fmt::entier((n.payants - n.essais_payes) as f64)))),
    ];
    let e = |v: i64| fmt::entier(v as f64);
    let depuis = fmt::jour_court_cle(RONDS_DEPUIS);
    let tuiles = vec![
        Tuile { libelle: "Achats sans essai", valeur: e(n.directs),
                aide: "Achats lancés depuis le paywall sans essai, cartes refusées comprises : Payant dit lesquels ont été encaissés".into() },
        Tuile { libelle: "Essais en cours", valeur: e(n.en_cours), aide: "Essais que RevenueCat voit encore courir".into() },
        Tuile { libelle: "À vérifier", valeur: e(n.a_verifier),
                aide: "Convertis que RevenueCat n'a pas encore lus, ou qu'il ne connaît pas en production".into() },
        Tuile { libelle: "Rond fini", valeur: pc(n.rond()),
                aide: format!("{} sur {} installs depuis le {depuis} ont fini au moins un rond", e(n.ronds), e(n.base_rond)) },
        Tuile { libelle: "Retour J1", valeur: pc(n.retour_j1()),
                aide: format!("{} sur {} installs assez anciens, depuis le {depuis}, ont fini un rond le lendemain", e(n.j1), e(n.base_j1)) },
        Tuile { libelle: "Retour J7", valeur: pc(n.retour_j7()),
                aide: format!("{} sur {} installs assez anciens, depuis le {depuis}, ont fini un rond 7 jours après", e(n.j7), e(n.base_j7)) },
        Tuile { libelle: "Part des installs", valeur: pc(Segment::taux(n.installs, racine.installs)),
                aide: format!("{} des {} installs en haut de l'arbre", e(n.installs), e(racine.installs)) },
        Tuile { libelle: "Part du nœud au-dessus", valeur: pc(parent.and_then(|p| Segment::taux(n.installs, p.installs))),
                aide: parent.map(|p| format!("{} des {} installs du nœud au-dessus", e(n.installs), e(p.installs)))
                    .unwrap_or_else(|| "Aucun filtre posé".into()) },
    ];
    Entonnoir { fragile, vide: None, pas, tuiles }
}

/// Le tunnel d'onboarding : chaque étape rapportée aux installs, la perte depuis la précédente, la pire en ambre.
fn tunnel(etapes: &[(&str, i64)], chemin: &[Filtre]) -> Tunnel {
    let premier = etapes.first().map(|e| e.1).unwrap_or(0);
    if premier <= 0 {
        return Tunnel { etapes: vec![] };
    }
    let pire = plus_grosse_perte(etapes);
    let choisie = chemin.iter().find_map(|f| if let Filtre::Ecran(r) = f { Some(*r as usize) } else { None });
    Tunnel {
        etapes: etapes.iter().enumerate().map(|(i, &(nom, n))| {
            let part = n as f64 / premier as f64;
            let perte = if i > 0 { etapes[i - 1].1 - n } else { 0 };
            Etape {
                nom: nom.into(),
                nombre: fmt::entier(n as f64),
                part,
                pourcent: fmt::pourcent(part, 0),
                perte: if perte > 0 { fmt::delta(-perte as f64, false) } else { String::new() },
                pire: Some(i) == pire,
                choisie: Some(i) == choisie,
                aide: (i > 0).then(|| if Some(i) == choisie { "Retirer ce filtre".into() } else { format!("Garder ceux qui ont passé « {nom} »") }),
            }
        }).collect(),
    }
}

/// Le nœud comparé valeur par valeur ; trié par la colonne `tri`, les taux fragiles en dernier, les installs d'avant
/// la mesure toujours au bout.
fn table(d: Dimension, enfants: &[Segment], noeud: &Segment, chemin: &[Filtre], tri: Option<&str>) -> Decoupe {
    let onglets = DIMENSIONS.iter().map(|&x| {
        let posees: Vec<String> = chemin.iter().filter_map(|f| match f {
            Filtre::Valeur(y, v) if *y == x => Some(decoupage::libelle(x, v)),
            Filtre::Sauf(y, v) if *y == x => Some(format!("sauf {}", decoupage::libelle(x, v))),
            _ => None,
        }).collect();
        let posees = (!posees.is_empty()).then(|| posees.join(", "));
        Onglet {
            id: x,
            nom: x.nom(),
            actif: x == d,
            aide: match &posees {
                Some(p) => format!("Déjà filtré : {p}"),
                None => format!("Découper par {}", x.nom().to_lowercase()),
            },
            posees,
        }
    }).collect();
    let (mesures, pas_mesure): (Vec<&Segment>, Vec<&Segment>) = enfants.iter().partition(|s| s.valeur != Dimension::PAS_MESURE);
    let hauts: Vec<Option<f64>> = COLONNES.iter().map(|c| {
        meilleur(&mesures.iter().map(|s| if (c.base)(s) >= Segment::MINIMUM { c.taux.and_then(|t| t(s)) } else { None }).collect::<Vec<_>>())
    }).collect();
    let aucun = vec![None; COLONNES.len()];
    let colonne = COLONNES.iter().find(|c| Some(c.titre) == tri);
    let mut triees = mesures.clone();
    if let Some(c) = colonne {
        // tri stable : à égalité, l'ordre de base reste
        triees.sort_by(|a, b| {
            let (fa, fb) = ((c.base)(a) >= Segment::MINIMUM, (c.base)(b) >= Segment::MINIMUM);
            fb.cmp(&fa).then_with(|| {
                let (ka, kb) = (c.cle(a).unwrap_or(-1.0), c.cle(b).unwrap_or(-1.0));
                kb.partial_cmp(&ka).unwrap_or(std::cmp::Ordering::Equal)
            })
        });
    }
    let lignes = triees.into_iter().chain(pas_mesure).map(|s| {
        let libelle = decoupage::libelle(d, &s.valeur);
        let pas_mesure = s.valeur == Dimension::PAS_MESURE;
        let posee = chemin.contains(&Filtre::Valeur(d, s.valeur.clone()));
        let part = Segment::taux(s.installs, noeud.installs).unwrap_or(0.0);
        LigneDecoupe {
            aide: if posee { format!("Retirer « {libelle} » du chemin") } else { format!("N'en garder que « {libelle} »") },
            aide_ecarter: format!("Écarter « {libelle} » : garder tous les autres"),
            cellules: cellules(s, if pas_mesure { &aucun } else { &hauts }),
            valeur: s.valeur.clone(),
            pas_mesure,
            posee,
            part: part.min(1.0),
            pourcent: taux1(part),
            libelle,
        }
    }).collect();
    Decoupe {
        titre: format!("Comparer par {}", d.nom().to_lowercase()),
        texte: format!("clique une valeur pour n'en garder que ses installs, l'œil barré pour l'écarter{}",
                       if decoupage::multiple(d) { " ; plusieurs réponses par install, les parts se recoupent" } else { "" }),
        nom: d.nom(),
        onglets,
        colonnes: COLONNES.iter().map(|c| {
            let actif = Some(c.titre) == tri;
            EnTete {
                titre: c.titre,
                largeur: c.largeur,
                actif,
                aide: format!("{}{}", c.aide, if actif { ". Clique pour revenir à l'ordre de base." } else { ". Clique pour trier." }),
            }
        }).collect(),
        lignes,
        total: (!enfants.is_empty()).then(|| cellules(noeud, &aucun)),
    }
}

fn cellules(s: &Segment, hauts: &[Option<f64>]) -> Vec<Cellule> {
    COLONNES.iter().zip(hauts).map(|(c, &haut)| {
        let base = (c.base)(s);
        let n = c.n.map(|n| fmt::entier(n(s) as f64));
        Cellule {
            aide: if c.taux.is_some() { format!("{} sur {}", n.clone().unwrap_or_default(), fmt::entier(base as f64)) } else { String::new() },
            taux: c.taux.map(|t| Taux::de(t(s), haut, base)),
            n,
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meilleur_demande_deux_taux_positifs() {
        assert_eq!(meilleur(&[Some(0.2)]), None);
        assert_eq!(meilleur(&[Some(0.2), None, Some(0.3)]), Some(0.3));
        assert_eq!(meilleur(&[Some(0.0), Some(0.0)]), None);
    }

    #[test]
    fn ratio_sur_les_jours_releves_des_deux() {
        let a = Chiffres { valeur: Some(6.0), avant: None, serie: vec![Some(2.0), Some(4.0), None], jours: Some(2) };
        let b = Chiffres { valeur: Some(4.0), avant: None, serie: vec![Some(1.0), None, Some(3.0)], jours: None };
        let r = ratio(&a, &b);
        assert_eq!(r.valeur, Some(2.0));
        assert_eq!(r.jours, Some(1));
        assert_eq!(r.serie, vec![Some(2.0), None, None]);
    }
}
