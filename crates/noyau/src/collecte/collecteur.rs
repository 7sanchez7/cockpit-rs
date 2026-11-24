//! Lance chaque source à sa cadence, écrit chaque passage dans la table `collecte`, notifie les nouveautés.
//! Les règles (échéance, reprise après échec, notifications) sont dans docs/collecte.md.
use super::gcloud::GCloud;
use super::hote::Hote;
use super::http::Http;
use super::secrets::Secrets;
use super::source::{Contexte, Source};
use crate::erreur::{Erreur, Res};
use crate::jour::{self, Date};
use crate::modeles::{EtatPassage, Evenement, Passage};
use crate::store::Store;
use chrono::Duration;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Une source en erreur ou non configurée (session expirée, clé absente) est retentée au plus tard 10 minutes après :
/// elle repasse au vert dès que la session revient, sans attendre sa cadence.
pub const REPRISE_APRES_ECHEC: i64 = 600;
/// Une nouveauté plus vieille que ça ne notifie pas (retour de veille, rattrapage).
pub const FRAICHEUR_NOTIF: i64 = 2 * 3600;
/// Réglage qui met la collecte en pause quand il vaut « 1 ».
pub const REGLAGE_PAUSE: &str = "collecte.pause";

pub struct Collecteur {
    pub sources: Vec<Arc<dyn Source>>,
    pub store: Arc<Store>,
    pub secrets: Arc<Secrets>,
    pub http: Http,
    pub gcloud: Arc<GCloud>,
    pub hote: Arc<dyn Hote>,
    /// Ids des sources en train de tourner, pour les indicateurs de la barre du haut.
    en_cours: Mutex<HashSet<String>>,
    boucle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Collecteur {
    pub fn new(store: Arc<Store>, sources: Vec<Arc<dyn Source>>, hote: Arc<dyn Hote>) -> Arc<Collecteur> {
        let http = Http::new();
        let gcloud = Arc::new(GCloud::new(http.clone()));
        let secrets = Arc::new(Secrets::new(gcloud.clone(), http.clone()));
        Arc::new(Collecteur { sources, store, secrets, http, gcloud, hote, en_cours: Mutex::new(HashSet::new()), boucle: Mutex::new(None) })
    }

    pub fn en_cours(&self) -> HashSet<String> { self.en_cours.lock().clone() }

    /// Un tour par minute, tant que l'app tourne.
    pub fn demarrer(self: &Arc<Self>) {
        let mut b = self.boucle.lock();
        if b.is_some() {
            return;
        }
        let moi = self.clone();
        *b = Some(tokio::spawn(async move {
            loop {
                moi.tour();
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            }
        }));
    }

    /// Lance les sources arrivées à échéance, sauf si la collecte est en pause.
    pub fn tour(self: &Arc<Self>) {
        if self.store.reglage(REGLAGE_PAUSE).ok().flatten().as_deref() == Some("1") {
            return;
        }
        for s in self.sources.iter().filter(|s| self.a_echeance(s.as_ref(), &jour::maintenant())) {
            self.lancer_en_fond(s.clone());
        }
    }

    /// Relance tout de suite une source, ou toutes (Ctrl R, bouton de Santé).
    pub fn relancer(self: &Arc<Self>, id: Option<&str>) {
        for s in self.sources.iter().filter(|s| id.is_none_or(|i| s.id() == i)) {
            self.lancer_en_fond(s.clone());
        }
    }

    fn lancer_en_fond(self: &Arc<Self>, s: Arc<dyn Source>) {
        let moi = self.clone();
        tokio::spawn(async move { moi.lancer(s.as_ref()).await });
    }

    /// Ce qu'une source ou une action du SAV reçoit pour travailler.
    pub fn contexte(&self) -> Res<Contexte> {
        Ok(Contexte {
            store: self.store.clone(),
            secrets: self.secrets.clone(),
            http: self.http.clone(),
            gcloud: self.gcloud.clone(),
            businesses: self.store.businesses()?,
            hote: self.hote.clone(),
        })
    }

    fn a_echeance(&self, s: &dyn Source, maintenant: &Date) -> bool {
        match self.store.dernier_passage(s.id()) {
            Ok(Some(d)) => a_echeance(&d, s.cadence(), s.reprise(), maintenant),
            _ => true,
        }
    }

    /// Un passage complet : la collecte, son journal, puis les notifications. Rend le passage écrit.
    pub async fn lancer(self: &Arc<Self>, s: &dyn Source) -> Option<Passage> {
        let _r = self.reserver(s.id())?;
        Some(self.passage(s).await)
    }

    /// Marque `id` en cours jusqu'à ce que la réservation tombe ; None s'il l'est déjà.
    fn reserver(self: &Arc<Self>, id: &str) -> Option<Reservation> {
        if !self.en_cours.lock().insert(id.to_string()) {
            return None;
        }
        self.store.a_change();
        Some(Reservation { collecteur: self.clone(), id: id.into() })
    }

    async fn passage(&self, s: &dyn Source) -> Passage {
        let premier = !matches!(self.store.dernier_succes(s.id()), Ok(Some(_)));
        let avant = self.store.dernier_passage(s.id()).ok().flatten();
        let mut p = Passage { source: s.id().into(), debut: jour::maintenant(), fin: jour::maintenant(), etat: EtatPassage::Ok, erreur: None, lignes: 0 };
        let r = match self.contexte() {
            Ok(ctx) => s.collecte(&ctx).await,
            Err(e) => Err(e),
        };
        match r {
            Ok(n) => p.lignes = n as i64,
            Err(Erreur::NonConfigure(m)) => {
                p.etat = EtatPassage::NonConfigure;
                p.erreur = Some(m);
            }
            Err(e) => {
                p.etat = EtatPassage::Erreur;
                p.erreur = Some(e.to_string());
            }
        }
        p.fin = jour::maintenant();
        if let Err(e) = self.store.journal(&p) {
            eprintln!("cockpit : journal de {} : {e}", s.id());
        }
        match p.etat {
            EtatPassage::Ok => eprintln!("cockpit : {} : {} lignes en {:.1} s", s.id(), p.lignes, (p.fin - p.debut).num_milliseconds() as f64 / 1000.0),
            _ => eprintln!("cockpit : {} : {}", s.id(), p.erreur.as_deref().unwrap_or_default()),
        }
        let nouveaux = self.store.prendre_nouveaux(s.id());
        if !premier {
            self.notifier(&nouveaux);
        }
        if p.etat == EtatPassage::Erreur && avant.is_some_and(|a| a.etat == EtatPassage::Ok) {
            self.hote.notifier(&format!("Cockpit · {} en panne", s.nom()), p.erreur.as_deref().unwrap_or_default(), &format!("panne:{}:{}", s.id(), jour::iso(&p.debut)));
        }
        p
    }

    /// Une notification par nouveauté récente dont le type est activé dans Réglages ; au-delà de 3, un résumé.
    fn notifier(&self, evenements: &[Evenement]) {
        let noms: HashMap<String, String> = self.store.businesses().unwrap_or_default().into_iter().map(|b| (b.id, b.nom)).collect();
        let coupes: HashSet<String> = evenements
            .iter()
            .map(|e| e.type_.reglage_notif())
            .filter(|r| self.store.reglage(r).ok().flatten().as_deref() == Some("0"))
            .collect();
        let a_envoyer = a_notifier(evenements, &coupes, &jour::maintenant());
        for e in a_envoyer.iter().take(3) {
            let nom = noms.get(&e.business).unwrap_or(&e.business);
            self.hote.notifier(&format!("{nom} · {}", e.type_.libelle()), &e.texte, &e.id);
        }
        if a_envoyer.len() > 3 {
            self.hote.notifier("Cockpit", &format!("{} autres nouveautés", a_envoyer.len() - 3), &format!("resume:{}", jour::iso(&jour::maintenant())));
        }
    }
}

/// Une source marquée en cours ; elle cesse de l'être quand la réservation tombe.
struct Reservation {
    collecteur: Arc<Collecteur>,
    id: String,
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.collecteur.en_cours.lock().remove(&self.id);
        self.collecteur.store.a_change();
    }
}

/// Une source est due quand sa cadence (ou son délai de reprise après un échec, s'il est plus court) est écoulée.
pub fn a_echeance(dernier: &Passage, cadence: i64, reprise: i64, maintenant: &Date) -> bool {
    let delai = if dernier.etat == EtatPassage::Ok { cadence } else { cadence.min(reprise) };
    dernier.debut + Duration::seconds(delai) <= *maintenant
}

/// Les nouveautés de moins de 2 h dont le réglage de notification n'est pas coupé.
pub fn a_notifier<'a>(evenements: &'a [Evenement], coupes: &HashSet<String>, maintenant: &Date) -> Vec<&'a Evenement> {
    evenements.iter().filter(|e| e.at > *maintenant - Duration::seconds(FRAICHEUR_NOTIF) && !coupes.contains(&e.type_.reglage_notif())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modeles::TypeFil;

    fn passage(etat: EtatPassage, il_y_a: i64) -> Passage {
        let d = jour::maintenant() - Duration::seconds(il_y_a);
        Passage { source: "x".into(), debut: d, fin: d, etat, erreur: None, lignes: 0 }
    }

    #[test]
    fn echeance() {
        let m = jour::maintenant();
        assert!(!a_echeance(&passage(EtatPassage::Ok, 700), 3600, 600, &m));
        assert!(a_echeance(&passage(EtatPassage::Erreur, 700), 3600, 600, &m));
        assert!(a_echeance(&passage(EtatPassage::Ok, 3601), 3600, 600, &m));
        assert!(!a_echeance(&passage(EtatPassage::NonConfigure, 100), 300, 600, &m));
    }

    #[test]
    fn fraicheur_et_reglages() {
        let m = jour::maintenant();
        let e = |id: &str, t: TypeFil, h: i64| Evenement { id: id.into(), business: "b".into(), at: m - Duration::seconds(h), type_: t, texte: String::new(), lien: None };
        let ev = vec![e("1", TypeFil::Achat, 60), e("2", TypeFil::Achat, 3 * 3600), e("3", TypeFil::Avis, 60)];
        let coupes = HashSet::from([TypeFil::Avis.reglage_notif()]);
        let ids: Vec<&str> = a_notifier(&ev, &coupes, &m).iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["1"]);
    }
}
