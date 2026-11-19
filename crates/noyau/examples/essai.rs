//! `cargo run -p noyau --example essai -- <Source>` : une collecte contre les vraies API sur `.cache/essai.sqlite`,
//! avec la graine de la machine, puis ce qu'elle a écrit. Sans interface : ni notification, ni navigateur caché.
use noyau::collecte::collecteur::Collecteur;
use noyau::collecte::hote::SansInterface;
use noyau::db::Valeur;
use noyau::store::Store;
use noyau::{chemins, jour, p};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

#[tokio::main]
async fn main() {
    if let Err(e) = essayer().await {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

async fn essayer() -> Result<(), Box<dyn std::error::Error>> {
    let sources = noyau::sources::toutes();
    let noms: Vec<&str> = sources.iter().map(|s| s.nom()).collect();
    let voulu = std::env::args().nth(1).ok_or_else(|| format!("quelle source ? {}", noms.join(", ")))?;
    let source = sources
        .iter()
        .find(|s| s.nom().eq_ignore_ascii_case(&voulu) || s.id().eq_ignore_ascii_case(&voulu))
        .ok_or_else(|| format!("source inconnue : {voulu} ({})", noms.join(", ")))?
        .clone();
    std::fs::create_dir_all(".cache")?;
    let store = Arc::new(Store::ouvrir_a(Path::new(".cache/essai.sqlite"), Some(&chemins::graine()))?);
    let ctx = Collecteur::new(store.clone(), sources.clone(), Arc::new(SansInterface)).contexte()?;
    let depuis = jour::maintenant();
    let debut = Instant::now();
    let n = source.collecte(&ctx).await.map_err(|e| format!("{} a échoué : {e}", source.nom()))?;
    println!("{} : {n} lignes en {:.1} s\n", source.nom(), debut.elapsed().as_secs_f64());

    let montrer = |titre: &str, sql: &str, args: Vec<Valeur>| -> Result<(), Box<dyn std::error::Error>> {
        let lignes = store.db.query(sql, args)?;
        if lignes.is_empty() {
            return Ok(());
        }
        println!("— {titre} ({})", lignes.len());
        for l in &lignes {
            let mut c: Vec<_> = l.colonnes.iter().collect();
            c.sort_by(|a, b| a.0.cmp(b.0));
            let texte: Vec<String> = c.iter().map(|(k, v)| format!("{k}={}", afficher(v))).collect();
            println!("  {}", texte.join("  "));
        }
        println!();
        Ok(())
    };
    montrer(
        "serie",
        "select business, metrique, jour, heure, valeur from serie where maj >= ? order by metrique, jour desc, heure limit 60",
        p![&depuis],
    )?;
    montrer("fil", "select at, type, texte from fil where id like ? order by at desc limit 15", p![format!("{}:%", source.id())])?;
    montrer(
        "a_traiter",
        "select id, texte, etiquette, ton, fait_le from a_traiter where source = ? order by cree_le desc limit 15",
        p![source.id()],
    )?;
    for table in ["video", "video_releve", "app", "rang", "avis", "install", "paiement", "porte", "sav_fil", "sav_message", "sav_retour", "sav_compte"] {
        let n = store.db.query(&format!("select count(*) as n from {table}"), p![])?.first().and_then(|l| l.entier("n")).unwrap_or(0);
        if n > 0 {
            println!("{table} : {n} lignes au total");
        }
    }
    Ok(())
}

fn afficher(v: &Valeur) -> String {
    match v {
        Valeur::Null => "nil".into(),
        Valeur::Int(i) => i.to_string(),
        Valeur::Reel(r) => r.to_string(),
        Valeur::Texte(t) => t.clone(),
        Valeur::Blob(b) => format!("<{} octets>", b.len()),
    }
}
