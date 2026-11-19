//! Rapports Play figés et JWT RS256 du compte de service, avec une clé RSA tirée pour le test.
use super::*;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};

const PREFIXE: &str = "stats/installs/installs_app.roleo.roleo_";

fn fichier_cle(contenu: &str) -> PathBuf {
    let f = std::env::temp_dir().join(format!("{}-play-publisher.json", uuid::Uuid::new_v4()));
    std::fs::write(&f, contenu).unwrap();
    f
}

#[test]
fn mois_lus() {
    let mut noms: Vec<String> =
        ["202611", "202607", "202609", "202608", "202610"].iter().map(|m| format!("{PREFIXE}{m}_overview.csv")).collect();
    noms.push(format!("{PREFIXE}202611_country.csv"));
    noms.push("stats/installs/installs_autre.app_202611_overview.csv".into());
    let attendu: Vec<String> = ["202608", "202609", "202610", "202611"].iter().map(|m| format!("{PREFIXE}{m}_overview.csv")).collect();
    assert_eq!(a_lire(&noms, PREFIXE, "2026-11-14"), attendu);
}

#[test]
fn bucket_copie_depuis_la_console() {
    for config in ["pubsite_prod_rev_0123", " gs://pubsite_prod_rev_0123/ ", "gs://pubsite_prod_rev_0123/stats/installs"] {
        assert_eq!(bucket(config), "pubsite_prod_rev_0123");
    }
}

#[test]
fn rapport_en_utf16() {
    let csv = "Date,Package Name,Daily Device Installs,Daily Device Uninstalls,Daily Device Upgrades,Total User Installs,\
        Daily User Installs,Daily User Uninstalls,Active Device Installs,Install events,Update events,Uninstall events\n\
        2026-11-14,app.roleo.roleo,4,0,0,4,3,0,4,4,0,0\n\
        2026-11-15,app.roleo.roleo,9,1,0,12,8,1,11,10,0,1\n\n";
    let data: Vec<u8> = [0xFF, 0xFE].into_iter().chain(csv.encode_utf16().flat_map(u16::to_le_bytes)).collect();
    let attendu = HashMap::from([("2026-11-14".to_string(), 3.0), ("2026-11-15".to_string(), 8.0)]);
    assert_eq!(installs(&data).unwrap(), attendu);
    assert!(matches!(installs(b"Date,Installs\n2026-11-14,3\n"), Err(Erreur::Format(_))));
    assert!(matches!(installs(&[0xFF, 0xFE, 0x00, 0xD8]), Err(Erreur::Format(_))));
}

#[test]
fn assertion_signee_rs256() {
    let privee = rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).unwrap();
    let pem = privee.to_pkcs8_pem(LineEnding::LF).unwrap().to_string();
    let corps = serde_json::json!({
        "client_email": "play@exemple.iam.gserviceaccount.com",
        "token_uri": "https://oauth2.googleapis.com/token",
        "private_key": pem,
    });
    let f = fichier_cle(&corps.to_string());
    let c = compte(&f);
    std::fs::remove_file(&f).unwrap();
    let c = c.unwrap();
    let maintenant = jour::depuis_iso("2026-11-14T12:00:00Z").unwrap();
    let jwt = assertion(&c, &maintenant).unwrap();
    assert_eq!(jwt.split('.').count(), 3);
    let publique = privee.to_public_key().to_public_key_pem(LineEnding::LF).unwrap();
    let mut validation = Validation::new(Algorithm::RS256);
    validation.validate_exp = false;
    validation.set_audience(&[&c.url_jeton]);
    let charge = jsonwebtoken::decode::<Value>(&jwt, &DecodingKey::from_rsa_pem(publique.as_bytes()).unwrap(), &validation)
        .unwrap()
        .claims;
    assert_eq!(charge["iss"], c.email.as_str());
    assert_eq!(charge["scope"], "https://www.googleapis.com/auth/devstorage.read_only");
    assert_eq!(charge["aud"], c.url_jeton.as_str());
    assert_eq!(charge["exp"].as_i64(), Some(maintenant.timestamp() + 3600));
}

#[test]
fn compte_abime() {
    let absent = std::env::temp_dir().join("roleo-play-publisher.json");
    assert_eq!(compte(&absent).unwrap_err().to_string(), "Non configuré : clé du compte de service absente (~/.googleplay/roleo-play-publisher.json)");
    let f = fichier_cle(r#"{"client_email":"play@exemple.iam.gserviceaccount.com","token_uri":"https://oauth2.googleapis.com/token",
        "private_key":"-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----"}"#);
    let abimee = compte(&f);
    std::fs::remove_file(&f).unwrap();
    assert!(matches!(abimee, Err(Erreur::Format(_))));
    let f = fichier_cle(r#"{"client_email":"play@exemple.iam.gserviceaccount.com"}"#);
    let incomplet = compte(&f);
    std::fs::remove_file(&f).unwrap();
    assert!(matches!(incomplet, Err(Erreur::Format(_))));
}

#[test]
fn nom_d_objet_encode() {
    assert_eq!(alphanumerique("stats/a_1.csv"), "stats%2Fa%5F1%2Ecsv");
}
