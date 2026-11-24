//! SQLite par rusqlite. Paramètres : entiers, réels, booléens, textes, dates (écrites en ISO), nul.
use crate::erreur::{Erreur, Res};
use crate::jour::{self, Date};
use parking_lot::ReentrantMutex;
use rusqlite::types::{ToSqlOutput, Value as SqlValue, ValueRef};
use rusqlite::{Connection, OpenFlags, ToSql};
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Valeur {
    Null,
    Int(i64),
    Reel(f64),
    Texte(String),
    Blob(Vec<u8>),
}

impl ToSql for Valeur {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(match self {
            Valeur::Null => ToSqlOutput::Owned(SqlValue::Null),
            Valeur::Int(v) => ToSqlOutput::Owned(SqlValue::Integer(*v)),
            Valeur::Reel(v) => ToSqlOutput::Owned(SqlValue::Real(*v)),
            Valeur::Texte(v) => ToSqlOutput::Borrowed(ValueRef::Text(v.as_bytes())),
            Valeur::Blob(v) => ToSqlOutput::Borrowed(ValueRef::Blob(v)),
        })
    }
}

macro_rules! depuis_entier { ($($t:ty),*) => { $(impl From<$t> for Valeur { fn from(v: $t) -> Self { Valeur::Int(v as i64) } })* }; }
depuis_entier!(i32, i64, u32, u64, usize, isize);
impl From<bool> for Valeur { fn from(v: bool) -> Self { Valeur::Int(v as i64) } }
impl From<f64> for Valeur { fn from(v: f64) -> Self { Valeur::Reel(v) } }
impl From<&str> for Valeur { fn from(v: &str) -> Self { Valeur::Texte(v.to_string()) } }
impl From<String> for Valeur { fn from(v: String) -> Self { Valeur::Texte(v) } }
impl From<&String> for Valeur { fn from(v: &String) -> Self { Valeur::Texte(v.clone()) } }
impl From<Date> for Valeur { fn from(v: Date) -> Self { Valeur::Texte(jour::iso(&v)) } }
impl From<&Date> for Valeur { fn from(v: &Date) -> Self { Valeur::Texte(jour::iso(v)) } }
impl<T: Into<Valeur>> From<Option<T>> for Valeur {
    fn from(v: Option<T>) -> Self { v.map(Into::into).unwrap_or(Valeur::Null) }
}
impl<T: Clone + Into<Valeur>> From<&Option<T>> for Valeur {
    fn from(v: &Option<T>) -> Self { v.clone().map(Into::into).unwrap_or(Valeur::Null) }
}

/// Les paramètres d'une requête : `p![a, b, c]`.
#[macro_export]
macro_rules! p {
    () => { Vec::<$crate::db::Valeur>::new() };
    ($($x:expr),+ $(,)?) => { vec![$($crate::db::Valeur::from($x)),+] };
}

/// Une ligne de résultat, colonnes par nom. Une colonne NULL est absente.
#[derive(Debug, Clone, Default)]
pub struct Ligne {
    pub colonnes: HashMap<String, Valeur>,
}

impl Ligne {
    pub fn texte(&self, c: &str) -> Option<String> {
        match self.colonnes.get(c)? {
            Valeur::Texte(s) => Some(s.clone()),
            _ => None,
        }
    }
    pub fn entier(&self, c: &str) -> Option<i64> {
        match self.colonnes.get(c)? {
            Valeur::Int(v) => Some(*v),
            Valeur::Reel(v) => Some(*v as i64),
            _ => None,
        }
    }
    pub fn nombre(&self, c: &str) -> Option<f64> {
        match self.colonnes.get(c)? {
            Valeur::Reel(v) => Some(*v),
            Valeur::Int(v) => Some(*v as f64),
            _ => None,
        }
    }
    pub fn date(&self, c: &str) -> Option<Date> { self.texte(c).and_then(|s| jour::depuis_iso(&s)) }
    pub fn donnees(&self, c: &str) -> Option<Vec<u8>> {
        match self.colonnes.get(c)? {
            Valeur::Blob(b) => Some(b.clone()),
            Valeur::Texte(s) => Some(s.as_bytes().to_vec()),
            _ => None,
        }
    }
    pub fn booleen(&self, c: &str) -> bool { self.entier(c).unwrap_or(0) != 0 }
}

struct Interne {
    conn: Connection,
    profondeur: usize,
}

/// Une connexion partagée entre threads. Une transaction dans une transaction rejoint la première.
pub struct Database {
    interne: ReentrantMutex<RefCell<Interne>>,
}

impl Database {
    /// `lecture` ouvre la base d'une autre app sans jamais l'écrire ni toucher à son journal.
    pub fn ouvrir(chemin: &str, lecture: bool) -> Res<Self> {
        let drapeaux = if lecture {
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX
        } else {
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_NO_MUTEX
        };
        let conn = Connection::open_with_flags(chemin, drapeaux)
            .map_err(|_| Erreur::Sql(format!("ouverture de {chemin} impossible")))?;
        conn.busy_timeout(std::time::Duration::from_millis(5000))?;
        if !lecture {
            conn.execute_batch("pragma journal_mode = wal; pragma synchronous = normal;")?;
        }
        Ok(Database { interne: ReentrantMutex::new(RefCell::new(Interne { conn, profondeur: 0 })) })
    }

    /// Une base en mémoire, pour les tests.
    pub fn memoire() -> Res<Self> {
        let conn = Connection::open_in_memory()?;
        Ok(Database { interne: ReentrantMutex::new(RefCell::new(Interne { conn, profondeur: 0 })) })
    }

    /// Lignes modifiées par la dernière écriture.
    pub fn changements(&self) -> usize {
        let g = self.interne.lock();
        let n = g.borrow().conn.changes();
        n as usize
    }

    /// Plusieurs instructions sans paramètres (le schéma).
    pub fn script(&self, sql: &str) -> Res<()> {
        let g = self.interne.lock();
        let r = g.borrow().conn.execute_batch(sql);
        Ok(r?)
    }

    pub fn execute(&self, sql: &str, params: Vec<Valeur>) -> Res<usize> {
        let g = self.interne.lock();
        let i = g.borrow();
        let mut stmt = i.conn.prepare_cached(sql)?;
        let n = stmt.execute(rusqlite::params_from_iter(params.iter()))?;
        Ok(n)
    }

    pub fn query(&self, sql: &str, params: Vec<Valeur>) -> Res<Vec<Ligne>> {
        let g = self.interne.lock();
        let i = g.borrow();
        let mut stmt = i.conn.prepare_cached(sql)?;
        let noms: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let mut rows = stmt.query(rusqlite::params_from_iter(params.iter()))?;
        let mut lignes = Vec::new();
        while let Some(r) = rows.next()? {
            let mut colonnes = HashMap::with_capacity(noms.len());
            for (k, nom) in noms.iter().enumerate() {
                match valeur(r.get_ref(k)?) {
                    Valeur::Null => {}
                    v => { colonnes.insert(nom.clone(), v); }
                }
            }
            lignes.push(Ligne { colonnes });
        }
        Ok(lignes)
    }

    /// Les noms des colonnes dans l'ordre du select, puis les lignes, NULL compris : pour afficher une requête libre.
    pub fn tableau(&self, sql: &str) -> Res<(Vec<String>, Vec<Vec<Valeur>>)> {
        let g = self.interne.lock();
        let i = g.borrow();
        let mut stmt = i.conn.prepare(sql)?;
        let noms: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let mut rows = stmt.query([])?;
        let mut lignes = Vec::new();
        while let Some(r) = rows.next()? {
            lignes.push((0..noms.len()).map(|k| r.get_ref(k).map(valeur)).collect::<Result<_, _>>()?);
        }
        Ok((noms, lignes))
    }

    pub fn colonne_existe(&self, table: &str, colonne: &str) -> Res<bool> {
        Ok(self
            .query("select name from pragma_table_info(?)", crate::p![table])?
            .iter()
            .any(|l| l.texte("name").as_deref() == Some(colonne)))
    }

    /// Tout ou rien. Une transaction dans une transaction rejoint la première.
    pub fn transaction<T>(&self, f: impl FnOnce() -> Res<T>) -> Res<T> {
        let g = self.interne.lock();
        if g.borrow().profondeur > 0 {
            return f();
        }
        g.borrow().conn.execute_batch("begin immediate")?;
        g.borrow_mut().profondeur += 1;
        let r = f();
        g.borrow_mut().profondeur -= 1;
        match r {
            Ok(v) => {
                g.borrow().conn.execute_batch("commit")?;
                Ok(v)
            }
            Err(e) => {
                let _ = g.borrow().conn.execute_batch("rollback");
                Err(e)
            }
        }
    }
}

fn valeur(v: ValueRef) -> Valeur {
    match v {
        ValueRef::Null => Valeur::Null,
        ValueRef::Integer(v) => Valeur::Int(v),
        ValueRef::Real(v) => Valeur::Reel(v),
        ValueRef::Text(t) => Valeur::Texte(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => Valeur::Blob(b.to_vec()),
    }
}
