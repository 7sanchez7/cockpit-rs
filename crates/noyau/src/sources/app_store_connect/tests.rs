//! Réponses figées de l'API App Store Connect (ids inventés) et forme du JWT.
use super::*;
use crate::essai;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use ring::rand::SystemRandom;
use ring::signature::{EcdsaKeyPair, KeyPair, UnparsedPublicKey, ECDSA_P256_SHA256_FIXED, ECDSA_P256_SHA256_FIXED_SIGNING};
use serde_json::Value;

const AVIS: &str = r#"
    {"data":[
      {"type":"customerReviews","id":"r-top","attributes":{"rating":5,"title":"Top","body":"Super app",
        "reviewerNickname":"Lea","createdDate":"2026-11-11T00:18:14-08:00","territory":"FRA"},
        "relationships":{"response":{"data":null,"links":{"self":"…"}}}},
      {"type":"customerReviews","id":"r-repondu","attributes":{"rating":2,"title":"Bof","body":"Trop cher",
        "reviewerNickname":"Max","createdDate":"2026-11-10T09:00:00-08:00","territory":"BEL"},
        "relationships":{"response":{"data":{"type":"customerReviewResponses","id":"rep-1"}}}},
      {"type":"customerReviews","id":"r-moyen","attributes":{"rating":3,"title":null,"body":"Correct",
        "reviewerNickname":"Sam","createdDate":"2026-11-02T10:00:00-07:00","territory":"FRA"},
        "relationships":{"response":{"data":null}}},
      {"type":"customerReviews","id":"r-nul","attributes":{"rating":1,"title":"Nul","body":"Plante",
        "reviewerNickname":"Tom","createdDate":"2026-10-01T10:00:00-07:00","territory":"ESP"},
        "relationships":{"response":{"data":null}}}],
     "included":[{"type":"customerReviewResponses","id":"rep-1","attributes":{"responseBody":"Merci","state":"PUBLISHED"}}],
     "links":{"self":"…"},"meta":{"paging":{"total":4,"limit":200}}}"#;

const VERSIONS: &str = r#"
    {"data":[
      {"type":"appStoreVersions","id":"v-203","attributes":{"platform":"IOS","versionString":"2.0.3",
        "appStoreState":"READY_FOR_SALE","appVersionState":"READY_FOR_DISTRIBUTION","createdDate":"2026-11-01T14:25:38-08:00"}},
      {"type":"appStoreVersions","id":"v-204","attributes":{"platform":"IOS","versionString":"2.0.4",
        "appStoreState":"WAITING_FOR_REVIEW","appVersionState":"WAITING_FOR_REVIEW","createdDate":"2026-11-12T08:00:00-08:00"}}],
     "links":{"self":"…"},"meta":{"paging":{"total":2,"limit":5}}}"#;

fn maintenant() -> Date { jour::depuis_iso("2026-11-13T00:00:00Z").unwrap() }

/// Une clé P-256 tirée pour le test : la paire et son PEM PKCS#8, le format des `.p8` d'Apple.
fn cle_p256() -> (EcdsaKeyPair, String) {
    let alea = SystemRandom::new();
    let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &alea).unwrap();
    let paire = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &alea).unwrap();
    let b64 = STANDARD.encode(pkcs8.as_ref());
    let lignes: Vec<&str> = b64.as_bytes().chunks(64).map(|c| std::str::from_utf8(c).unwrap()).collect();
    (paire, format!("-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n", lignes.join("\n")))
}

fn partie(s: &str) -> Value { serde_json::from_slice(&URL_SAFE_NO_PAD.decode(s).unwrap()).unwrap() }

#[test]
fn jeton_signe_es256() {
    let (paire, pem) = cle_p256();
    let jwt = jeton("KEY1234", "issuer-1", &pem, &maintenant()).unwrap();
    assert!(!jwt.contains('=') && !jwt.contains('+') && !jwt.contains('/'));
    let parties: Vec<&str> = jwt.split('.').collect();
    assert_eq!(parties.len(), 3);
    let entete = partie(parties[0]);
    assert_eq!(entete["alg"], "ES256");
    assert_eq!(entete["kid"], "KEY1234");
    assert_eq!(entete["typ"], "JWT");
    let charge = partie(parties[1]);
    let (iat, exp) = (charge["iat"].as_i64().unwrap(), charge["exp"].as_i64().unwrap());
    assert_eq!(charge["iss"], "issuer-1");
    assert_eq!(charge["aud"], "appstoreconnect-v1");
    assert_eq!(iat, maintenant().timestamp());
    assert!(exp > iat && exp - iat <= 20 * 60);
    let message = format!("{}.{}", parties[0], parties[1]);
    let signature = URL_SAFE_NO_PAD.decode(parties[2]).unwrap();
    let publique = UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, paire.public_key().as_ref());
    assert!(publique.verify(message.as_bytes(), &signature).is_ok());
    let (autre, _) = cle_p256();
    assert!(UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, autre.public_key().as_ref())
        .verify(message.as_bytes(), &signature)
        .is_err());
}

#[test]
fn cle_lue_dans_le_dossier() {
    let dossier = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&dossier).unwrap();
    assert!(matches!(cle(&dossier), Err(Erreur::NonConfigure(_))));
    let (_, pem) = cle_p256();
    std::fs::write(dossier.join("AuthKey_TEST1234.p8"), &pem).unwrap();
    let lue = cle(&dossier);
    std::fs::remove_dir_all(&dossier).unwrap();
    assert_eq!(lue.unwrap(), ("TEST1234".to_string(), pem));
}

#[test]
fn cle_illisible() {
    let dossier = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&dossier).unwrap();
    std::fs::write(dossier.join("AuthKey_ABIME.p8"), "pas une clé").unwrap();
    let lue = cle(&dossier);
    std::fs::remove_dir_all(&dossier).unwrap();
    assert_eq!(lue.unwrap_err().to_string(), "Non configuré : clé AuthKey_ABIME.p8 illisible");
}

#[test]
fn avis_decodes() {
    let avis = avis(AVIS.as_bytes(), "roleo").unwrap();
    assert_eq!(avis.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["r-top", "r-repondu", "r-moyen", "r-nul"]);
    assert_eq!(avis.iter().map(|a| a.repondu).collect::<Vec<_>>(), [false, true, false, false]);
    assert_eq!(avis[0].at, jour::depuis_iso("2026-11-11T08:18:14Z").unwrap());
    assert_eq!(avis[0].pays.as_deref(), Some("FRA"));
    assert_eq!(avis[0].auteur.as_deref(), Some("Lea"));
    assert_eq!(avis[2].titre, None);
}

#[test]
fn taches_pour_les_avis_sans_reponse() {
    let avis = avis(AVIS.as_bytes(), "roleo").unwrap();
    let t = taches(&avis, "123");
    assert_eq!(t.iter().map(|t| t.cle.as_str()).collect::<Vec<_>>(), ["avis:r-moyen", "avis:r-nul"]);
    assert_eq!(t.iter().map(|t| t.ton).collect::<Vec<_>>(), [Ton::Neutre, Ton::Alerte]);
    assert_eq!(t[0].texte, "Répondre à Sam");
    assert_eq!(t[1].texte, "Répondre à Tom, « Nul »");
    assert_eq!(t[1].etiquette.as_deref(), Some("1 ★"));
    assert_eq!(t[1].lien.as_deref(), Some("https://appstoreconnect.apple.com/apps/123/distribution/activity/ios/ratingsResponses"));
}

#[test]
fn fil_des_avis_recents() {
    let avis = avis(AVIS.as_bytes(), "roleo").unwrap();
    let fil = fil_avis(&avis, "123", &maintenant());
    assert_eq!(fil.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["asc:avis:r-top", "asc:avis:r-repondu"]);
    assert_eq!(fil.iter().map(|e| e.texte.as_str()).collect::<Vec<_>>(), ["★★★★★ Top", "★★☆☆☆ Bof"]);
    assert!(fil.iter().all(|e| e.type_ == TypeFil::Avis));
}

#[test]
fn version_la_plus_recente() {
    let v = version(VERSIONS.as_bytes(), "roleo", "123", Some(maintenant())).unwrap().unwrap();
    assert_eq!(v.id, "asc:version:v-204:WAITING_FOR_REVIEW");
    assert_eq!(v.texte, "2.0.4 en attente d'examen");
    assert_eq!(v.at, maintenant());
    let premier = version(VERSIONS.as_bytes(), "roleo", "123", None).unwrap().unwrap();
    assert_eq!(premier.at, jour::depuis_iso("2026-11-12T16:00:00Z").unwrap());
    assert_eq!(version(br#"{"data":[]}"#, "roleo", "123", None).unwrap(), None);
}

#[test]
fn etats() {
    assert_eq!(etat("READY_FOR_SALE"), "en vente");
    assert_eq!(etat("IN_REVIEW"), "en examen");
    assert_eq!(etat("NOUVEL_ETAT"), "nouvel etat");
}

#[test]
fn ecriture_dans_le_store() {
    let store = essai::store();
    let ids = |sql: &str| -> Vec<String> { store.db.query(sql, p![]).unwrap().iter().filter_map(|l| l.texte("id")).collect() };
    let avis = avis(AVIS.as_bytes(), "roleo").unwrap();
    let version = version(VERSIONS.as_bytes(), "roleo", "123", Some(maintenant())).unwrap();
    let n = ecrire(&avis, version.clone(), "123", "roleo", &store, &maintenant()).unwrap();
    // 4 avis, 2 tâches, 2 avis récents et une version dans le fil
    assert_eq!(n, 9);
    assert_eq!(ids("select id from avis where repondu = 1"), ["r-repondu"]);
    assert_eq!(ids("select id from fil order by id"), ["asc:avis:r-repondu", "asc:avis:r-top", "asc:version:v-204:WAITING_FOR_REVIEW"]);
    let ouvertes = store.db.query("select id, ton from a_traiter where fait_le is null order by id", p![]).unwrap();
    assert_eq!(ouvertes.iter().filter_map(|l| l.texte("id")).collect::<Vec<_>>(), ["asc:avis:r-moyen", "asc:avis:r-nul"]);
    assert_eq!(ouvertes.iter().filter_map(|l| l.texte("ton")).collect::<Vec<_>>(), ["neutre", "alerte"]);

    // l'avis à 3 étoiles a reçu une réponse : sa tâche passe à fait, rien ne rentre deux fois dans le fil
    let suite: Vec<Avis> = avis.into_iter().map(|mut a| { a.repondu |= a.id == "r-moyen"; a }).collect();
    let m = ecrire(&suite, version, "123", "roleo", &store, &maintenant()).unwrap();
    assert_eq!(m, 5);
    assert_eq!(ids("select id from a_traiter where fait_le is null"), ["asc:avis:r-nul"]);
    assert_eq!(store.db.query("select count(*) as n from fil", p![]).unwrap()[0].entier("n"), Some(3));
}

/// Un rapport de ventes quotidien : premiers téléchargements (1F, 1T) de l'app, sans retéléchargement (3F), mise à
/// jour (7F), achat intégré (IAY) ni autre app.
#[test]
fn telechargements_du_rapport_de_ventes() {
    let colonnes = ["Provider", "SKU", "Product Type Identifier", "Units", "Begin Date", "Apple Identifier"];
    let lignes = [
        ["APPLE", "roleo", "1F", "12", "11/12/2026", "123"],
        ["APPLE", "roleo", "1F", "3", "11/12/2026", "123"],
        ["APPLE", "roleo", "1T", "2", "11/12/2026", "123"],
        ["APPLE", "roleo", "3F", "7", "11/12/2026", "123"],
        ["APPLE", "roleo", "7F", "40", "11/12/2026", "123"],
        ["APPLE", "an", "IAY", "1", "11/12/2026", "123"],
        ["APPLE", "autre", "1F", "50", "11/12/2026", "999"],
    ];
    let tsv: String = std::iter::once(colonnes).chain(lignes).map(|l| l.join("\t") + "\r\n").collect();
    assert_eq!(telechargements(tsv.as_bytes(), "123").unwrap(), 17.0);
    assert!(matches!(telechargements(b"Provider\tUnits\n", "123"), Err(Erreur::Format(_))));
    // le rapport arrive en gzip
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(&mut gz, tsv.as_bytes()).unwrap();
    assert_eq!(telechargements(&gunzip(&gz.finish().unwrap()).unwrap(), "123").unwrap(), 17.0);
    assert!(gunzip(&[0x1F, 0x8B, 8, 0]).is_err());
}

#[test]
fn jours_de_ventes_a_relire() {
    let tous: HashSet<String> = jour::derniers(HISTORIQUE_VENTES, "2026-11-12").into_iter().collect();
    assert!(a_relire(&tous, "2026-11-13").is_empty());
    let mut partiel = tous.clone();
    partiel.remove("2026-11-12");
    partiel.remove("2026-10-01");
    assert_eq!(a_relire(&partiel, "2026-11-13"), ["2026-10-01", "2026-11-12"]);
}

#[test]
fn telechargements_somment_les_stores() {
    let store = essai::store();
    let valeur = || store.db.query("select valeur from serie where metrique = ?", p![TELECHARGEMENTS]).unwrap()[0].nombre("valeur");
    Boutique::AppStore.telechargements(&store, "roleo", "2026-11-12", 47.0).unwrap();
    assert_eq!(valeur(), Some(47.0));
    Boutique::PlayStore.telechargements(&store, "roleo", "2026-11-12", 6.0).unwrap();
    assert_eq!(valeur(), Some(53.0));
}
