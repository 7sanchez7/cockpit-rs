//! Les fournisseurs de la veille : LurkAPI d'abord, TikHub en repli, sinon les anciens chemins
//! (page cachée, DuckDuckGo, yt-dlp). Voir docs/sources/veille.md § Les fournisseurs.
use super::http::Http;
use super::source::Contexte;
use super::{lurkapi, tikhub};
use crate::erreur::{Erreur, Res};
use crate::modeles::Plateforme;
use serde_json::Value;

/// Les clés des fournisseurs, résolues une fois par passage.
pub struct Fournisseurs {
    lurk: Option<String>,
    tikhub: Option<String>,
}

impl Fournisseurs {
    pub async fn resoudre(ctx: &Contexte) -> Fournisseurs {
        Fournisseurs { lurk: lurkapi::cle(ctx).await, tikhub: tikhub::cle(ctx).await }
    }

    /// Vrai si au moins un fournisseur répond : la veille n'ouvre alors ni page cachée ni session.
    pub fn configure(&self) -> bool { self.lurk.is_some() || self.tikhub.is_some() }

    /// Une recherche, LurkAPI puis TikHub en repli ; rend les pages JSON à lire par `parseurs`.
    pub async fn chercher(&self, http: &Http, p: Plateforme, mot: &str, voulues: i64) -> Res<Vec<Value>> {
        let mut premiere: Option<Erreur> = None;
        if let Some(cle) = &self.lurk {
            match lurkapi::chercher(http, cle, p, mot, voulues).await {
                Ok(v) => return Ok(v),
                Err(e) => premiere = Some(e),
            }
        }
        if let Some(cle) = &self.tikhub {
            match tikhub::chercher(http, cle, p, mot, voulues).await {
                Ok(v) => return Ok(v),
                Err(e) => { if premiere.is_none() { premiere = Some(e); } }
            }
        }
        Err(premiere.unwrap_or_else(|| Erreur::NonConfigure("aucun fournisseur de veille configuré".into())))
    }

    /// Les dernières vidéos d'un compte, même repli que `chercher`.
    pub async fn dernieres(&self, http: &Http, p: Plateforme, handle: &str, n: usize) -> Res<Vec<Value>> {
        let mut premiere: Option<Erreur> = None;
        if let Some(cle) = &self.lurk {
            match lurkapi::dernieres(http, cle, p, handle, n).await {
                Ok(v) => return Ok(v),
                Err(e) => premiere = Some(e),
            }
        }
        if let Some(cle) = &self.tikhub {
            match tikhub::dernieres(http, cle, p, handle, n).await {
                Ok(v) => return Ok(v),
                Err(e) => { if premiere.is_none() { premiere = Some(e); } }
            }
        }
        Err(premiere.unwrap_or_else(|| Erreur::NonConfigure("aucun fournisseur de veille configuré".into())))
    }
}
