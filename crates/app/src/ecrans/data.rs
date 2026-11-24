//! Les commandes de l'écran data : l'argent et toute la data de l'app sur la période (rétention, le rond, le bilan),
//! ou les installs d'un build comparés au build d'avant ; l'arbre des installs à part, pour qu'un clic dans l'arbre ne
//! relise que lui.
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
use noyau::lecture::rond::{JourRonds, ParJour, PremiersRonds};
use noyau::lecture::Lectures;
use noyau::metrique::{self as m, en_euros, EUR_PAR_USD};
use noyau::modeles::{Cohorte, Evenement, TypeFil};
use noyau::Res;
use serde::Serialize;
use tauri::State;

/// L'ordre des découpages : où ils s'arrêtent, ce qui se vend, sur quel téléphone, d'où ils viennent, puis qui ils sont.
const DIMENSIONS: [Dimension; 17] = [
    Loin, Produit, Plateforme, Source, Age, Motivation, Blocage, Niveau, Langue, Maternelle, Zone, Pays, Interet, Notifs, Compte,
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
    arbre: Arbre,
    retention: Retention,
    /// None tant que les événements manquent.
    rond: Option<Section>,
    bilan: Option<Section>,
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
        arbre: Arbre { texte: TEXTE_ARBRE, debut: f.debut(), fin: f.fin(), racine: vec![] },
        retention: Retention { section: Section { titre: "Rétention", texte: String::new(), kpis: vec![] },
                               sous_titre: sous_titre_cohortes(), cohortes: vec![] },
        rond: None,
        bilan: None,
    };
    Ok(match choisi {
        Some(b) => VueData {
            arbre: Arbre { texte: TEXTE_ARBRE, debut: b.debut.clone(), fin: b.fin.clone(), racine: vec![b.filtre()] },
            retention: retention_build(l, b, avant)?,
            ..base
        },
        None => VueData {
            argent: Some(argent(l, &f)?),
            retention: retention(l, &f)?,
            rond: rond(l, &f)?,
            bilan: bilan(l, &f)?,
            ..base
        },
    })
}

const TEXTE_ARBRE: &str = "Clique une marche, un écran du tunnel ou une valeur pour descendre ; le chemin du haut remonte. Chaque nœud dit combien de ses installs prennent l'essai, paient, commencent et finissent un rond, et l'écart avec le nœud du dessus.";

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

// le rond et le bilan, depuis les événements

/// Des chiffres lus jour par jour sur une fenêtre : la courbe, le total qui fait la valeur, de quoi comparer.
struct Lu<T> {
    serie: Vec<T>,
    total: T,
    avant: Option<T>,
    /// Les jours comptés quand la couverture ne prend qu'une partie de la fenêtre.
    jours: Option<i64>,
    /// Le premier jour couvert : avant, la courbe n'a pas de point.
    depuis: String,
}

impl<T: ParJour> Lu<T> {
    /// Aujourd'hui sur 14 jours de courbe, comparé au même jour de la semaine d'avant à la même heure si `heure` (lire
    /// reçoit l'instant de coupe) ; sinon la période, comparée à celle d'avant quand la couverture la prend entière.
    fn de(f: &Fenetre, depuis: &str, heure: bool, lire: impl Fn(&str, &str, Option<&str>) -> Res<Vec<T>>) -> Res<Lu<T>> {
        let fin = f.fin();
        if f.jour() {
            let serie = lire(&jour::decale(&fin, -13), &fin, None)?;
            let j7 = jour::decale(&fin, -7);
            let coupe = jour::iso(&(f.maintenant - chrono::Duration::days(7)));
            let avant = match heure && j7.as_str() >= depuis {
                true => Some(T::somme(&lire(&j7, &j7, Some(&coupe))?)),
                false => None,
            };
            return Ok(Lu { total: serie.last().cloned().unwrap_or_default(), serie, avant, jours: None, depuis: depuis.into() });
        }
        let (a, _) = f.avant().unwrap_or_default();
        let compare = a.as_str() >= depuis;
        let tout = lire(&if compare { a } else { f.debut() }, &fin, None)?;
        let (avant, serie) = tout.split_at(tout.len().saturating_sub(f.n() as usize));
        Ok(Lu::sur(serie.to_vec(), compare.then(|| T::somme(avant)), depuis))
    }

    /// `serie` comptée sur ses jours couverts.
    fn sur(serie: Vec<T>, avant: Option<T>, depuis: &str) -> Lu<T> {
        let couverts: Vec<T> = serie.iter().filter(|j| j.jour() >= depuis).cloned().collect();
        Lu {
            total: T::somme(&couverts),
            avant,
            jours: (!couverts.is_empty() && couverts.len() < serie.len()).then_some(couverts.len() as i64),
            serie,
            depuis: depuis.into(),
        }
    }

    fn chiffres(&self, mesure: impl Fn(&T) -> Option<f64>) -> Chiffres {
        Chiffres {
            valeur: mesure(&self.total),
            avant: self.avant.as_ref().and_then(&mesure),
            serie: self.serie.iter().map(|j| if j.jour() >= self.depuis.as_str() { mesure(j) } else { None }).collect(),
            jours: self.jours,
        }
    }
}

/// Les événements du rond, du bilan et du retour sur la fenêtre ; None tant que la table ne couvre pas un jour entier.
fn evenements(l: &Lectures, f: &Fenetre) -> Res<Option<Lu<JourRonds>>> {
    let Some(depuis) = l.evenements_depuis()? else { return Ok(None) };
    Lu::de(f, &depuis, true, |debut, fin, coupe| l.ronds_par_jour(debut, fin, coupe)).map(Some)
}

fn rond(l: &Lectures, f: &Fenetre) -> Res<Option<Section>> {
    let Some(lu) = evenements(l, f)? else { return Ok(None) };
    Ok(Some(Section {
        titre: "Le rond",
        texte: "Tous les ronds joués, nouveaux et anciens users, sans les filtres de l'arbre. Quittés en cours : les ronds quittés après au moins une réponse, sur les ronds commencés. Formes dites : les tournures du jour que l'apprenant a dites, sur celles du rond. Durée et réponse du tuteur : des médianes.".into(),
        kpis: vec![
            carte("Quittés en cours", lu.chiffres(JourRonds::quittes), taux1, fmt::points, true),
            carte("Formes dites", lu.chiffres(JourRonds::dites), taux1, fmt::points, false),
            carte("Réponses justes", lu.chiffres(JourRonds::justes), taux1, fmt::points, false),
            carte("Réponses à la voix", lu.chiffres(JourRonds::voix), taux1, fmt::points, false),
            carte("Durée d'un rond", lu.chiffres(JourRonds::duree), fmt::cumul, fmt::evolution, false),
            carte("Réponse du tuteur", lu.chiffres(JourRonds::tuteur), fmt::duree, fmt::evolution, true),
        ],
    }))
}

fn bilan(l: &Lectures, f: &Fenetre) -> Res<Option<Section>> {
    let Some(lu) = evenements(l, f)? else { return Ok(None) };
    Ok(Some(Section {
        titre: "Bilan et retour",
        texte: "Le bilan après le rond et son quiz, ce qui ramène dans l'app, la feuille d'avis maison.".into(),
        kpis: vec![
            carte("Bilan lu jusqu'au bout", lu.chiffres(JourRonds::bilan_fini), taux1, fmt::points, false),
            carte("Quiz du bilan juste", lu.chiffres(JourRonds::quiz_juste), taux1, fmt::points, false),
            carte("Ouvertes par une notif", lu.chiffres(JourRonds::par_notif), taux1, fmt::points, false),
            carte("Avis : oui", lu.chiffres(JourRonds::avis_oui), taux1, fmt::points, false),
        ],
    }))
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
    /// None tant qu'aucun install du nœud ne se mesure (avant `RONDS_DEPUIS`).
    premier_rond: Option<PremierRond>,
    tuiles: Vec<Tuile>,
}

/// L'anneau du premier rond : au centre la part des installs qui en commencent un, autour ses trois parts.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PremierRond {
    centre: String,
    ecart: Option<Ecart>,
    aide: String,
    parts: Vec<PartRond>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PartRond {
    libelle: &'static str,
    couleur: &'static str,
    /// La part des installs mesurés, de 0 à 1 : la longueur de l'arc.
    part: f64,
    valeur: String,
    sous: Option<String>,
    ecart: Option<Ecart>,
    aide: String,
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
    ecart: Option<Ecart>,
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

const COLONNES: [Colonne; 9] = [
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
    Colonne { titre: "Lancé", aide: "Ont commencé au moins un rond, sur les installs depuis la mesure des ronds", largeur: "court",
              n: None, taux: Some(Segment::commence), base: |s| s.base_rond },
    Colonne { titre: "Fini", aide: "Ont fini au moins un rond, sur les installs depuis la mesure des ronds", largeur: "court",
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
    // les ronds des installs du nœud, depuis qu'ils se mesurent
    let ronds = |k: usize| -> Res<PremiersRonds> {
        let d = debut.max(RONDS_DEPUIS);
        if fin < d { return Ok(PremiersRonds::default()) }
        Ok(PremiersRonds::somme(&l.premiers_ronds(&tous[..racine.len() + k], d, fin)?))
    };
    let parent = match chemin.len().checked_sub(1) {
        Some(k) => Some((&noeuds[k], ronds(k)?)),
        None => None,
    };
    Ok(VueArbre {
        miettes,
        entonnoir: entonnoir((noeud, &ronds(chemin.len())?), &noeuds[0], parent.as_ref().map(|(s, r)| (*s, r)), chemin),
        tunnel: tunnel(&etapes, chemin),
        decoupe: table(decoupe, &l.enfants(&tous, decoupe, debut, fin)?, noeud, chemin, tri),
        dimensions: DIMENSIONS.to_vec(),
    })
}

fn pc(t: Option<f64>) -> String { t.map(taux1).unwrap_or_else(|| VIDE.into()) }

/// L'écart avec le nœud du dessus, vert s'il va dans le bon sens (une baisse si `inverse`), gris quand la base est
/// trop petite pour trancher.
fn ecart(n: Option<f64>, p: Option<f64>, dire: fn(Option<f64>, Option<f64>) -> Option<String>, valeur: impl Fn(f64) -> String,
         base: i64, inverse: bool) -> Option<Ecart> {
    let texte = dire(n, p)?;
    let e = fmt::ecart(n, p).unwrap_or(0.0) * if inverse { -1.0 } else { 1.0 };
    Some(Ecart {
        texte,
        couleur: if base < Segment::MINIMUM || e.abs() < 0.0005 { "secondaire" } else if e > 0.0 { "succes" } else { "danger" },
        aide: format!("Le nœud du dessus : {}", p.map(valeur).unwrap_or_else(|| VIDE.into())),
    })
}

/// Le nœud ouvert en entonnoir : installs, paywall vu, essai, payant, chacun rapporté aux installs du nœud et comparé
/// au nœud du dessus ; dessous, les ronds de ses installs.
fn entonnoir((n, r): (&Segment, &PremiersRonds), racine: &Segment, parent: Option<(&Segment, &PremiersRonds)>,
             chemin: &[Filtre]) -> Entonnoir {
    let fragile = (n.installs > 0 && n.installs < Segment::MINIMUM).then(|| {
        format!("Moins de {} installs : un install de plus bouge les taux de plusieurs points", Segment::MINIMUM)
    });
    if n.installs == 0 {
        return Entonnoir {
            fragile, vide: Some("Aucun install ne passe ces filtres sur la période."), pas: vec![], premier_rond: None, tuiles: vec![],
        };
    }
    let e = |v: i64| fmt::entier(v as f64);
    let part = |nombre: i64| nombre as f64 / n.installs.max(1) as f64;
    let marche = |nom: &'static str, nombre: i64, couleur: &'static str, filtre: Filtre, taux: fn(&Segment) -> Option<f64>,
                  sous: Option<String>| {
        let posee = chemin.contains(&filtre);
        Pas::Marche {
            nom, nombre: e(nombre), couleur, sous, part: part(nombre),
            taux: Some(taux1(part(nombre))),
            ecart: parent.and_then(|(p, _)| ecart(taux(n), taux(p), fmt::points, taux1, n.installs, false)),
            filtre: Some(filtre), posee,
            aide: Some(if posee { "Retirer ce filtre".into() } else { format!("Garder ceux qui sont passés par « {nom} »") }),
        }
    };
    let passage = |t: Option<f64>, texte: &str, perte: i64| Pas::Passage {
        texte: format!("{} {texte}", pc(t)),
        perte: (perte > 0).then(|| fmt::delta(-perte as f64, false)),
    };
    let tous = format!("{} de tous les installs", taux1(n.installs as f64 / racine.installs.max(1) as f64));
    let dont: Vec<String> = [
        (n.payants > n.essais_payes).then(|| format!("{} sans essai", e(n.payants - n.essais_payes))),
        (n.a_verifier > 0).then(|| format!("{} à vérifier", e(n.a_verifier))),
    ].into_iter().flatten().collect();
    let pas = vec![
        Pas::Marche {
            nom: "Installs", nombre: e(n.installs), couleur: "secondaire",
            sous: parent.map(|(p, _)| match chemin.len() {
                1 => tous.clone(),
                _ => format!("{} du nœud au-dessus, {tous}", pc(Segment::taux(n.installs, p.installs))),
            }),
            part: 1.0, taux: None, ecart: None, filtre: None, posee: false, aide: None,
        },
        passage(n.paywall_vu(), "voient le paywall", n.installs - n.paywall),
        marche("Paywall vu", n.paywall, "bleu", Filtre::Paywall, Segment::paywall_vu, None),
        passage(Segment::taux(n.essais, n.paywall), "prennent l'essai", n.paywall - n.essais),
        marche("Essai gratuit", n.essais, "violet", Filtre::Essai, Segment::essai,
               (n.en_cours > 0).then(|| format!("dont {} en cours", e(n.en_cours)))),
        passage(n.essai_vers_payant(), "des essais finis paient", n.essais_finis() - n.essais_payes),
        marche("Payant", n.payants, "succes", Filtre::Payant, Segment::payant,
               (!dont.is_empty()).then(|| format!("dont {}", dont.join(", ")))),
    ];
    Entonnoir { fragile, vide: None, pas, premier_rond: premier_rond(n, parent.map(|(p, _)| p)), tuiles: tuiles(r, parent.map(|(_, p)| p)) }
}

/// Les installs mesurés du nœud qui finissent un rond, qui en commencent un sans en finir aucun, qui n'en commencent
/// aucun ; chaque part comparée au nœud du dessus. Un install qui finit un rond l'a commencé, les trois font le tout.
fn premier_rond(n: &Segment, parent: Option<&Segment>) -> Option<PremierRond> {
    if n.base_rond == 0 { return None }
    let e = |v: i64| fmt::entier(v as f64);
    let fini = |s: &Segment| Segment::taux(s.ronds, s.base_rond);
    let sans_finir = |s: &Segment| Segment::taux(s.commences - s.ronds, s.base_rond);
    let jamais = |s: &Segment| Segment::taux(s.base_rond - s.commences, s.base_rond);
    let part = |libelle, couleur, mesure: fn(&Segment) -> Option<f64>, inverse, nombre: i64, sous: Option<String>, aide: String| PartRond {
        libelle, couleur, part: mesure(n).unwrap_or(0.0), valeur: pc(mesure(n)), sous, aide: format!("{} installs : {aide}", e(nombre)),
        ecart: parent.and_then(|p| ecart(mesure(n), mesure(p), fmt::points, taux1, n.base_rond, inverse)),
    };
    // avant RONDS_DEPUIS, les ronds ne se mesuraient pas
    let base = match n.base_rond < n.installs {
        true => format!("Sur les {} installs mesurés, les ronds se lisent depuis le {}", e(n.base_rond), fmt::jour_court_cle(RONDS_DEPUIS)),
        false => format!("Sur les {} installs du nœud", e(n.base_rond)),
    };
    Some(PremierRond {
        centre: pc(n.commence()),
        ecart: parent.and_then(|p| ecart(n.commence(), p.commence(), fmt::points, taux1, n.base_rond, false)),
        aide: format!("{base}, {} ont commencé au moins un rond", e(n.commences)),
        parts: vec![
            part("Rond fini", "succes", fini, false, n.ronds, None, "ont fini au moins un rond".into()),
            part("Commencé, pas fini", "ambre", sans_finir, true, n.commences - n.ronds,
                 n.fin_du_rond().map(|t| format!("{} de ceux qui commencent", taux1(1.0 - t))),
                 format!("ont commencé un rond sans en finir aucun, sur les {} qui ont commencé", e(n.commences))),
            part("Pas commencé", "tertiaire", jamais, true, n.base_rond - n.commences, None,
                 "n'ont commencé aucun rond".into()),
        ],
    })
}

/// Ce que les installs du nœud ont fait de leurs ronds, chaque moyenne comparée au nœud du dessus.
fn tuiles(r: &PremiersRonds, parent: Option<&PremiersRonds>) -> Vec<Tuile> {
    let e = |v: i64| fmt::entier(v as f64);
    let tuile = |libelle, mesure: fn(&PremiersRonds) -> Option<f64>, texte: fn(f64) -> String, inverse, aide: String| Tuile {
        libelle, valeur: mesure(r).map(texte).unwrap_or_else(|| VIDE.into()), aide,
        ecart: parent.and_then(|p| ecart(mesure(r), mesure(p), fmt::evolution, texte, r.installs, inverse)),
    };
    vec![
        tuile("Délai au 1er rond", PremiersRonds::delai, fmt::cumul, true,
              format!("La médiane, de l'install au premier rond commencé, sur {} installs", e(r.delais.len() as i64))),
        tuile("Ronds par install", PremiersRonds::ronds_par_install, |v| fmt::decimal(v, 2), false,
              format!("{} ronds finis par les {} installs mesurés, depuis leur install", e(r.ronds), e(r.installs))),
        tuile("Jours avec un rond", PremiersRonds::jours_par_fidele, |v| fmt::decimal(v, 1), false,
              format!("{} jours avec un rond fini, pour les {} installs qui en ont fini un", e(r.jours), e(r.fini))),
    ]
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
    fn premier_rond_partage_les_installs_mesures() {
        let segment = |installs, commences, ronds| Segment { installs, base_rond: installs, commences, ronds, ..Default::default() };
        let (n, p) = (segment(50, 40, 30), segment(100, 80, 70));
        let a = premier_rond(&n, Some(&p)).unwrap();
        let parts: Vec<f64> = a.parts.iter().map(|p| p.part).collect();
        assert_eq!(parts, [0.6, 0.2, 0.2]);
        assert_eq!(a.parts[1].sous.as_deref(), Some("25,0 % de ceux qui commencent"));
        // 20 % commencent sans finir, contre 10 % au-dessus : une hausse, donc en rouge ; même part qui commence, en gris
        let couleurs: Vec<_> = a.parts.iter().map(|p| p.ecart.as_ref().unwrap().couleur).collect();
        assert_eq!(couleurs, ["danger", "danger", "secondaire"]);
        assert_eq!(a.ecart.unwrap().couleur, "secondaire");
        assert!(premier_rond(&Segment { installs: 5, ..Default::default() }, None).is_none());
    }
}
