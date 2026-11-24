//! L'hôte de la collecte dans l'app : notifications du système, sessions des sites et navigateur caché.
//! Les sessions viennent de Safari sur Mac, sinon de la fenêtre « Sessions » de l'app, dont les WebViews partagent
//! le même magasin de cookies. Aucune valeur de cookie n'est affichée, écrite ou mise dans une erreur.
use async_trait::async_trait;
use noyau::collecte::hote::{Hote, Navigateur, SCRIPT_BALISES, SCRIPT_ECOUTE};
use noyau::collecte::http::{UA_NAVIGATEUR, UA_SAFARI};
use noyau::collecte::safari;
use noyau::{Erreur, Res};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::webview::{Cookie, PageLoadEvent};
use tauri::{AppHandle, Manager, Webview, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::Notify;

/// Le préfixe des fenêtres du navigateur caché ; la capacité `veille` ne vaut que pour elles.
pub const PREFIXE: &str = "veille-";
/// Hors de l'écran : une fenêtre cachée laisse le système suspendre le JS, et le défilement infini ne charge rien.
const HORS_ECRAN: f64 = -30_000.0;
/// L'agent du moteur de la WebView : WebKit sur Mac, Chromium (WebView2) ailleurs.
pub const AGENT: &str = if cfg!(target_os = "macos") { UA_SAFARI } else { UA_NAVIGATEUR };

pub struct HoteApp {
    pub app: AppHandle,
    pub pages: Pages,
}

/// Les pages cachées ouvertes, par libellé de fenêtre, pour leur rendre ce que leur script envoie.
#[derive(Default, Clone)]
pub struct Pages(Arc<Mutex<HashMap<String, Arc<Tampon>>>>);

#[derive(Default)]
pub struct Tampon {
    corps: Mutex<Vec<String>>,
    balises: Mutex<Option<Vec<String>>>,
    ou: Mutex<Option<String>>,
    charge: Notify,
    recu: Notify,
}

impl Pages {
    /// Un message du script d'une page cachée (commande `plugin:veille|message`).
    pub fn recevoir(&self, page: &str, cle: &str, texte: String) {
        let Some(t) = self.0.lock().get(page).cloned() else { return };
        match cle {
            "corps" if !texte.is_empty() => t.corps.lock().push(texte),
            "balises" => {
                *t.balises.lock() = Some(serde_json::from_str(&texte).unwrap_or_default());
                t.recu.notify_one();
            }
            "ou" => {
                *t.ou.lock() = Some(texte);
                t.recu.notify_one();
            }
            _ => {}
        }
    }
}

impl HoteApp {
    /// Une WebView de l'app, qui donne accès au magasin de cookies commun.
    fn vue(&self) -> Option<Webview> { self.app.webviews().into_values().next() }

    fn cookies_app(&self, domaine: &str, requis: &str) -> Res<HashMap<String, String>> {
        let vue = self.vue().ok_or_else(|| Erreur::NonConfigure("aucune fenêtre ouverte pour lire les sessions".into()))?;
        let url = format!("https://www.{domaine}/").parse().map_err(|e| Erreur::Commande(format!("{domaine} : {e}")))?;
        let cookies = vue.cookies_for_url(url).map_err(|e| Erreur::Commande(format!("cookies de {domaine} : {e}")))?;
        let jar: HashMap<String, String> = cookies.iter().map(|c| (c.name().to_string(), c.value().to_string())).collect();
        if jar.get(requis).is_none_or(|v| v.is_empty()) {
            return Err(Erreur::NonConfigure(format!("pas de session : se connecter à {domaine} dans Réglages, Sessions")));
        }
        Ok(jar)
    }
}

#[async_trait]
impl Hote for HoteApp {
    fn notifier(&self, titre: &str, corps: &str, _id: &str) {
        if let Err(e) = self.app.notification().builder().title(titre).body(corps).show() {
            eprintln!("notification : {e}");
        }
    }

    /// Safari d'abord sur Mac (la session de tous les jours), puis la fenêtre Sessions de l'app.
    fn cookies(&self, domaine: &str, requis: &str) -> Res<HashMap<String, String>> {
        match safari::cookies(domaine, requis) {
            Ok(c) => Ok(c),
            Err(safari) => self.cookies_app(domaine, requis).map_err(|app| if cfg!(target_os = "macos") { safari } else { app }),
        }
    }

    fn provenance(&self, domaine: &str, requis: &str) -> String {
        let app = match self.cookies_app(domaine, requis) {
            Ok(c) => format!("app : {} cookies", c.len()),
            Err(_) => "app : pas de session".into(),
        };
        format!("{} ; {app}", safari::provenance(domaine, requis))
    }

    async fn navigateur(&self, filtres: &[String], cookies: &HashMap<String, String>, domaine: &str) -> Res<Box<dyn Navigateur>> {
        static NUMERO: AtomicU64 = AtomicU64::new(0);
        let libelle = format!("{PREFIXE}{}", NUMERO.fetch_add(1, Ordering::SeqCst));
        let tampon = Arc::new(Tampon::default());
        self.pages.0.lock().insert(libelle.clone(), tampon.clone());
        let filtres = serde_json::to_string(filtres)?;
        let envoi = format!("((t) => window.__TAURI_INTERNALS__.invoke('plugin:veille|message', {{ page: '{libelle}', cle: 'corps', texte: t }}))");
        let script = SCRIPT_ECOUTE.replace("__FILTRES__", &filtres).replace("__ENVOI__", &envoi);
        let charge = tampon.clone();
        let app = self.app.clone();
        let (l, s) = (libelle.clone(), script);
        // une fenêtre se crée sur le fil principal : on la demande et on attend
        let fenetre = tokio::task::spawn_blocking(move || {
            WebviewWindowBuilder::new(&app, &l, WebviewUrl::External("about:blank".parse().expect("about:blank")))
                .title("Veille")
                .inner_size(1_280.0, 1_800.0)
                .position(HORS_ECRAN, HORS_ECRAN)
                .decorations(false)
                .skip_taskbar(true)
                .focused(false)
                .user_agent(AGENT)
                .initialization_script(&s)
                .on_page_load(move |_, p| {
                    if p.event() == PageLoadEvent::Finished && p.url().scheme() != "about" {
                        charge.charge.notify_one();
                    }
                })
                .build()
        })
        .await
        .map_err(|e| Erreur::Commande(format!("page cachée : {e}")))?
        .map_err(|e| Erreur::Commande(format!("page cachée : {e}")))?;
        for (nom, valeur) in cookies {
            let c = Cookie::build((nom.clone(), valeur.clone())).domain(domaine.to_string()).path("/").secure(true).build();
            fenetre.set_cookie(c).map_err(|e| Erreur::Commande(format!("session de la page : {e}")))?;
        }
        let noms = cookies.keys().cloned().collect();
        Ok(Box::new(Page { fenetre, tampon, libelle, noms, pages: self.pages.clone() }))
    }
}

struct Page {
    fenetre: WebviewWindow,
    tampon: Arc<Tampon>,
    libelle: String,
    /// Les noms des cookies de session posés, pour dire si la page les garde.
    noms: Vec<String>,
    pages: Pages,
}

impl Page {
    fn executer(&self, js: &str) -> Res<()> { self.fenetre.eval(js).map_err(|e| Erreur::Commande(format!("page : {e}"))) }

    /// Combien des cookies de session posés la page porte encore : un site qui refuse la session les efface.
    fn session(&self) -> String {
        let gardes = self.fenetre.url().ok().and_then(|u| self.fenetre.cookies_for_url(u).ok()).map(|c| {
            self.noms.iter().filter(|n| c.iter().any(|x| x.name() == n.as_str())).count()
        });
        gardes.map(|g| format!(", {g}/{} cookies de session", self.noms.len())).unwrap_or_default()
    }

    fn renvoyer(&self, cle: &str, expression: &str) -> String {
        format!(
            "window.__TAURI_INTERNALS__.invoke('plugin:veille|message', {{ page: '{}', cle: '{cle}', texte: String({expression}) }})",
            self.libelle
        )
    }
}

#[async_trait]
impl Navigateur for Page {
    async fn lire(&self, url: &str, attente: f64, defilements: u32) -> Res<Vec<String>> {
        self.tampon.corps.lock().clear();
        *self.tampon.balises.lock() = None;
        let cible = url.parse().map_err(|e| Erreur::Commande(format!("{url} : {e}")))?;
        self.fenetre.navigate(cible).map_err(|e| Erreur::Commande(format!("page : {e}")))?;
        tokio::time::timeout(Duration::from_secs(45), self.tampon.charge.notified())
            .await
            .map_err(|_| Erreur::Commande("page : pas chargée en 45 s".into()))?;
        tokio::time::sleep(Duration::from_secs_f64(attente)).await;
        for _ in 0..defilements {
            self.executer("window.scrollBy(0, document.body.scrollHeight)")?;
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
        let attendu = self.tampon.recu.notified();
        self.executer(&self.renvoyer("balises", &format!("JSON.stringify({SCRIPT_BALISES})")))?;
        // une page qui ne répond plus rend ce qu'elle a déjà envoyé
        let _ = tokio::time::timeout(Duration::from_secs(5), attendu).await;
        let mut tout = std::mem::take(&mut *self.tampon.corps.lock());
        tout.extend(self.tampon.balises.lock().take().unwrap_or_default());
        Ok(tout)
    }

    async fn ou(&self) -> String {
        *self.tampon.ou.lock() = None;
        let attendu = self.tampon.recu.notified();
        let lieu = "[document.title, location.pathname, document.querySelector('.captcha-verify-container') && 'captcha'].filter(Boolean).join(', ')";
        if self.executer(&self.renvoyer("ou", lieu)).is_err() {
            return String::new();
        }
        let _ = tokio::time::timeout(Duration::from_secs(3), attendu).await;
        let lieu = self.tampon.ou.lock().take().unwrap_or_default();
        lieu + &self.session()
    }

    async fn fermer(&self) {
        self.pages.0.lock().remove(&self.libelle);
        if let Err(e) = self.fenetre.destroy() {
            eprintln!("page cachée : {e}");
        }
    }
}
