//! Le SAV : fils avec les users et retours, lus à chaque passage ; réponse, fermeture d'un fil et traitement
//! d'un retour écrits dans Firestore comme le dashboard admin. Voir docs/sources/firebase.md § SAV.
use super::firestore::{Document, Firestore, Valeur};
use crate::collecte::{Contexte, Requete};
use crate::erreur::{format_err, Erreur, Res};
use crate::jour;
use crate::modeles::{CleFil, FilSAV, MessageSAV, ReleveSAV, RetourSAV};
use crate::store::Store;
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashSet};

/// Fils et retours les plus récents relus à chaque passage, en plus des ouverts.
pub const RECENTS: usize = 50;
/// Le fil complet que l'app et le dashboard affichent (`threadLimit` de Roleo).
pub const MESSAGES_PAR_FIL: usize = 100;
/// Une réponse, en caractères Unicode (`maxMessageRunes` de Roleo).
pub const LONGUEUR_REPONSE: usize = 2000;
pub const LONGUEUR_APERCU: usize = 140;
/// Ce que le SAV lit d'un `users/{uid}`.
pub const CHAMPS_FIL: [&str; 6] = ["supportLastAt", "supportLastFrom", "supportPreview", "supportOpen", "supportUnread", "displayName"];

// lecture

/// Les fils ouverts et récents, les retours à traiter et récents, les messages des fils qui ont bougé depuis
/// la dernière lecture, les emails pas encore connus.
pub async fn lire_sav(business: &str, f: &Firestore, ouverts: &[Document], nouveaux: &[Document], store: &Store)
                      -> Res<ReleveSAV> {
    let mut r = ReleveSAV::default();
    // un compte qui n'a jamais écrit a supportLastAt à null, trié après les autres
    let recents: Vec<Document> = f.derniers("users", None, "supportLastAt", RECENTS, Some(&CHAMPS_FIL)).await?
        .into_iter().filter(|d| d.champ("supportLastAt").and_then(Valeur::instant).is_some()).collect();
    r.fils = uniques(ouverts, &recents).into_iter().map(|d| fil(d, business)).collect::<Res<_>>()?;
    let retours_recents = f.derniers("feedback", None, "createdAt", RECENTS, None).await?;
    r.retours = uniques(nouveaux, &retours_recents).into_iter().map(|d| retour(d, business)).collect::<Res<_>>()?;
    let lus = store.fils_lus(business)?;
    for fil in &r.fils {
        if lus.get(&fil.uid) == Some(&jour::iso(&fil.dernier_at)) {
            continue;
        }
        let docs = f.derniers("support", Some(&format!("users/{}", fil.uid)), "createdAt", MESSAGES_PAR_FIL, None).await?;
        r.messages.insert(fil.uid.clone(), docs.iter().rev().map(message).collect::<Res<_>>()?);
    }
    let connus = store.comptes_sav(business)?;
    let uids: BTreeSet<String> = r.fils.iter().map(|x| x.uid.clone())
        .chain(r.retours.iter().filter_map(|x| x.uid.clone()))
        .filter(|u| !connus.contains(u))
        .collect();
    r.emails = emails(&uids.into_iter().collect::<Vec<_>>(), f).await?;
    Ok(r)
}

pub fn fil(d: &Document, business: &str) -> Res<FilSAV> {
    let at = d.champ("supportLastAt").and_then(Valeur::instant)
        .ok_or_else(|| format_err(format!("users/{} sans supportLastAt", d.id())))?;
    // `readUnread` de Roleo
    let u = d.champ("supportUnread");
    let non_vu = u.and_then(|u| u.champ("id")).and_then(Valeur::texte).is_some()
        && u.and_then(|u| u.champ("text")).and_then(Valeur::texte).is_some()
        && u.and_then(|u| u.champ("at")).and_then(Valeur::instant).is_some();
    Ok(FilSAV {
        business: business.into(), uid: d.id(), nom: non_vide(d, "displayName"), email: None, dernier_at: at,
        dernier_equipe: d.champ("supportLastFrom").and_then(Valeur::texte) == Some("team"),
        apercu: d.champ("supportPreview").and_then(Valeur::texte).unwrap_or("").into(),
        ouvert: d.champ("supportOpen").and_then(Valeur::booleen) == Some(true), non_vu,
    })
}

pub fn message(d: &Document) -> Res<MessageSAV> {
    let at = d.champ("createdAt").and_then(Valeur::instant).ok_or_else(|| format_err("un message de support sans createdAt"))?;
    Ok(MessageSAV {
        id: d.id(), equipe: d.champ("from").and_then(Valeur::texte) == Some("team"),
        texte: d.champ("text").and_then(Valeur::texte).unwrap_or("").into(), at,
    })
}

pub fn retour(d: &Document, business: &str) -> Res<RetourSAV> {
    let at = d.champ("createdAt").and_then(Valeur::instant)
        .ok_or_else(|| format_err(format!("feedback/{} sans createdAt", d.id())))?;
    Ok(RetourSAV {
        business: business.into(), id: d.id(), uid: non_vide(d, "uid"), email: None, source: non_vide(d, "source"),
        categorie: non_vide(d, "category"), etoiles: d.champ("stars").and_then(Valeur::nombre).map(|n| n as i64),
        message: d.champ("message").and_then(Valeur::texte).unwrap_or("").into(), citation: non_vide(d, "quote"),
        conversation: non_vide(d, "conversationId"), traite: d.champ("status").and_then(Valeur::texte) == Some("done"),
        traite_le: d.champ("handledAt").and_then(Valeur::instant), plateforme: non_vide(d, "platform"),
        build: d.champ("appBuild").and_then(Valeur::nombre).map(|n| (n as i64).to_string()), at,
    })
}

/// L'email de chaque uid, vide pour un invité ou un compte supprimé. Cent par appel.
pub async fn emails(uids: &[String], f: &Firestore) -> Res<BTreeMap<String, String>> {
    #[derive(Deserialize)]
    struct Comptes {
        users: Option<Vec<Compte>>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Compte {
        local_id: String,
        email: Option<String>,
    }
    let mut emails = BTreeMap::new();
    for lot in uids.chunks(100) {
        let req = requete(format!("https://identitytoolkit.googleapis.com/v1/projects/{}/accounts:lookup", f.projet), f,
                          &json!({"localId": lot}));
        for uid in lot {
            emails.insert(uid.clone(), String::new());
        }
        for c in f.http.json::<Comptes>(&req).await?.users.unwrap_or_default() {
            emails.insert(c.local_id, c.email.unwrap_or_default());
        }
    }
    Ok(emails)
}

// actions de l'écran SAV

/// Envoie la réponse de l'équipe comme le dashboard (`sendTeamMessage` de Roleo) : le message et le résumé du fil
/// en un commit, puis la notif sur chaque appareil. Rend ce que l'écran affiche sous le champ.
pub async fn repondre(ctx: &Contexte, texte: &str, cle: &CleFil) -> Res<String> {
    let t = reponse(texte).ok_or_else(|| format_err(format!("réponse vide ou de plus de {LONGUEUR_REPONSE} caractères")))?;
    let (f, nom) = firestore(&cle.business, ctx).await?;
    let id = nouvel_id();
    let maintenant = jour::maintenant();
    let resume = apercu(&t);
    let non_lu = BTreeMap::from([
        ("id".to_string(), Valeur::Texte(id.clone())), ("text".to_string(), Valeur::Texte(t.clone())),
        ("at".to_string(), Valeur::Instant(maintenant)),
    ]);
    f.commit(vec![
        f.ecriture(&format!("users/{}/support/{id}", cle.uid), &[
            ("from", Valeur::Texte("team".into())), ("text", Valeur::Texte(t.clone())),
            ("sentBy", Valeur::Texte("cockpit".into())), ("feedbackId", Valeur::Nul), ("createdAt", Valeur::Instant(maintenant)),
        ], false)?,
        f.ecriture(&format!("users/{}", cle.uid), &[
            ("supportLastAt", Valeur::Instant(maintenant)), ("supportLastFrom", Valeur::Texte("team".into())),
            ("supportPreview", Valeur::Texte(resume.clone())), ("supportOpen", Valeur::Booleen(false)),
            ("supportUnread", Valeur::Carte(non_lu)),
        ], true)?,
    ]).await?;
    ctx.store.repondre_sav(cle, &MessageSAV { id: id.clone(), equipe: true, texte: t, at: maintenant }, &resume)?;
    Ok(match notifier(&cle.uid, &nom, &id, &resume, &f).await {
        Ok(0) => "Envoyé, aucun appareil à notifier".into(),
        Ok(_) => "Envoyé et notifié".into(),
        Err(e) => format!("Envoyé, la notif a échoué : {e}"),
    })
}

/// Le fil marqué traité sans réponse (`closeThread` de Roleo).
pub async fn fermer(ctx: &Contexte, cle: &CleFil) -> Res<()> {
    let (f, _) = firestore(&cle.business, ctx).await?;
    f.commit(vec![f.ecriture(&format!("users/{}", cle.uid), &[("supportOpen", Valeur::Booleen(false))], true)?]).await?;
    ctx.store.fermer_fil_sav(cle)
}

/// Un retour marqué traité, ou rouvert, comme le bouton du dashboard.
pub async fn traiter(ctx: &Contexte, r: &RetourSAV, traite: bool) -> Res<()> {
    let (f, _) = firestore(&r.business, ctx).await?;
    let maintenant = jour::maintenant();
    let champs = [
        ("status", Valeur::Texte(if traite { "done" } else { "new" }.into())),
        ("handledAt", if traite { Valeur::Instant(maintenant) } else { Valeur::Nul }),
    ];
    f.commit(vec![f.ecriture(&format!("feedback/{}", r.id), &champs, true)?]).await?;
    ctx.store.traiter_retour_sav(&r.business, &r.id, traite, &maintenant)
}

/// La notif `support` sur chaque appareil du compte (`notifyUser` de Roleo) ; un jeton que FCM refuse est effacé.
/// Rend le nombre d'appareils atteints ; lève la première autre erreur si aucun ne l'a été.
async fn notifier(uid: &str, titre: &str, id: &str, corps: &str, f: &Firestore) -> Res<usize> {
    let appareils = f.ou("pushTokens", "uid", json!({"stringValue": uid}), Some(&["token"])).await?;
    let mut atteints = 0;
    let mut morts = vec![];
    let mut erreur = None;
    for a in &appareils {
        let Some(jeton) = a.champ("token").and_then(Valeur::texte) else { continue };
        let req = requete(format!("https://fcm.googleapis.com/v1/projects/{}/messages:send", f.projet), f,
                          &json!({"message": notif(jeton, titre, id, corps)}));
        match f.http.donnees(&req).await {
            Ok(_) => atteints += 1,
            // UNREGISTERED ou INVALID_ARGUMENT : app désinstallée, jeton tourné
            Err(Erreur::Http(400 | 404, _)) => morts.push(json!({"delete": a.name})),
            Err(e) => {
                erreur.get_or_insert(e);
            }
        }
    }
    if !morts.is_empty() {
        f.commit(morts).await?;
    }
    match erreur {
        Some(e) if atteints == 0 => Err(e),
        _ => Ok(atteints),
    }
}

/// Le message FCM de `pushMessage` de Roleo : data seule sur Android, alerte APNs sur iOS.
pub fn notif(jeton: &str, titre: &str, id: &str, corps: &str) -> Value {
    json!({
        "token": jeton,
        "data": {"kind": "support", "message_id": id, "title": titre, "body": corps},
        "android": {"priority": "HIGH"},
        "apns": {"payload": {"aps": {"alert": {"title": titre, "body": corps}, "sound": "default"}}},
    })
}

// outils

/// Le texte d'une réponse sans les blancs autour ; `None` s'il est vide ou trop long (`parseText` de Roleo).
pub fn reponse(texte: &str) -> Option<String> {
    let t = texte.trim();
    if t.is_empty() || t.chars().count() > LONGUEUR_REPONSE { None } else { Some(t.into()) }
}

/// Le résumé d'un fil (`preview` de Roleo) : blancs réduits à une espace, 140 caractères Unicode au plus.
pub fn apercu(texte: &str) -> String {
    let ligne = Regex::new(r"\s+").expect("regex des blancs").replace_all(texte, " ").into_owned();
    if ligne.chars().count() <= LONGUEUR_APERCU {
        return ligne;
    }
    ligne.chars().take(LONGUEUR_APERCU - 1).collect::<String>() + "…"
}

/// Un id de document comme Firestore les tire : 20 caractères alphanumériques.
pub fn nouvel_id() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let octets: Vec<u8> = [uuid::Uuid::new_v4(), uuid::Uuid::new_v4()].iter().flat_map(|u| *u.as_bytes()).collect();
    octets.iter().take(20).map(|o| ALPHABET[*o as usize % ALPHABET.len()] as char).collect()
}

/// Le Firestore du projet d'un business, et son nom (titre des notifs).
async fn firestore(business: &str, ctx: &Contexte) -> Res<(Firestore, String)> {
    let b = ctx.businesses.iter().find(|b| b.id == business);
    let Some((b, projet)) = b.and_then(|b| b.config.firebase_project.as_ref().filter(|p| !p.is_empty()).map(|p| (b, p))) else {
        return Err(Erreur::NonConfigure("ce business n'a pas de projet Firebase".into()));
    };
    Ok((Firestore { projet: projet.clone(), http: ctx.http.clone(), jeton: ctx.gcloud.jeton().await? }, b.nom.clone()))
}

fn requete(url: String, f: &Firestore, corps: &Value) -> Requete {
    Requete::new(url, "POST").jeton(&f.jeton).entete("x-goog-user-project", &f.projet).json(corps)
}

/// Les documents de `a` puis de `b`, chaque id une seule fois.
fn uniques<'a>(a: &'a [Document], b: &'a [Document]) -> Vec<&'a Document> {
    let mut vus = HashSet::new();
    a.iter().chain(b).filter(|d| vus.insert(d.id())).collect()
}

fn non_vide(d: &Document, champ: &str) -> Option<String> {
    d.champ(champ).and_then(Valeur::texte).filter(|s| !s.is_empty()).map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jour::Date;
    use crate::modeles::EtatFil;
    use crate::sources::firebase::firestore::tests::document;

    fn d(s: &str) -> Date { jour::depuis_iso(s).unwrap() }

    fn user(champs: &str) -> Res<FilSAV> {
        fil(&document(&format!(r#"{{"name": "projects/p/databases/(default)/documents/users/u1", "fields": {{{champs}}}}}"#))?,
            "roleo")
    }

    #[test]
    fn etat_du_fil() {
        let at = r#""supportLastAt": {"timestampValue": "2026-11-14T10:00:00.250Z"}"#;
        let a_repondre = user(&(at.to_string() + r#", "supportLastFrom": {"stringValue": "user"}, "supportOpen": {"booleanValue": true},"#
            + r#""supportPreview": {"stringValue": "Bonjour"}, "displayName": {"stringValue": "Lina"}"#)).unwrap();
        assert_eq!(a_repondre.etat(), EtatFil::ARepondre);
        assert_eq!(a_repondre.nom.as_deref(), Some("Lina"));
        assert_eq!(a_repondre.apercu, "Bonjour");
        assert_eq!(jour::iso(&a_repondre.dernier_at), "2026-11-14T10:00:00Z");
        let unread = r#""supportUnread": {"mapValue": {"fields": {"id": {"stringValue": "m1"}, "text": {"stringValue": "Re"},"#
            .to_string() + r#""at": {"timestampValue": "2026-11-14T10:00:00Z"}}}}"#;
        let equipe = at.to_string() + r#", "supportLastFrom": {"stringValue": "team"}, "supportOpen": {"booleanValue": false}"#;
        assert_eq!(user(&format!("{equipe}, {unread}")).unwrap().etat(), EtatFil::PasLu);
        assert_eq!(user(&(equipe + r#", "supportUnread": {"nullValue": null}"#)).unwrap().etat(), EtatFil::Lu);
        let traite = at.to_string() + r#", "supportLastFrom": {"stringValue": "user"}, "displayName": {"stringValue": ""}"#;
        assert_eq!(user(&traite).unwrap().etat(), EtatFil::Traite);
        assert_eq!(user(at).unwrap().nom, None);
        assert!(user(r#""supportOpen": {"booleanValue": true}"#).is_err());
    }

    #[test]
    fn retour_lu() {
        let complet = retour(&document(r#"
            {"name": "projects/p/databases/(default)/documents/feedback/r1", "fields": {
              "uid": {"stringValue": "u1"}, "source": {"stringValue": "chat_report"}, "category": {"stringValue": "weird"},
              "message": {"stringValue": "bizarre"}, "quote": {"stringValue": "Hola"}, "conversationId": {"stringValue": "c1"},
              "status": {"stringValue": "done"}, "handledAt": {"timestampValue": "2026-11-14T11:00:00Z"},
              "platform": {"stringValue": "ios"}, "appBuild": {"integerValue": "49"}, "stars": {"integerValue": "2"},
              "createdAt": {"timestampValue": "2026-11-14T10:00:00Z"}}}
            "#).unwrap(), "roleo").unwrap();
        assert_eq!(complet.id, "r1");
        assert_eq!(complet.uid.as_deref(), Some("u1"));
        assert_eq!(complet.source.as_deref(), Some("chat_report"));
        assert_eq!(complet.categorie.as_deref(), Some("weird"));
        assert_eq!(complet.citation.as_deref(), Some("Hola"));
        assert_eq!(complet.conversation.as_deref(), Some("c1"));
        assert!(complet.traite);
        assert_eq!(complet.traite_le.map(|x| jour::iso(&x)).as_deref(), Some("2026-11-14T11:00:00Z"));
        assert_eq!(complet.plateforme.as_deref(), Some("ios"));
        assert_eq!(complet.build.as_deref(), Some("49"));
        assert_eq!(complet.etoiles, Some(2));
        let ancien = retour(&document(r#"
            {"name": "a/feedback/r0", "fields": {"message": {"stringValue": "vieux"}, "status": {"stringValue": "new"},
              "createdAt": {"timestampValue": "2026-09-01T10:00:00Z"}}}
            "#).unwrap(), "roleo").unwrap();
        assert!(ancien.uid.is_none() && ancien.source.is_none() && ancien.categorie.is_none() && ancien.build.is_none());
        assert!(!ancien.traite);
        let sans_date = document(r#"{"name": "a/feedback/r2", "fields": {"message": {"stringValue": "x"}}}"#).unwrap();
        assert!(retour(&sans_date, "roleo").is_err());
    }

    #[test]
    fn message_lu() {
        let m = message(&document(r#"
            {"name": "a/users/u1/support/m1", "fields": {"from": {"stringValue": "team"}, "text": {"stringValue": "Re"},
              "createdAt": {"timestampValue": "2026-11-14T10:00:00Z"}}}
            "#).unwrap()).unwrap();
        assert_eq!(m, MessageSAV { id: "m1".into(), equipe: true, texte: "Re".into(), at: d("2026-11-14T10:00:00Z") });
    }

    #[test]
    fn apercu_et_reponse() {
        assert_eq!(apercu("Salut\n\n  toi\t!"), "Salut toi !");
        let long = "é".repeat(200);
        assert_eq!(apercu(&long).chars().count(), LONGUEUR_APERCU);
        assert!(apercu(&long).ends_with('…'));
        let juste = "a".repeat(LONGUEUR_APERCU);
        assert_eq!(apercu(&juste), juste);
        assert_eq!(reponse("  merci \n").as_deref(), Some("merci"));
        assert_eq!(reponse(" \n "), None);
        assert!(reponse(&"a".repeat(LONGUEUR_REPONSE)).is_some());
        assert_eq!(reponse(&"a".repeat(LONGUEUR_REPONSE + 1)), None);
        let id = nouvel_id();
        assert!(id.len() == 20 && id.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn notif_support() {
        let n = notif("jeton", "Roleo", "m1", "Re");
        assert_eq!(n["token"], "jeton");
        assert_eq!(n["data"], json!({"kind": "support", "message_id": "m1", "title": "Roleo", "body": "Re"}));
        assert_eq!(n["android"], json!({"priority": "HIGH"}));
        assert_eq!(n["apns"]["payload"]["aps"]["alert"], json!({"title": "Roleo", "body": "Re"}));
    }
}
