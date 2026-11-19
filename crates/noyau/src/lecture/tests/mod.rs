//! Les tests des lectures, portés de CockpitTests avec les mêmes données et les mêmes attendus.
use super::Lectures;
use crate::jour::{self, Date};
use crate::modeles::{Plateforme, Video};
use crate::store::Store;

mod accueil;
mod argent;
mod format;
mod lectures;
mod percee;
mod sante;
mod sav;
mod store;
mod videos;
mod widget;

fn d(iso: &str) -> Date { jour::depuis_iso(iso).unwrap() }

/// L'instant des tests : le 13 novembre 2026 à 15 h à Paris.
fn maintenant() -> Date { d("2026-11-13T14:00:00Z") }

fn lectures(s: &Store) -> Lectures<'_> { Lectures { maintenant: maintenant(), ..Lectures::new(&s.db, s.businesses().unwrap()) } }

fn video(id: &str, compte: &str, legende: &str, publiee: Option<&str>) -> Video {
    Video {
        id: id.into(), business: "roleo".into(), compte: compte.into(), legende: Some(legende.into()),
        publiee_le: publiee.map(d), duree_s: None, url: None, plateforme: Plateforme::Tiktok,
    }
}
