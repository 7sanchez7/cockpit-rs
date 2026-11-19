//! Firestore en REST : `batchGet` et `runQuery` lisent ; `commit` écrit, pour le SAV seulement (docs/sources/firebase.md).
use crate::collecte::{Http, Requete};
use crate::erreur::{format_err, Res};
use crate::jour::{self, Date};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap};

pub struct Firestore {
    pub projet: String,
    pub http: Http,
    pub jeton: String,
}

impl Firestore {
    fn racine(&self) -> String { format!("projects/{}/databases/(default)/documents", self.projet) }

    /// Les documents trouvés parmi `ids`, rangés par id, réduits à `champs`. Cent par appel.
    pub async fn documents(&self, collection: &str, ids: &[String], champs: &[&str]) -> Res<HashMap<String, Document>> {
        #[derive(Deserialize)]
        struct Lu {
            found: Option<Document>,
            missing: Option<String>,
        }
        let mut trouves = HashMap::new();
        for lot in ids.chunks(100) {
            let noms: Vec<String> = lot.iter().map(|id| format!("{}/{collection}/{id}", self.racine())).collect();
            let corps = json!({"documents": noms, "mask": {"fieldPaths": champs}});
            for l in self.http.json::<Vec<Lu>>(&self.requete(":batchGet", &corps)).await? {
                match (l.found, l.missing) {
                    (Some(d), _) => {
                        trouves.insert(d.id(), d);
                    }
                    (None, None) => return Err(format_err(format!("batchGet {collection} : ni found ni missing"))),
                    (None, Some(_)) => {}
                }
            }
        }
        Ok(trouves)
    }

    /// Les documents de `collection` dont `champ` vaut `valeur` (typée, `{"booleanValue": true}`), réduits à `champs`
    /// (`None` : tous les champs).
    pub async fn ou(&self, collection: &str, champ: &str, valeur: Value, champs: Option<&[&str]>) -> Res<Vec<Document>> {
        let mut structure = json!({
            "from": [{"collectionId": collection}],
            "where": {"fieldFilter": {"field": {"fieldPath": champ}, "op": "EQUAL", "value": valeur}},
        });
        if let Some(c) = champs {
            structure["select"] = selection(c);
        }
        self.requete_structuree(structure, None).await
    }

    /// Les `limite` derniers documents de `collection` par `ordre` décroissant, réduits à `champs` (`None` : tous) ;
    /// `parent` vise une sous-collection (`users/<uid>`). Un document sans `ordre` n'en fait pas partie.
    pub async fn derniers(&self, collection: &str, parent: Option<&str>, ordre: &str, limite: usize,
                          champs: Option<&[&str]>) -> Res<Vec<Document>> {
        let mut structure = json!({
            "from": [{"collectionId": collection}],
            "orderBy": [{"field": {"fieldPath": ordre}, "direction": "DESCENDING"}],
            "limit": limite,
        });
        if let Some(c) = champs {
            structure["select"] = selection(c);
        }
        self.requete_structuree(structure, parent).await
    }

    /// Écrit tout ou rien (`ecriture`, `{"delete": nom}`).
    pub async fn commit(&self, ecritures: Vec<Value>) -> Res<()> {
        self.http.donnees(&self.requete(":commit", &json!({"writes": ecritures}))).await?;
        Ok(())
    }

    /// Remplace les `champs` de `chemin` (`users/<uid>`) sans toucher aux autres ; `existe` false crée le document
    /// et échoue s'il est déjà là, true échoue s'il n'existe pas.
    pub fn ecriture(&self, chemin: &str, champs: &[(&str, Valeur)], existe: bool) -> Res<Value> {
        let mut fields = Map::new();
        for (k, v) in champs {
            fields.insert((*k).to_string(), valeur(v)?);
        }
        let masque: Vec<&str> = champs.iter().map(|(k, _)| *k).collect();
        Ok(json!({
            "update": {"name": format!("{}/{chemin}", self.racine()), "fields": fields},
            "updateMask": {"fieldPaths": masque},
            "currentDocument": {"exists": existe},
        }))
    }

    /// Lance une requête structurée, à la racine ou sous `parent`.
    async fn requete_structuree(&self, structure: Value, parent: Option<&str>) -> Res<Vec<Document>> {
        #[derive(Deserialize)]
        struct Trouve {
            document: Option<Document>,
        }
        let chemin = parent.map(|p| format!("/{p}:runQuery")).unwrap_or_else(|| ":runQuery".into());
        // une réponse sans `document` ne porte que readTime : aucun résultat
        let r = self.http.json::<Vec<Trouve>>(&self.requete(&chemin, &json!({"structuredQuery": structure}))).await?;
        Ok(r.into_iter().filter_map(|t| t.document).collect())
    }

    fn requete(&self, methode: &str, corps: &Value) -> Requete {
        Requete::new(format!("https://firestore.googleapis.com/v1/{}{methode}", self.racine()), "POST")
            .jeton(&self.jeton)
            .entete("x-goog-user-project", &self.projet)
            .json(corps)
    }
}

fn selection(champs: &[&str]) -> Value {
    json!({"fields": champs.iter().map(|c| json!({"fieldPath": c})).collect::<Vec<_>>()})
}

/// Une valeur en valeur typée de Firestore REST. L'écriture ne prend que ce que le SAV écrit.
pub fn valeur(v: &Valeur) -> Res<Value> {
    Ok(match v {
        Valeur::Nul => json!({"nullValue": null}),
        Valeur::Booleen(b) => json!({"booleanValue": b}),
        Valeur::Entier(n) => json!({"integerValue": n.to_string()}),
        Valeur::Texte(s) => json!({"stringValue": s}),
        Valeur::Instant(d) => json!({"timestampValue": d.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()}),
        Valeur::Carte(m) => {
            let mut fields = Map::new();
            for (k, x) in m {
                fields.insert(k.clone(), valeur(x)?);
            }
            json!({"mapValue": {"fields": fields}})
        }
        Valeur::Reel(_) => return Err(format_err("valeur Firestore non prise en charge : Double")),
        Valeur::Liste(_) => return Err(format_err("valeur Firestore non prise en charge : Array")),
    })
}

/// Un document lu en REST, ses champs décodés.
#[derive(Debug, Clone, Deserialize)]
pub struct Document {
    pub name: String,
    pub fields: Option<BTreeMap<String, Valeur>>,
}

impl Document {
    pub fn id(&self) -> String { self.name.rsplit('/').next().unwrap_or_default().to_string() }

    pub fn champ(&self, nom: &str) -> Option<&Valeur> { self.fields.as_ref()?.get(nom) }
}

/// Une valeur typée de Firestore REST : `{"integerValue": "22"}`, `{"mapValue": {"fields": …}}`…
#[derive(Debug, Clone, PartialEq)]
pub enum Valeur {
    Entier(i64),
    Reel(f64),
    Texte(String),
    Booleen(bool),
    Instant(Date),
    Carte(BTreeMap<String, Valeur>),
    Liste(Vec<Valeur>),
    Nul,
}

impl Valeur {
    pub fn nombre(&self) -> Option<f64> {
        match self {
            Valeur::Entier(n) => Some(*n as f64),
            Valeur::Reel(x) => Some(*x),
            _ => None,
        }
    }

    pub fn texte(&self) -> Option<&str> { if let Valeur::Texte(s) = self { Some(s) } else { None } }
    pub fn instant(&self) -> Option<Date> { if let Valeur::Instant(d) = self { Some(*d) } else { None } }
    pub fn booleen(&self) -> Option<bool> { if let Valeur::Booleen(b) = self { Some(*b) } else { None } }
    pub fn champ(&self, cle: &str) -> Option<&Valeur> { if let Valeur::Carte(m) = self { m.get(cle) } else { None } }

    fn depuis(v: &Value) -> Result<Valeur, String> {
        let o = v.as_object().ok_or("type Firestore non lu")?;
        if let Some(s) = o.get("integerValue").and_then(Value::as_str) {
            // int64 en chaîne
            return s.parse().map(Valeur::Entier).map_err(|_| "entier illisible".into());
        }
        if let Some(x) = o.get("doubleValue").and_then(Value::as_f64) {
            return Ok(Valeur::Reel(x));
        }
        if let Some(s) = o.get("stringValue").and_then(Value::as_str) {
            return Ok(Valeur::Texte(s.into()));
        }
        if let Some(b) = o.get("booleanValue").and_then(Value::as_bool) {
            return Ok(Valeur::Booleen(b));
        }
        if let Some(s) = o.get("timestampValue").and_then(Value::as_str) {
            return jour::depuis_iso(s).map(Valeur::Instant).ok_or_else(|| "instant illisible".into());
        }
        if let Some(m) = o.get("mapValue") {
            let mut carte = BTreeMap::new();
            if let Some(f) = m.get("fields").and_then(Value::as_object) {
                for (k, x) in f {
                    carte.insert(k.clone(), Valeur::depuis(x)?);
                }
            }
            return Ok(Valeur::Carte(carte));
        }
        if let Some(l) = o.get("arrayValue") {
            let valeurs = l.get("values").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
            return valeurs.iter().map(Valeur::depuis).collect::<Result<_, _>>().map(Valeur::Liste);
        }
        if o.contains_key("nullValue") {
            return Ok(Valeur::Nul);
        }
        Err("type Firestore non lu".into())
    }
}

impl<'de> Deserialize<'de> for Valeur {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Valeur::depuis(&Value::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn document(json: &str) -> Res<Document> { crate::collecte::http::decoder(json.as_bytes()) }

    #[test]
    fn valeurs_typees() {
        let d = document(r#"
            {"name": "projects/p/databases/(default)/documents/feedback/abc", "fields": {
              "n": {"integerValue": "22"}, "x": {"doubleValue": 1.5}, "s": {"stringValue": "salut"},
              "b": {"booleanValue": true}, "t": {"timestampValue": "2026-11-12T23:49:10.123456Z"},
              "m": {"mapValue": {"fields": {"k": {"integerValue": "3"}}}}, "vide": {"mapValue": {}},
              "l": {"arrayValue": {"values": [{"stringValue": "a"}, {"nullValue": null}]}}, "z": {"nullValue": null}}}
            "#).unwrap();
        assert_eq!(d.id(), "abc");
        assert_eq!(d.champ("n"), Some(&Valeur::Entier(22)));
        assert_eq!(d.champ("n").unwrap().nombre(), Some(22.0));
        assert_eq!(d.champ("x").unwrap().nombre(), Some(1.5));
        assert_eq!(d.champ("s").unwrap().texte(), Some("salut"));
        assert_eq!(d.champ("s").unwrap().nombre(), None);
        assert_eq!(d.champ("b").unwrap().booleen(), Some(true));
        assert_eq!(jour::iso(&d.champ("t").unwrap().instant().unwrap()), "2026-11-12T23:49:10Z");
        assert_eq!(d.champ("m").unwrap().champ("k"), Some(&Valeur::Entier(3)));
        assert_eq!(d.champ("vide"), Some(&Valeur::Carte(BTreeMap::new())));
        assert_eq!(d.champ("l"), Some(&Valeur::Liste(vec![Valeur::Texte("a".into()), Valeur::Nul])));
        assert_eq!(d.champ("z"), Some(&Valeur::Nul));
        assert_eq!(d.champ("absent"), None);
        assert_eq!(document(r#"{"name": "a/b"}"#).unwrap().champ("x"), None);
    }

    #[test]
    fn valeur_illisible_leve() {
        assert!(document(r#"{"name": "a/b", "fields": {"n": {"integerValue": "douze"}}}"#).is_err());
        assert!(document(r#"{"name": "a/b", "fields": {"t": {"timestampValue": "hier"}}}"#).is_err());
        assert!(document(r#"{"name": "a/b", "fields": {"g": {"geoPointValue": {}}}}"#).is_err());
    }

    #[test]
    fn valeurs_firestore() {
        let instant = jour::depuis_iso("2026-11-14T10:00:00.250Z").unwrap();
        assert_eq!(valeur(&Valeur::Nul).unwrap()["nullValue"], Value::Null);
        assert_eq!(valeur(&Valeur::Booleen(true)).unwrap()["booleanValue"], true);
        assert_eq!(valeur(&Valeur::Entier(3)).unwrap()["integerValue"], "3");
        assert_eq!(valeur(&Valeur::Texte("x".into())).unwrap()["stringValue"], "x");
        assert_eq!(valeur(&Valeur::Instant(instant)).unwrap()["timestampValue"], "2026-11-14T10:00:00.250Z");
        let carte = valeur(&Valeur::Carte(BTreeMap::from([
            ("id".to_string(), Valeur::Texte("m1".into())),
            ("at".to_string(), Valeur::Nul),
        ]))).unwrap();
        assert_eq!(carte["mapValue"]["fields"]["id"]["stringValue"], "m1");
        assert!(carte["mapValue"]["fields"]["at"].as_object().unwrap().contains_key("nullValue"));
        assert!(valeur(&Valeur::Reel(1.5)).is_err());
    }

    #[test]
    fn ecriture_firestore() {
        let f = Firestore { projet: "p".into(), http: Http::new(), jeton: "j".into() };
        let e = f.ecriture("users/u1", &[("supportOpen", Valeur::Booleen(false)), ("supportPreview", Valeur::Texte("Re".into()))], true)
            .unwrap();
        assert_eq!(e["update"]["name"], "projects/p/databases/(default)/documents/users/u1");
        assert_eq!(e["update"]["fields"]["supportOpen"]["booleanValue"], false);
        assert_eq!(e["updateMask"]["fieldPaths"], json!(["supportOpen", "supportPreview"]));
        assert_eq!(e["currentDocument"]["exists"], true);
    }
}
