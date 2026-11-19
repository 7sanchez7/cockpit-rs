//! Les JWT signés en local : ES256 pour App Store Connect, RS256 pour les comptes de service Google.
use crate::erreur::{Erreur, Res};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::Serialize;

/// ES256 avec une clé `.p8` (PEM PKCS#8) et son `kid`.
pub fn es256(kid: &str, charge: &impl Serialize, pem: &str) -> Res<String> {
    let cle = EncodingKey::from_ec_pem(pem.as_bytes()).map_err(|e| Erreur::Format(format!("clé ES256 illisible : {e}")))?;
    let mut entete = Header::new(Algorithm::ES256);
    entete.kid = Some(kid.to_string());
    signer(&entete, charge, &cle)
}

/// RS256 avec la `private_key` d'un compte de service (PEM).
pub fn rs256(charge: &impl Serialize, pem: &str) -> Res<String> {
    let cle = EncodingKey::from_rsa_pem(pem.as_bytes()).map_err(|e| Erreur::Format(format!("clé RS256 illisible : {e}")))?;
    signer(&Header::new(Algorithm::RS256), charge, &cle)
}

fn signer(entete: &Header, charge: &impl Serialize, cle: &EncodingKey) -> Res<String> {
    jsonwebtoken::encode(entete, charge, cle).map_err(|e| Erreur::Format(format!("JWT : {e}")))
}
