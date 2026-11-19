//! La base du cockpit. Les sources écrivent par ces méthodes, les écrans lisent par `lecture`.
//! Chaque table et chaque métrique est décrite dans docs/donnees.md.
use crate::business::{Business, Config, Graine};
use crate::db::{Database, Valeur};
use crate::erreur::{Erreur, Res};
use crate::jour::{self, Date};
use crate::modeles::*;
use crate::p;
use crate::schema;
use chrono::Duration;
use parking_lot::Mutex;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

pub struct Store {
    pub db: Database,
    /// Incrémenté après chaque collecte : les écrans qui le lisent se redessinent.
    version: AtomicU64,
    /// Lignes du fil insérées depuis le dernier `prendre_nouveaux`, pour les notifications.
    nouveaux: Mutex<Vec<Evenement>>,
}

impl Store {
    /// `graine` est appliquée si elle a changé depuis la dernière fois (docs/collecte.md § Graine).
    pub fn ouvrir_a(chemin: &Path, graine: Option<&Path>) -> Res<Store> {
        Self::avec(Database::ouvrir(&chemin.to_string_lossy(), false)?, graine)
    }

    /// Une base en mémoire, pour les tests.
    pub fn memoire() -> Res<Store> { Self::avec(Database::memoire()?, None) }

    /// Une base en mémoire semée avec une graine, pour les tests.
    pub fn memoire_avec(graine: &Path) -> Res<Store> { Self::avec(Database::memoire()?, Some(graine)) }

    fn avec(db: Database, graine: Option<&Path>) -> Res<Store> {
        db.script(schema::SQL)?;
        for (table, colonne, definition) in schema::COLONNES {
            if !db.colonne_existe(table, colonne)? {
                db.execute(&format!("alter table {table} add column {colonne} {definition}"), p![])?;
            }
        }
        let s = Store { db, version: AtomicU64::new(0), nouveaux: Mutex::new(Vec::new()) };
        if let Some(g) = graine { s.semer(g)?; }
        s.db.execute("delete from collecte where debut < ?", p![jour::maintenant() - Duration::days(30)])?;
        Ok(s)
    }

    /// La base de l'app, dans le dossier de données de l'utilisateur.
    pub fn ouvrir() -> Res<Store> {
        let fichier = crate::chemins::base();
        if let Some(d) = fichier.parent() { std::fs::create_dir_all(d)?; }
        Self::ouvrir_a(&fichier, Some(&crate::chemins::graine()))
    }

    /// Écrit les businesses de la graine et ajoute ses comptes absents, une fois par version du fichier : un compte mis
    /// en pause reste en pause, un business supprimé dans les réglages ne revient qu'avec une nouvelle graine.
    fn semer(&self, chemin: &Path) -> Res<()> {
        let Ok(meta) = std::fs::metadata(chemin) else { return Ok(()) };
        let Ok(modifie) = meta.modified() else { return Ok(()) };
        let date = jour::iso(&Date::from(modifie));
        if self.reglage("graine")?.as_deref() == Some(date.as_str()) { return Ok(()); }
        let g: Graine = std::fs::read(chemin)
            .map_err(Erreur::from)
            .and_then(|d| serde_json::from_slice(&d).map_err(Erreur::from))
            .map_err(|e| Erreur::Commande(format!("{} illisible : {e}", chemin.display())))?;
        let suivis: HashSet<String> = self.comptes()?.iter().map(Compte::id).collect();
        self.lot(|| {
            for b in &g.businesses { self.enregistrer(b)?; }
            for c in g.comptes.iter().filter(|c| !suivis.contains(&c.id())) { self.ajouter(c)?; }
            self.regler("graine", Some(&date))
        })
    }

    pub fn version(&self) -> u64 { self.version.load(Ordering::SeqCst) }
    pub fn a_change(&self) { self.version.fetch_add(1, Ordering::SeqCst); }

    /// Plusieurs écritures en une transaction.
    pub fn lot<T>(&self, f: impl FnOnce() -> Res<T>) -> Res<T> { self.db.transaction(f) }

    // business

    pub fn businesses(&self) -> Res<Vec<Business>> {
        self.db.query("select * from business order by ordre, nom", p![])?.iter().map(|l| {
            let config: Config = serde_json::from_str(&l.texte("config").unwrap_or_else(|| "{}".into()))?;
            Ok(Business {
                id: l.texte("id").unwrap_or_default(), nom: l.texte("nom").unwrap_or_default(),
                lettre: l.texte("lettre").unwrap_or_default(),
                teinte: l.texte("teinte").unwrap_or_else(|| "menthe".into()), config,
            })
        }).collect()
    }

    pub fn enregistrer(&self, b: &Business) -> Res<()> {
        let config = serde_json::to_string(&b.config)?;
        self.db.execute("
            insert into business (id, nom, lettre, teinte, config) values (?, ?, ?, ?, ?)
            on conflict (id) do update set nom = excluded.nom, lettre = excluded.lettre,
                teinte = excluded.teinte, config = excluded.config",
            p![&b.id, &b.nom, &b.lettre, &b.teinte, config])?;
        Ok(())
    }

    pub fn supprimer_business(&self, id: &str) -> Res<()> {
        self.db.execute("delete from business where id = ?", p![id])?;
        Ok(())
    }

    // comptes

    pub fn comptes(&self) -> Res<Vec<Compte>> {
        Ok(self.db.query("select * from compte order by business, plateforme, ajoute_le", p![])?.iter().filter_map(|l| {
            let p = Plateforme::depuis(&l.texte("plateforme")?)?;
            Some(Compte { business: l.texte("business").unwrap_or_default(), plateforme: p, handle: l.texte("handle")?, actif: l.booleen("actif") })
        }).collect())
    }

    /// Suit un compte ; un compte déjà là change de business et repart.
    pub fn ajouter(&self, c: &Compte) -> Res<()> {
        self.db.execute("
            insert into compte (id, business, plateforme, handle, actif, ajoute_le) values (?, ?, ?, ?, ?, ?)
            on conflict (id) do update set business = excluded.business, actif = excluded.actif",
            p![c.id(), &c.business, c.plateforme.brut(), &c.handle, c.actif, jour::maintenant()])?;
        Ok(())
    }

    pub fn activer_compte(&self, id: &str, actif: bool) -> Res<()> {
        self.db.execute("update compte set actif = ? where id = ?", p![actif, id])?;
        Ok(())
    }

    /// Le compte n'est plus suivi ni affiché ; ses vidéos et relevés restent dans la base.
    pub fn retirer_compte(&self, id: &str) -> Res<()> {
        self.db.execute("delete from compte where id = ?", p![id])?;
        Ok(())
    }

    pub fn releve_compte(&self, r: &ReleveCompte) -> Res<()> {
        self.db.execute("insert or replace into compte_releve (compte, at, abonnes, videos, likes) values (?, ?, ?, ?, ?)",
            p![&r.compte, r.at, r.abonnes, r.videos, r.likes])?;
        Ok(())
    }

    // séries

    /// Écrit la valeur d'une métrique ; `heure` -1 = la journée entière. Réécrit la ligne si elle existe.
    pub fn serie(&self, business: &str, metrique: &str, jour: &str, heure: i64, valeur: f64) -> Res<()> {
        self.db.execute("
            insert into serie (business, metrique, jour, heure, valeur, maj) values (?, ?, ?, ?, ?, ?)
            on conflict (business, metrique, jour, heure) do update set valeur = excluded.valeur, maj = excluded.maj",
            p![business, metrique, jour, heure, valeur, jour::maintenant()])?;
        Ok(())
    }

    /// Écrit la journée de `metrique` comme la somme des journées de `parts` ; rien si aucune n'est relevée.
    pub fn somme(&self, business: &str, metrique: &str, parts: &[String], jour: &str) -> Res<()> {
        let trous = vec!["?"; parts.len()].join(", ");
        let mut params = p![business, metrique, jour, jour::maintenant(), business, jour];
        params.extend(parts.iter().map(Valeur::from));
        self.db.execute(&format!("
            insert into serie (business, metrique, jour, heure, valeur, maj)
            select ?, ?, ?, -1, sum(valeur), ? from serie
            where business = ? and jour = ? and heure = -1 and metrique in ({trous})
            group by business
            on conflict (business, metrique, jour, heure) do update set valeur = excluded.valeur, maj = excluded.maj"), params)?;
        Ok(())
    }

    /// Vrai s'il manque hier ou le plus vieux des `jours` jours finissant aujourd'hui : l'historique est à relire.
    pub fn historique_incomplet(&self, business: &str, metrique: &str, jours: i64, aujourdhui: &str) -> Res<bool> {
        for recul in [1, jours - 1] {
            let l = self.db.query("select 1 from serie where business = ? and metrique = ? and jour = ? and heure = -1",
                p![business, metrique, jour::decale(aujourdhui, -recul)])?;
            if l.is_empty() { return Ok(true); }
        }
        Ok(false)
    }

    // fil

    /// Ajoute une ligne au fil ; rend `true` si elle est nouvelle (elle sera notifiée).
    pub fn fil(&self, e: &Evenement) -> Res<bool> {
        let n = self.db.execute("insert or ignore into fil (id, business, at, type, texte, lien) values (?, ?, ?, ?, ?, ?)",
            p![&e.id, &e.business, e.at, e.type_.brut(), &e.texte, &e.lien])?;
        if n == 0 { return Ok(false); }
        self.nouveaux.lock().push(e.clone());
        Ok(true)
    }

    /// Les lignes du fil ajoutées par une source depuis le dernier appel.
    pub fn prendre_nouveaux(&self, source: &str) -> Vec<Evenement> {
        let prefixe = format!("{source}:");
        let mut n = self.nouveaux.lock();
        let (miens, autres): (Vec<_>, Vec<_>) = n.drain(..).partition(|e| e.id.starts_with(&prefixe));
        *n = autres;
        miens
    }

    // vidéos

    /// Ce que la plateforme dit d'une vidéo. Ne touche pas à ce que la fiche a écrit.
    pub fn video(&self, v: &Video) -> Res<()> {
        self.db.execute("
            insert into video (id, business, compte, legende, publiee_le, duree_s, url, plateforme) values (?, ?, ?, ?, ?, ?, ?, ?)
            on conflict (id) do update set business = excluded.business, compte = excluded.compte, plateforme = excluded.plateforme,
                legende = coalesce(excluded.legende, video.legende),
                publiee_le = coalesce(excluded.publiee_le, video.publiee_le),
                duree_s = coalesce(excluded.duree_s, video.duree_s),
                url = coalesce(excluded.url, video.url)",
            p![&v.id, &v.business, &v.compte, &v.legende, v.publiee_le, v.duree_s, &v.url, v.plateforme.brut()])?;
        Ok(())
    }

    /// Ce que la fiche `contenu/bank` dit d'une vidéo.
    pub fn fiche_video(&self, f: &FicheVideo) -> Res<()> {
        self.db.execute("
            insert into video (id, business, compte, url, titre, format, fiche, verdict) values (?, ?, ?, ?, ?, ?, ?, ?)
            on conflict (id) do update set titre = excluded.titre, format = excluded.format,
                fiche = excluded.fiche, verdict = excluded.verdict",
            p![&f.id, &f.business, &f.compte, &f.url, &f.titre, &f.format, &f.fiche, f.verdict])?;
        Ok(())
    }

    pub fn releve(&self, r: &Releve) -> Res<()> {
        self.db.execute("
            insert or replace into video_releve (video, at, vues, likes, commentaires, partages, enregistrements,
                watch_moyen_s, completion, abonnes) values (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            p![&r.video, r.at, r.vues, r.likes, r.commentaires, r.partages, r.enregistrements, r.watch_moyen_s, r.completion, r.abonnes])?;
        Ok(())
    }

    /// Remplace l'analyse d'une vidéo, en JSON (docs/donnees.md).
    pub fn analyse(&self, video: &str, at: &Date, a: &Analyse) -> Res<()> {
        self.db.execute("insert or replace into video_analyse (video, at, donnees) values (?, ?, ?)",
            p![video, at, serde_json::to_string(a)?])?;
        Ok(())
    }

    // apps et classements

    /// Ce que l'App Store dit d'une app. Ne touche pas à `suivie`.
    pub fn app(&self, a: &Appli) -> Res<()> {
        self.db.execute("
            insert into app (id, nom, editeur, icone, genre, note, nb_notes, version, business)
            values (?, ?, ?, ?, ?, ?, ?, ?, ?)
            on conflict (id) do update set nom = excluded.nom, editeur = excluded.editeur, icone = excluded.icone,
                genre = excluded.genre, note = excluded.note, nb_notes = excluded.nb_notes,
                version = excluded.version, business = coalesce(excluded.business, app.business)",
            p![&a.id, &a.nom, &a.editeur, &a.icone, a.genre, a.note, a.nb_notes, &a.version, &a.business])?;
        Ok(())
    }

    pub fn suivre(&self, app: &str, oui: bool) -> Res<()> {
        self.db.execute("update app set suivie = ? where id = ?", p![oui, app])?;
        Ok(())
    }

    pub fn apps_suivies(&self) -> Res<Vec<Appli>> {
        Ok(self.db.query("select * from app where suivie = 1 order by business is null, nom", p![])?.iter().map(|l| Appli {
            id: l.texte("id").unwrap_or_default(), nom: l.texte("nom").unwrap_or_default(), editeur: l.texte("editeur"),
            icone: l.texte("icone"), genre: l.entier("genre"), note: l.nombre("note"), nb_notes: l.entier("nb_notes"),
            version: l.texte("version"), business: l.texte("business"),
        }).collect())
    }

    /// Une position dans un classement. Tous les rangs d'un même passage partagent le même `at`.
    pub fn rang(&self, app: &str, pays: &str, genre: i64, liste: Liste, at: &Date, position: i64) -> Res<()> {
        self.db.execute("insert or replace into rang (app, pays, genre, liste, at, position) values (?, ?, ?, ?, ?, ?)",
            p![app, pays, genre, liste.brut(), at, position])?;
        Ok(())
    }

    // avis

    pub fn avis(&self, a: &Avis) -> Res<()> {
        self.db.execute("
            insert into avis (id, business, pays, note, titre, texte, auteur, at, repondu) values (?, ?, ?, ?, ?, ?, ?, ?, ?)
            on conflict (id) do update set note = excluded.note, titre = excluded.titre, texte = excluded.texte,
                repondu = excluded.repondu",
            p![&a.id, &a.business, &a.pays, a.note, &a.titre, &a.texte, &a.auteur, a.at, a.repondu])?;
        Ok(())
    }

    // à traiter

    /// Remplace les tâches ouvertes d'une source pour un business : les nouvelles entrent,
    /// celles qui ne sont plus renvoyées passent à fait. Une tâche cochée reste faite.
    pub fn synchroniser_taches(&self, source: &str, business: &str, taches: &[Tache]) -> Res<()> {
        self.lot(|| {
            let mut ids = HashSet::new();
            for t in taches {
                let id = format!("{source}:{}", t.cle);
                self.db.execute("
                    insert into a_traiter (id, business, texte, etiquette, ton, echeance, source, lien, cree_le)
                    values (?, ?, ?, ?, ?, ?, ?, ?, ?)
                    on conflict (id) do update set texte = excluded.texte, etiquette = excluded.etiquette,
                        ton = excluded.ton, echeance = excluded.echeance, lien = excluded.lien",
                    p![&id, business, &t.texte, &t.etiquette, t.ton.brut(), &t.echeance, source, &t.lien, jour::maintenant()])?;
                ids.insert(id);
            }
            let ouvertes = self.db.query("select id from a_traiter where source = ? and business = ? and fait_le is null", p![source, business])?;
            for id in ouvertes.iter().filter_map(|l| l.texte("id")).filter(|id| !ids.contains(id)) {
                self.db.execute("update a_traiter set fait_le = ? where id = ?", p![jour::maintenant(), id])?;
            }
            Ok(())
        })
    }

    pub fn ajouter_tache(&self, texte: &str, business: Option<&str>, echeance: Option<&str>) -> Res<()> {
        self.db.execute("insert into a_traiter (id, business, texte, echeance, source, cree_le) values (?, ?, ?, ?, 'manuel', ?)",
            p![format!("manuel:{}", uuid::Uuid::new_v4().to_string().to_uppercase()), business, texte, echeance, jour::maintenant()])?;
        Ok(())
    }

    pub fn cocher(&self, id: &str, fait: bool) -> Res<()> {
        self.db.execute("update a_traiter set fait_le = ? where id = ?", p![fait.then(jour::maintenant), id])?;
        Ok(())
    }

    pub fn supprimer_tache(&self, id: &str) -> Res<()> {
        self.db.execute("delete from a_traiter where id = ? and source = 'manuel'", p![id])?;
        Ok(())
    }

    // SAV

    /// Écrit ce qu'un passage a lu du SAV. Un fil ouvert ou un retour à traiter en base qui n'est plus dans le relevé
    /// a été traité ailleurs (dashboard) : il passe fermé, ou traité. Rend le nombre de lignes écrites.
    pub fn synchroniser_sav(&self, business: &str, r: &ReleveSAV) -> Res<usize> {
        self.lot(|| {
            for f in &r.fils {
                self.db.execute("
                    insert or replace into sav_fil (business, uid, nom, dernier_at, dernier_equipe, apercu, ouvert, non_vu)
                    values (?, ?, ?, ?, ?, ?, ?, ?)",
                    p![business, &f.uid, &f.nom, f.dernier_at, f.dernier_equipe, &f.apercu, f.ouvert, f.non_vu])?;
            }
            let ouverts: HashSet<&str> = r.fils.iter().filter(|f| f.ouvert).map(|f| f.uid.as_str()).collect();
            for l in self.db.query("select uid from sav_fil where business = ? and ouvert = 1", p![business])? {
                let Some(uid) = l.texte("uid") else { continue };
                if ouverts.contains(uid.as_str()) { continue; }
                self.db.execute("update sav_fil set ouvert = 0 where business = ? and uid = ?", p![business, uid])?;
            }
            for x in &r.retours {
                self.db.execute("
                    insert or replace into sav_retour (business, id, uid, source, categorie, etoiles, message, citation,
                        conversation, traite, traite_le, plateforme, build, at)
                    values (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    p![business, &x.id, &x.uid, &x.source, &x.categorie, x.etoiles, &x.message, &x.citation, &x.conversation,
                       x.traite, x.traite_le, &x.plateforme, &x.build, x.at])?;
            }
            let a_traiter: HashSet<&str> = r.retours.iter().filter(|x| !x.traite).map(|x| x.id.as_str()).collect();
            for l in self.db.query("select id from sav_retour where business = ? and traite = 0", p![business])? {
                let Some(id) = l.texte("id") else { continue };
                if a_traiter.contains(id.as_str()) { continue; }
                self.db.execute("update sav_retour set traite = 1 where business = ? and id = ?", p![business, id])?;
            }
            for (uid, messages) in &r.messages {
                self.db.execute("delete from sav_message where business = ? and uid = ?", p![business, uid])?;
                for m in messages { self.message(business, uid, m)?; }
            }
            for (uid, email) in &r.emails {
                self.db.execute("insert or replace into sav_compte (business, uid, email) values (?, ?, ?)", p![business, uid, email])?;
            }
            Ok(r.fils.len() + r.retours.len() + r.messages.values().map(Vec::len).sum::<usize>() + r.emails.len())
        })
    }

    /// Les fils dont les messages sont en base, avec l'instant de leur dernier message tel qu'écrit (ISO) :
    /// un fil dont l'instant a changé est à relire.
    pub fn fils_lus(&self, business: &str) -> Res<HashMap<String, String>> {
        Ok(self.db.query("
            select uid, dernier_at from sav_fil f where business = ?
                and exists (select 1 from sav_message m where m.business = f.business and m.uid = f.uid)", p![business])?
            .iter().filter_map(|l| Some((l.texte("uid")?, l.texte("dernier_at")?))).collect())
    }

    /// Les uid dont l'email est connu ; un invité, sans email, est relu à chaque passage (il a pu lier un compte).
    pub fn comptes_sav(&self, business: &str) -> Res<HashSet<String>> {
        Ok(self.db.query("select uid from sav_compte where business = ? and email != ''", p![business])?
            .iter().filter_map(|l| l.texte("uid")).collect())
    }

    /// La réponse partie de l'écran SAV : le message, le fil passé à l'équipe et fermé, ses tâches faites.
    pub fn repondre_sav(&self, cle: &CleFil, m: &MessageSAV, apercu: &str) -> Res<()> {
        self.lot(|| {
            self.message(&cle.business, &cle.uid, m)?;
            self.db.execute("
                insert into sav_fil (business, uid, dernier_at, dernier_equipe, apercu, ouvert, non_vu) values (?, ?, ?, 1, ?, 0, 1)
                on conflict (business, uid) do update set dernier_at = excluded.dernier_at, dernier_equipe = 1,
                    apercu = excluded.apercu, ouvert = 0, non_vu = 1",
                p![&cle.business, &cle.uid, m.at, apercu])?;
            self.fermer_taches(cle)
        })
    }

    /// Le fil marqué traité sans réponse.
    pub fn fermer_fil_sav(&self, cle: &CleFil) -> Res<()> {
        self.lot(|| {
            self.db.execute("update sav_fil set ouvert = 0 where business = ? and uid = ?", p![&cle.business, &cle.uid])?;
            self.fermer_taches(cle)
        })
    }

    /// Un retour marqué traité, ou rouvert, et sa tâche avec.
    pub fn traiter_retour_sav(&self, business: &str, id: &str, traite: bool, le: &Date) -> Res<()> {
        self.lot(|| {
            self.db.execute("update sav_retour set traite = ?, traite_le = ? where business = ? and id = ?",
                p![traite, traite.then_some(*le), business, id])?;
            self.cocher(&ids_sav::tache_retour(id), traite)
        })
    }

    fn message(&self, business: &str, uid: &str, m: &MessageSAV) -> Res<()> {
        self.db.execute("insert or replace into sav_message (business, uid, id, equipe, texte, at) values (?, ?, ?, ?, ?, ?)",
            p![business, uid, &m.id, m.equipe, &m.texte, m.at])?;
        Ok(())
    }

    fn fermer_taches(&self, cle: &CleFil) -> Res<()> {
        let prefixe = ids_sav::prefixe_tache_fil(&cle.uid);
        self.db.execute("update a_traiter set fait_le = ? where business = ? and fait_le is null and substr(id, 1, ?) = ?",
            p![jour::maintenant(), &cle.business, prefixe.chars().count(), &prefixe])?;
        Ok(())
    }

    // installs

    /// Remplace les installs d'un business à partir de `debut` : un jour recalculé perd ses anciennes lignes.
    pub fn installs(&self, business: &str, debut: &str, lignes: &[Install]) -> Res<()> {
        self.lot(|| {
            self.db.execute("delete from install where business = ? and jour >= ?", p![business, debut])?;
            for i in lignes {
                let valeurs: BTreeMap<&str, &Vec<String>> = i.valeurs.iter().map(|(d, v)| (d.brut(), v)).collect();
                self.db.execute("
                    insert into install (business, jour, loin, paywall, essai, direct, rond, j1, j7, j14, j30, client, valeurs)
                    values (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    p![business, &i.jour, i.loin, i.paywall, i.essai, i.direct, i.rond, i.j1, i.j7, i.j14, i.j30, &i.client,
                       serde_json::to_string(&valeurs)?])?;
            }
            Ok(())
        })
    }

    /// Les clients convertis à demander à RevenueCat, les plus récents d'abord : jamais lus, ou encore en essai
    /// (ou sans paiement), lus avant `relu` et installés depuis `depuis`.
    pub fn clients_a_verifier(&self, business: &str, depuis: &str, relu: &Date, limite: i64) -> Res<Vec<String>> {
        Ok(self.db.query("
            select i.client, max(i.jour) as jour from install i
            left join paiement p on p.business = i.business and p.client = i.client
            where i.business = ? and i.client is not null
                and (p.client is null or (p.etat in ('essai', 'non') and p.lu < ? and i.jour >= ?))
            group by i.client order by jour desc limit ?", p![business, relu, depuis, limite])?
            .iter().filter_map(|l| l.texte("client")).collect())
    }

    pub fn paiement(&self, business: &str, client: &str, etat: EtatPaiement, lu: &Date) -> Res<()> {
        self.db.execute("
            insert into paiement (business, client, etat, lu) values (?, ?, ?, ?)
            on conflict (business, client) do update set etat = excluded.etat, lu = excluded.lu",
            p![business, client, etat.brut(), lu])?;
        Ok(())
    }

    /// Remplace les portes du paywall d'un business à partir de `debut`.
    pub fn portes(&self, business: &str, debut: &str, lignes: &[Porte]) -> Res<()> {
        self.lot(|| {
            self.db.execute("delete from porte where business = ? and jour >= ?", p![business, debut])?;
            for x in lignes {
                self.db.execute("insert into porte (business, jour, porte, vues, essais, directs) values (?, ?, ?, ?, ?, ?)",
                    p![business, &x.jour, &x.porte, x.vues, x.essais, x.directs])?;
            }
            Ok(())
        })
    }

    // plan

    /// Remplace le plan par les lignes de la note, dans leur ordre.
    pub fn plan(&self, lignes: &[LignePlan]) -> Res<()> {
        self.lot(|| {
            self.db.execute("delete from plan", p![])?;
            for (i, l) in lignes.iter().enumerate() {
                self.db.execute("insert into plan (ordre, section, texte, coche, fait) values (?, ?, ?, ?, ?)",
                    p![i, &l.section, &l.texte, l.coche, l.fait])?;
            }
            Ok(())
        })
    }

    // journal de collecte

    pub fn journal(&self, x: &Passage) -> Res<()> {
        self.db.execute("insert into collecte (source, debut, fin, etat, erreur, lignes) values (?, ?, ?, ?, ?, ?)",
            p![&x.source, x.debut, x.fin, x.etat.brut(), &x.erreur, x.lignes])?;
        Ok(())
    }

    pub fn dernier_passage(&self, source: &str) -> Res<Option<Passage>> {
        Ok(self.db.query("select * from collecte where source = ? order by debut desc limit 1", p![source])?
            .first().map(Passage::depuis_ligne))
    }

    pub fn dernier_succes(&self, source: &str) -> Res<Option<Date>> {
        Ok(self.db.query("select max(fin) as fin from collecte where source = ? and etat = 'ok'", p![source])?
            .first().and_then(|l| l.date("fin")))
    }

    // réglages

    pub fn reglage(&self, cle: &str) -> Res<Option<String>> {
        Ok(self.db.query("select valeur from reglage where cle = ?", p![cle])?.first().and_then(|l| l.texte("valeur")))
    }

    pub fn regler(&self, cle: &str, valeur: Option<&str>) -> Res<()> {
        match valeur {
            Some(v) => self.db.execute("insert or replace into reglage (cle, valeur) values (?, ?)", p![cle, v])?,
            None => self.db.execute("delete from reglage where cle = ?", p![cle])?,
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serie_somme_et_taches() {
        let s = Store::memoire().unwrap();
        s.serie("b", "a.app_store", "2026-11-01", -1, 3.0).unwrap();
        s.serie("b", "a.play_store", "2026-11-01", -1, 4.0).unwrap();
        s.somme("b", "a", &["a.app_store".into(), "a.play_store".into()], "2026-11-01").unwrap();
        let l = s.db.query("select valeur from serie where metrique = 'a'", p![]).unwrap();
        assert_eq!(l[0].nombre("valeur"), Some(7.0));
        let t = Tache { cle: "x".into(), texte: "t".into(), ..Default::default() };
        s.synchroniser_taches("src", "b", &[t]).unwrap();
        s.synchroniser_taches("src", "b", &[]).unwrap();
        let l = s.db.query("select fait_le from a_traiter where id = 'src:x'", p![]).unwrap();
        assert!(l[0].texte("fait_le").is_some());
        assert!(s.fil(&Evenement { id: "src:1".into(), business: "b".into(), at: jour::maintenant(), type_: TypeFil::TOUS[0], texte: "x".into(), lien: None }).unwrap());
        assert_eq!(s.prendre_nouveaux("src").len(), 1);
        assert!(s.prendre_nouveaux("src").is_empty());
    }
}
