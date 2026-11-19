//! Les écritures de la veille (docs/donnees.md § Veille).
use crate::db::Valeur;
use crate::erreur::Res;
use crate::jour::{self, Date};
use crate::modeles::Plateforme;
use crate::modeles_veille::*;
use crate::p;
use crate::store::Store;
use chrono::Duration;

impl Store {
    // mots-clés

    pub fn mots_veille(&self) -> Res<Vec<MotVeille>> {
        Ok(self.db.query("select * from veille_mot order by business, ajoute", p![])?.iter().filter_map(|l| Some(MotVeille {
            business: l.texte("business")?, mot: l.texte("mot")?, vues_min: l.entier("vues_min").unwrap_or(20_000),
            actif: l.booleen("actif"), cherche: l.date("cherche"),
        })).collect())
    }

    /// Ajoute un mot-clé, ou change le seuil d'un mot déjà là et le remet en route.
    pub fn ajouter_mot(&self, business: &str, mot: &str, vues_min: i64) -> Res<()> {
        let mot = mot.trim();
        if mot.is_empty() { return Ok(()); }
        self.db.execute("
            insert into veille_mot (business, mot, vues_min, actif, ajoute) values (?, ?, ?, 1, ?)
            on conflict (business, mot) do update set vues_min = excluded.vues_min, actif = 1",
            p![business, mot, vues_min, jour::maintenant()])?;
        Ok(())
    }

    /// Le mot n'est plus cherché ; ses vidéos restent.
    pub fn retirer_mot(&self, business: &str, mot: &str) -> Res<()> {
        self.db.execute("delete from veille_mot where business = ? and mot = ?", p![business, mot.trim()])?;
        Ok(())
    }

    pub fn activer_mot(&self, business: &str, mot: &str, actif: bool) -> Res<()> {
        self.db.execute("update veille_mot set actif = ? where business = ? and mot = ?", p![actif, business, mot])?;
        Ok(())
    }

    pub fn cherche(&self, business: &str, mot: &str, at: &Date) -> Res<()> {
        self.db.execute("update veille_mot set cherche = ? where business = ? and mot = ?", p![at, business, mot])?;
        Ok(())
    }

    // vidéos

    /// Écrit une vidéo trouvée par `mot`, son relevé du moment et son compte. Le premier mot qui l'a trouvée reste.
    pub fn trouvaille(&self, t: &Trouvaille, business: &str, mot: Option<&str>, at: &Date) -> Res<()> {
        self.db.execute("
            insert into veille_video (id, business, plateforme, compte, url, legende, publiee, duree_s, mot, media, trouvee)
            values (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            on conflict (id) do update set legende = coalesce(excluded.legende, veille_video.legende),
                publiee = coalesce(excluded.publiee, veille_video.publiee),
                duree_s = coalesce(excluded.duree_s, veille_video.duree_s),
                media = coalesce(excluded.media, veille_video.media),
                mot = coalesce(veille_video.mot, excluded.mot)",
            p![t.id(), business, t.plateforme.brut(), &t.compte, t.url(), &t.legende, t.publiee, t.duree_s, mot, &t.media, at])?;
        self.db.execute("
            insert into veille_compte (id, plateforme, handle, abonnes, decouvert) values (?, ?, ?, ?, ?)
            on conflict (id) do update set abonnes = coalesce(excluded.abonnes, veille_compte.abonnes)",
            p![format!("{}:{}", t.plateforme.brut(), t.compte), t.plateforme.brut(), &t.compte, t.abonnes, at])?;
        self.relever(t, at)
    }

    /// Le relevé d'une vidéo déjà dans la veille (vue de nouveau sur son profil) ; rien sinon.
    pub fn relever(&self, t: &Trouvaille, at: &Date) -> Res<()> {
        if t.vues.is_none() && t.likes.is_none() { return Ok(()); }
        let id = t.id();
        self.db.execute("
            insert or replace into veille_releve (video, at, vues, likes, commentaires, partages, enregistrements)
            select ?, ?, ?, ?, ?, ?, ? where exists (select 1 from veille_video where id = ?)",
            p![&id, at, t.vues, t.likes, t.commentaires, t.partages, t.enregistrements, &id])?;
        Ok(())
    }

    /// Les comptes dont la médiane manque ou date de plus de 7 jours, ceux des plus grosses vidéos d'abord.
    pub fn comptes_a_baser(&self, plateforme: Plateforme, limite: i64, maintenant: &Date) -> Res<Vec<String>> {
        Ok(self.db.query("
            select c.handle,
                (select max(r.vues) from veille_video v join veille_releve r on r.video = v.id
                 where v.plateforme = c.plateforme and v.compte = c.handle) as top
            from veille_compte c where c.plateforme = ? and (c.base_at is null or c.base_at < ?)
            order by top desc limit ?", p![plateforme.brut(), *maintenant - Duration::days(7), limite])?
            .iter().filter_map(|l| l.texte("handle")).collect())
    }

    /// La médiane d'un compte, relue sur son profil ; `mediane` None = trop peu de vidéos, on ne retente qu'après 7 jours.
    pub fn base(&self, plateforme: Plateforme, handle: &str, mediane: Option<i64>, videos: i64, abonnes: Option<i64>, at: &Date) -> Res<()> {
        self.db.execute("update veille_compte set mediane = ?, nb_base = ?, base_at = ?, abonnes = coalesce(?, abonnes) where id = ?",
            p![mediane, videos, at, abonnes, format!("{}:{handle}", plateforme.brut())])?;
        Ok(())
    }

    pub fn videos_veille(&self, suite: &str, params: Vec<Valeur>) -> Res<Vec<VideoVeille>> {
        Ok(self.db.query(&format!("{} {suite}", VideoVeille::SQL), params)?.iter().filter_map(VideoVeille::depuis_ligne).collect())
    }

    pub fn analyse_veille(&self, video: &str, at: &Date, modele: &str, a: &AnalyseVeille) -> Res<()> {
        self.db.execute("insert or replace into veille_analyse (video, at, modele, donnees) values (?, ?, ?, ?)",
            p![video, at, modele, serde_json::to_string(a)?])?;
        Ok(())
    }

    /// Un appel à Claude facturé (analyse d'une vidéo, ou `demande:<id>`), écrit même quand sa réponse est illisible.
    pub fn cout_veille(&self, video: &str, at: &Date, modele: &str, entree: i64, sortie: i64, usd: f64) -> Res<()> {
        self.db.execute("insert into veille_cout (video, at, modele, entree, sortie, usd) values (?, ?, ?, ?, ?, ?)",
            p![video, at, modele, entree, sortie, usd])?;
        Ok(())
    }

    // demandes à Claude

    /// Une demande qui commence ; celles restées en cours (app quittée en route) passent en échec.
    pub fn nouvelle_demande(&self, d: &DemandeVeille) -> Res<()> {
        self.db.execute("update veille_demande set etat = ?, fin = ?, erreur = 'interrompue' where etat = ?",
            p![DemandeVeille::ECHOUEE, d.debut, DemandeVeille::EN_COURS])?;
        self.db.execute("insert into veille_demande (id, business, prompt, debut, etat) values (?, ?, ?, ?, ?)",
            p![&d.id, &d.business, &d.prompt, d.debut, &d.etat])?;
        Ok(())
    }

    pub fn demande_veille(&self, id: &str) -> Res<Option<DemandeVeille>> {
        Ok(self.db.query("select * from veille_demande where id = ?", p![id])?.first().and_then(DemandeVeille::depuis_ligne))
    }

    /// Une ligne de plus au journal d'une demande.
    pub fn noter_demande(&self, id: &str, ligne: &str) -> Res<()> {
        self.db.execute("update veille_demande set journal = json_insert(journal, '$[#]', ?) where id = ?", p![ligne, id])?;
        Ok(())
    }

    /// La fin d'une demande : sa synthèse, ou son erreur.
    pub fn finir_demande(&self, id: &str, fin: &Date, r: Result<&str, &str>) -> Res<()> {
        let (etat, synthese, erreur) = match r {
            Ok(s) => (DemandeVeille::FINIE, Some(s), None),
            Err(e) => (DemandeVeille::ECHOUEE, None, Some(e)),
        };
        self.db.execute("update veille_demande set fin = ?, etat = ?, synthese = ?, erreur = ? where id = ?",
            p![fin, etat, synthese, erreur, id])?;
        Ok(())
    }

    /// Une vidéo gardée pour une demande, à la suite des autres ; la garder de nouveau change sa raison.
    pub fn retenir_video(&self, demande: &str, video: &str, raison: &str) -> Res<()> {
        self.db.execute("
            insert into veille_retenue (demande, video, raison, ordre)
            values (?, ?, ?, (select count(*) from veille_retenue where demande = ?))
            on conflict (demande, video) do update set raison = excluded.raison",
            p![demande, video, raison, demande])?;
        Ok(())
    }

    /// Le total dépensé en appels à Claude, en dollars.
    pub fn depense_veille_usd(&self) -> Res<f64> {
        Ok(self.db.query("select coalesce(sum(usd), 0) as t from veille_cout", p![])?.first().and_then(|l| l.nombre("t")).unwrap_or(0.0))
    }
}
