//! Le schéma de la base, documenté table par table dans docs/donnees.md, identique à la version macOS.
//! Pas de migrations : `create … if not exists`, une colonne nouvelle s'ajoute par `COLONNES`, avec une valeur par défaut.

/// Colonnes ajoutées après coup à une table existante : (table, colonne, définition).
pub const COLONNES: [(&str, &str, &str); 1] = [("video", "plateforme", "text not null default 'tiktok'")];

pub const SQL: &str = r#"
create table if not exists business (
    id text primary key, nom text not null, lettre text not null, teinte text not null,
    config text not null, ordre integer not null default 0);
create table if not exists serie (
    business text not null, metrique text not null, jour text not null, heure integer not null default -1,
    valeur real not null, maj text not null,
    primary key (business, metrique, jour, heure));
create table if not exists fil (
    id text primary key, business text not null, at text not null, type text not null,
    texte text not null, lien text);
create index if not exists fil_at on fil (at);
create table if not exists video (
    id text primary key, business text not null, compte text not null, legende text,
    titre text, format text, fiche text, publiee_le text, duree_s real, url text,
    verdict integer not null default 0, plateforme text not null default 'tiktok');
create table if not exists video_releve (
    video text not null, at text not null, vues integer not null, likes integer, commentaires integer,
    partages integer, enregistrements integer, watch_moyen_s real, completion real, abonnes integer,
    primary key (video, at));
create table if not exists video_analyse (video text primary key, at text not null, donnees text not null);
create table if not exists compte (
    id text primary key, business text not null, plateforme text not null, handle text not null,
    actif integer not null default 1, ajoute_le text not null);
create table if not exists compte_releve (
    compte text not null, at text not null, abonnes integer, videos integer, likes integer,
    primary key (compte, at));
create table if not exists app (
    id text primary key, nom text not null, editeur text, icone text, genre integer,
    note real, nb_notes integer, version text, business text, suivie integer not null default 1);
create table if not exists rang (
    app text not null, pays text not null, genre integer not null, liste text not null, at text not null,
    position integer not null,
    primary key (app, pays, genre, liste, at));
create index if not exists rang_at on rang (at);
create table if not exists avis (
    id text primary key, business text not null, pays text, note integer not null, titre text,
    texte text, auteur text, at text not null, repondu integer not null default 0);
create table if not exists a_traiter (
    id text primary key, business text, texte text not null, etiquette text,
    ton text not null default 'neutre', echeance text, source text not null, lien text,
    cree_le text not null, fait_le text);
create table if not exists sav_fil (
    business text not null, uid text not null, nom text, dernier_at text not null, dernier_equipe integer not null,
    apercu text not null, ouvert integer not null, non_vu integer not null,
    primary key (business, uid));
create table if not exists sav_message (
    business text not null, uid text not null, id text not null, equipe integer not null, texte text not null,
    at text not null, primary key (business, uid, id));
create table if not exists sav_retour (
    business text not null, id text not null, uid text, source text, categorie text, etoiles integer,
    message text not null, citation text, conversation text, traite integer not null, traite_le text,
    plateforme text, build text, at text not null, primary key (business, id));
create table if not exists sav_compte (
    business text not null, uid text not null, email text not null, primary key (business, uid));
create table if not exists install (
    business text not null, jour text not null, loin integer not null, paywall integer not null,
    essai integer not null, direct integer not null, rond integer not null,
    j1 integer not null, j7 integer not null, j14 integer not null, j30 integer not null,
    client text, valeurs text not null);
create index if not exists install_jour on install (business, jour);
create table if not exists paiement (
    business text not null, client text not null, etat text not null, lu text not null,
    primary key (business, client));
create table if not exists porte (
    business text not null, jour text not null, porte text not null,
    vues integer not null, essais integer not null, directs integer not null,
    primary key (business, jour, porte));
-- remplacées par install, paiement et porte
drop table if exists cohorte;
drop table if exists entonnoir;
drop table if exists decoupe;
create table if not exists collecte (
    source text not null, debut text not null, fin text not null, etat text not null,
    erreur text, lignes integer not null default 0);
create index if not exists collecte_source on collecte (source, debut);
create table if not exists veille_mot (
    business text not null, mot text not null, vues_min integer not null default 20000,
    actif integer not null default 1, ajoute text not null, cherche text,
    primary key (business, mot));
create table if not exists veille_compte (
    id text primary key, plateforme text not null, handle text not null, abonnes integer,
    mediane integer, nb_base integer, base_at text, decouvert text not null);
create table if not exists veille_video (
    id text primary key, business text not null, plateforme text not null, compte text not null,
    url text not null, legende text, publiee text, duree_s real, mot text, media text, trouvee text not null);
create index if not exists veille_video_compte on veille_video (plateforme, compte);
create table if not exists veille_releve (
    video text not null, at text not null, vues integer, likes integer, commentaires integer,
    partages integer, enregistrements integer,
    primary key (video, at));
create table if not exists veille_analyse (video text primary key, at text not null, modele text, donnees text not null);
create table if not exists veille_cout (
    video text not null, at text not null, modele text not null, entree integer not null, sortie integer not null,
    usd real not null);
create table if not exists reglage (cle text primary key, valeur text not null);
create table if not exists plan (
    ordre integer primary key, section text not null, texte text not null, coche integer not null default 0,
    fait integer not null default 0);

"#;
