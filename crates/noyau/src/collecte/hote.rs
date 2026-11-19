//! Ce que la collecte demande à l'app qui la porte : les notifications, les sessions des sites et un navigateur caché.
//! L'app Tauri l'implémente avec ses WebViews ; la CLI se contente de `SansInterface`.
use crate::erreur::{Erreur, Res};
use async_trait::async_trait;
use std::collections::HashMap;

#[async_trait]
pub trait Hote: Send + Sync {
    /// Une notification du système. Sans interface, elle ne part pas.
    fn notifier(&self, titre: &str, corps: &str, id: &str);

    /// Les cookies de session d'un site (`tiktok.com`), par nom. Sur Mac, ceux de Safari ; l'app ajoute ceux de sa
    /// propre fenêtre de connexion (Windows). Rend `NonConfigure` sans session portant le cookie `requis`.
    fn cookies(&self, domaine: &str, requis: &str) -> Res<HashMap<String, String>> { super::safari::cookies(domaine, requis) }

    /// D'où vient la session, sans aucune valeur : pour les messages d'erreur.
    fn provenance(&self, domaine: &str, requis: &str) -> String { super::safari::provenance(domaine, requis) }

    /// Une page web cachée qui garde le JSON reçu par les URL contenant un des `filtres`, avec ces cookies posés sur
    /// `domaine` (`.tiktok.com`). Voir docs/sources/veille.md § La technique.
    async fn navigateur(&self, _filtres: &[String], _cookies: &HashMap<String, String>, _domaine: &str) -> Res<Box<dyn Navigateur>> {
        Err(Erreur::NonConfigure("le navigateur caché n'existe que dans l'app".into()))
    }
}

/// Une page web ouverte hors de l'écran. La page signe ses propres appels (X-Bogus chez TikTok, `doc_id` chez
/// Instagram) : on ne les imite pas, on les écoute. Un captcha n'est jamais résolu.
#[async_trait]
pub trait Navigateur: Send + Sync {
    /// Ouvre `url`, attend la fin du chargement puis `attente` secondes, défile `defilements` fois (une toutes les
    /// 3 s). Rend le texte de chaque réponse gardée depuis l'ouverture, plus les `<script type="application/json">`
    /// qui contiennent `play_count`, `playCount` ou `like_count`.
    async fn lire(&self, url: &str, attente: f64, defilements: u32) -> Res<Vec<String>>;
    /// Le titre et le chemin de la page, pour dire où elle a atterri (connexion, captcha).
    async fn ou(&self) -> String;
    /// Ferme la page ; à appeler une fois lue.
    async fn fermer(&self);
}

/// L'hôte de la CLI et des tests : pas de notification, pas de navigateur caché.
pub struct SansInterface;

impl Hote for SansInterface {
    fn notifier(&self, _: &str, _: &str, _: &str) {}
}

/// Le script injecté au démarrage de chaque page du navigateur caché : chaque réponse de `fetch` ou
/// `XMLHttpRequest` dont l'URL contient un filtre part vers `envoyer(texte)`. `__FILTRES__` reçoit un tableau JSON,
/// `__ENVOI__` l'expression qui transmet un texte à l'app.
pub const SCRIPT_ECOUTE: &str = r#"(() => {
  const F = __FILTRES__;
  const garde = (u) => { try { const s = String(u && u.url ? u.url : u); return F.some(f => s.includes(f)); } catch (e) { return false; } };
  const envoie = (t) => { try { __ENVOI__(String(t)); } catch (e) {} };
  const f0 = window.fetch;
  window.fetch = async function (...a) {
    const r = await f0.apply(this, a);
    if (garde(a[0])) { r.clone().text().then(envoie).catch(() => {}); }
    return r;
  };
  const o = XMLHttpRequest.prototype.open, s = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.open = function (m, u, ...r) { this.__veille = u; return o.call(this, m, u, ...r); };
  XMLHttpRequest.prototype.send = function (...a) {
    if (garde(this.__veille)) {
      this.addEventListener('load', () => {
        try { envoie(this.responseType === '' || this.responseType === 'text' ? this.responseText : JSON.stringify(this.response)); } catch (e) {}
      });
    }
    return s.apply(this, a);
  };
})();"#;

/// Les JSON que la page porte elle-même (Instagram y met ses premiers résultats).
pub const SCRIPT_BALISES: &str = r#"Array.from(document.querySelectorAll('script[type="application/json"]')).map(s => s.textContent || '')
  .filter(t => t.includes('play_count') || t.includes('playCount') || t.includes('like_count'))"#;
