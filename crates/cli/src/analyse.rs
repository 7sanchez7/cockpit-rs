//! `cockpit schema` et les vues `v_*` : ce qu'un agent lit avant d'écrire un `select`, et les jointures qu'il n'a plus
//! à réécrire. Les vues sont temporaires : elles vivent sur la connexion en lecture seule de la CLI, la base n'est
//! jamais modifiée et garde le schéma de la version Swift.
use std::collections::BTreeMap;

use noyau::db::{Database, Ligne, Valeur};
use noyau::dimension::Dimension;
use noyau::metrique as m;
use noyau::{Erreur, Res};

/// Les métriques de `serie` pivotées dans `v_jour`, totaux de tous les stores.
const METRIQUES_JOUR: [&str; 19] = [
    m::INSTALLS, m::TELECHARGEMENTS, m::ACTIFS, m::RONDS_COMMENCES, m::RONDS, m::ESSAIS, m::ACHATS,
    m::NOUVEAUX_CLIENTS, m::ABONNES, m::ESSAIS_ACTIFS, m::MRR_BRUT, m::MRR_NET, m::REVENU, m::COUT_IA, m::TOKENS_IA,
    m::ECHECS_IA, m::VUES_TIKTOK, m::NOTE, m::NB_NOTES,
];

/// Les vues et ce qu'elles rendent, pour `cockpit schema`.
const VUES: [(&str, &str); 3] = [
    ("v_install", "install + paiement, une ligne par install : semaine (son lundi), paiement (etat), paye (0 ou 1), \
        une colonne par dimension de valeurs (plusieurs réponses séparées par des virgules)"),
    ("v_jour", "serie pivotée, une ligne par business et jour (heure -1), une colonne par métrique, total des stores"),
    ("v_cohorte", "par business et semaine d'install : installs, puis des % ; pct_j1 à pct_j30 sur les seuls installs \
        dont la fenêtre est passée, nuls sinon"),
];

/// Les jours d'événements que `cockpit schema evenement` parcourt.
const JOURS_EVENEMENTS: i64 = 14;
/// Les valeurs les plus fréquentes montrées par dimension.
const VALEURS_MONTREES: usize = 8;

/// La base de l'app en lecture seule, avec les vues.
pub fn base() -> Res<Database> {
    let chemin = noyau::chemins::base();
    let chemin = chemin.to_str().ok_or_else(|| Erreur::Commande("chemin de la base illisible".into()))?;
    let db = Database::ouvrir(chemin, true)?;
    db.script(&vues())?;
    Ok(db)
}

fn vues() -> String {
    let dimensions: String = Dimension::TOUTES.into_iter().filter(|d| *d != Dimension::Loin)
        .map(|d| format!(", (select group_concat(value, ',') from json_each(i.valeurs, '$.{0}')) as {0}", d.brut()))
        .collect();
    let metriques: String = METRIQUES_JOUR.iter()
        .map(|x| format!(", max(case metrique when '{x}' then valeur end) as {x}"))
        .collect();
    let liste = METRIQUES_JOUR.map(|x| format!("'{x}'")).join(", ");
    let pct = |c: &str| format!(", round(100.0 * avg({c}), 1) as pct_{c}");
    let taux: String = ["paywall", "essai", "direct", "paye", "commence", "rond"].map(pct).concat();
    // une fenêtre jN se ferme N + 1 jours après l'install ; un jour de marge pour l'heure de Paris
    let retention: String = [1, 7, 14, 30]
        .map(|n| format!(", round(100.0 * avg(case when jour <= date('now', '-{} day') then j{n} end), 1) as pct_j{n}", n + 2))
        .concat();
    format!("\
create temp view if not exists v_install as
select i.business, i.jour, date(i.jour, 'weekday 0', '-6 day') as semaine, i.loin, i.paywall, i.essai, i.direct,
    i.commence, i.rond, i.j1, i.j7, i.j14, i.j30, i.commences, i.ronds, i.jours_ronds, i.premier_rond, i.dernier_rond,
    i.client, i.personne, p.etat as paiement, coalesce(p.etat = 'paye', 0) as paye{dimensions}
from install i left join paiement p on p.business = i.business and p.client = i.client;
create temp view if not exists v_jour as
select business, jour{metriques}
from serie where heure = -1 and metrique in ({liste}) group by business, jour;
create temp view if not exists v_cohorte as
select business, semaine, count(*) as installs{taux}{retention}
from (select i.*, date(i.jour, 'weekday 0', '-6 day') as semaine, coalesce(p.etat = 'paye', 0) as paye
    from install i left join paiement p on p.business = i.business and p.client = i.client)
group by business, semaine;")
}

fn texte(l: &Ligne, c: &str) -> String { l.texte(c).unwrap_or_default() }
fn nombre(l: &Ligne, c: &str) -> i64 { l.entier(c).unwrap_or(0) }

/// Les noms des colonnes d'une table ou d'une vue, avec leur type si `types`.
fn colonnes(db: &Database, t: &str, types: bool) -> Res<Vec<String>> {
    let lignes = db.query("select name, type from pragma_table_info(?)", vec![Valeur::Texte(t.into())])?;
    Ok(lignes.iter().map(|l| {
        let (nom, ty) = (texte(l, "name"), texte(l, "type").to_lowercase());
        if types && !ty.is_empty() { format!("{nom} {ty}") } else { nom }
    }).collect())
}

/// `cockpit schema` sans argument, ou le détail d'une table ou d'une vue.
pub fn schema(db: &Database, cible: Option<&str>) -> Res<()> {
    match cible {
        None => apercu(db),
        Some(t) => detail(db, t),
    }
}

fn apercu(db: &Database) -> Res<()> {
    let businesses: Vec<String> = db.query("select id from business order by ordre", vec![])?
        .iter().map(|l| texte(l, "id")).collect();
    println!("Lecture seule. jour = yyyy-MM-dd à Paris, at = ISO UTC, booléens 0 ou 1, JSON par \
        json_extract(col, '$.cle'). Filtrer business : {}.", businesses.join(", "));
    println!("Tables : lignes, premier et dernier jour, colonnes.");
    let tables = db.query("select name from sqlite_master where type = 'table' and name not like 'sqlite_%' \
        order by name", vec![])?;
    for t in tables.iter().map(|l| texte(l, "name")) {
        let cols = colonnes(db, &t, false)?;
        let date = if cols.iter().any(|c| c == "jour") { "jour" }
            else if cols.iter().any(|c| c == "at") { "substr(at, 1, 10)" }
            else { "null" };
        let l = db.query(&format!("select count(*) as n, min({date}) as debut, max({date}) as fin from \"{t}\""), vec![])?;
        let l = l.first().cloned().unwrap_or_default();
        let plage = l.texte("debut").map(|d| format!(" {d} → {}", texte(&l, "fin"))).unwrap_or_default();
        println!("{t} {}{plage} : {}", nombre(&l, "n"), cols.join(" "));
    }
    println!("Vues, dans cockpit sql seulement :");
    for (v, quoi) in VUES {
        println!("{v} : {quoi}\n  {}", colonnes(db, v, false)?.join(" "));
    }
    println!("Détail : cockpit schema <table|vue> ; evenement, serie et install y ajoutent le contenu de props, \
        metrique et valeurs.");
    Ok(())
}

fn detail(db: &Database, t: &str) -> Res<()> {
    let cols = colonnes(db, t, true)?;
    if cols.is_empty() { return Err(Erreur::Commande(format!("{t} : ni table ni vue"))) }
    println!("{t} : {}", cols.join(", "));
    match t {
        "evenement" => evenements(db),
        "serie" | "v_jour" => metriques(db),
        "install" | "v_install" => dimensions(db),
        _ => Ok(()),
    }
}

/// Une ligne par business quand il y en a plusieurs, rien sinon.
fn entete(courant: &mut String, business: &str) {
    if courant != business {
        println!("[{business}]");
        *courant = business.into();
    }
}

fn evenements(db: &Database) -> Res<()> {
    // les paires (business, nom) d'abord, pour lire le reste par l'index (business, nom, jour)
    let recent = format!("e.business = p.business and e.nom = p.nom and e.jour >= date('now', '-{JOURS_EVENEMENTS} day')");
    let paires = "with p as (select business, nom from evenement group by business, nom)";
    let noms = db.query(&format!("{paires} select * from (select p.business, p.nom, (select count(*) from evenement e \
        where {recent}) as n from p) where n > 0 order by business, n desc"), vec![])?;
    let cles = db.query(&format!("{paires} select p.business, p.nom, j.key from p join evenement e on {recent}, \
        json_each(e.props) j group by p.business, p.nom, j.key order by count(*) desc"), vec![])?;
    let mut par_nom: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for l in &cles {
        par_nom.entry((texte(l, "business"), texte(l, "nom"))).or_default().push(texte(l, "key"));
    }
    println!("Événements des {JOURS_EVENEMENTS} derniers jours, les plus nombreux d'abord : nom, nombre, clés de \
        props de la plus à la moins fréquente.");
    let mut courant = String::new();
    for l in &noms {
        let (business, nom) = (texte(l, "business"), texte(l, "nom"));
        entete(&mut courant, &business);
        let cles = par_nom.remove(&(business, nom.clone())).unwrap_or_default();
        println!("{nom} {} : {}", nombre(l, "n"), cles.join(" "));
    }
    Ok(())
}

/// Les suffixes d'une métrique, son premier et son dernier jour.
type Groupe = (Vec<String>, String, String);

fn metriques(db: &Database) -> Res<()> {
    let lignes = db.query("select business, metrique, heure >= 0 as horaire, min(jour) as debut, max(jour) as fin \
        from serie group by business, metrique, horaire order by business, horaire, metrique", vec![])?;
    // une métrique et ses suffixes (.app_store, .anthropic…) sur une ligne
    let mut groupes: BTreeMap<(String, i64, String), Groupe> = BTreeMap::new();
    for l in &lignes {
        let metrique = texte(l, "metrique");
        let (base, suffixe) = metrique.split_once('.').map(|(b, s)| (b.to_string(), format!(".{s}")))
            .unwrap_or((metrique.clone(), String::new()));
        let g = groupes.entry((texte(l, "business"), nombre(l, "horaire"), base))
            .or_insert_with(|| (vec![], texte(l, "debut"), texte(l, "fin")));
        if !suffixe.is_empty() { g.0.push(suffixe) }
        g.1 = g.1.clone().min(texte(l, "debut"));
        g.2 = g.2.clone().max(texte(l, "fin"));
    }
    println!("Métriques : premier et dernier jour. heure -1 = le jour entier, 0 à 23 = l'heure de Paris ; un suffixe \
        .x = la part d'un store ou d'un fournisseur, le nom nu = le total s'il existe.");
    let mut courant = (String::new(), 0);
    for ((business, horaire, base), (suffixes, debut, fin)) in groupes {
        if courant.0 != business { courant.1 = 0 }
        entete(&mut courant.0, &business);
        if horaire != courant.1 {
            println!("par heure :");
            courant.1 = horaire;
        }
        let suffixes = if suffixes.is_empty() { String::new() } else { format!(" {}", suffixes.join(" ")) };
        println!("{base}{suffixes} : {debut} → {fin}");
    }
    Ok(())
}

fn dimensions(db: &Database) -> Res<()> {
    let repondu = db.query("select i.business, j.key, count(*) as n from install i, json_each(i.valeurs) j \
        group by i.business, j.key order by i.business, n desc", vec![])?;
    let valeurs = db.query("select i.business, j.key, v.value, count(*) as n from install i, json_each(i.valeurs) j, \
        json_each(j.value) v group by i.business, j.key, v.value order by n desc", vec![])?;
    let mut par_cle: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for l in &valeurs {
        let valeur = l.colonnes.get("value").map(crate::cellule).unwrap_or_default();
        par_cle.entry((texte(l, "business"), texte(l, "key"))).or_default()
            .push(format!("{valeur} {}", nombre(l, "n")));
    }
    println!("valeurs : installs qui ont répondu, puis les réponses les plus fréquentes (une colonne de v_install \
        chacune).");
    let mut courant = String::new();
    for l in &repondu {
        let (business, cle) = (texte(l, "business"), texte(l, "key"));
        entete(&mut courant, &business);
        let tout = par_cle.remove(&(business, cle.clone())).unwrap_or_default();
        let reste = tout.len().saturating_sub(VALEURS_MONTREES);
        let suite = if reste > 0 { format!(", … {reste} autres") } else { String::new() };
        println!("{cle} {} : {}{suite}", nombre(l, "n"), tout.into_iter().take(VALEURS_MONTREES).collect::<Vec<_>>().join(", "));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vues_valides() {
        let store = noyau::store::Store::memoire().unwrap();
        let db = &store.db;
        db.script(&vues()).unwrap();
        db.script("insert into install (business, jour, loin, paywall, essai, direct, rond, j1, j7, j14, j30, client, \
            valeurs) values ('b', '2026-01-07', 3, 1, 1, 0, 1, 1, 0, 0, 0, 'c', \
            '{\"pays\":[\"FR\"],\"interet\":[\"music\",\"series\"]}');
            insert into paiement values ('b', 'c', 'paye', '2026-01-08');
            insert into serie values ('b', 'installs', '2026-01-07', -1, 4, '');").unwrap();
        let (_, l) = db.tableau("select semaine, pays, interet, paye from v_install").unwrap();
        assert_eq!(l[0].iter().map(crate::cellule).collect::<Vec<_>>(), ["2026-01-05", "FR", "music,series", "1"]);
        let (_, l) = db.tableau("select installs, pct_paye, pct_j1 from v_cohorte").unwrap();
        assert_eq!(l[0].iter().map(crate::cellule).collect::<Vec<_>>(), ["1", "100.0", "100.0"]);
        let (_, l) = db.tableau("select installs, actifs from v_jour").unwrap();
        assert_eq!(l[0].iter().map(crate::cellule).collect::<Vec<_>>(), ["4.0", ""]);
        for (v, _) in VUES { assert!(!colonnes(db, v, false).unwrap().is_empty()) }
    }
}
