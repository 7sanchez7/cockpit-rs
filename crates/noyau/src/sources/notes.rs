//! Le plan de l'accueil : la note « Cockpit » de l'app Notes, lue dans la base de Notes en lecture seule. Cockpit doit
//! avoir l'accès complet au disque, comme pour Safari. Notes n'existe que sur Mac. Voir docs/sources/notes.md.
use crate::collecte::{Contexte, Source};
use crate::erreur::{format_err, Erreur, Res};
use crate::modeles::LignePlan;
use async_trait::async_trait;
use std::io::Read;

/// Le titre de la note lue, sa première ligne.
pub const TITRE: &str = "Cockpit";
/// La section des lignes écrites avant le premier intertitre.
pub const SECTION_PAR_DEFAUT: &str = "Plan";

/// Les styles de paragraphe de Notes qui comptent ici ; les autres (puces, numéros, texte) font une ligne simple.
pub mod style {
    pub const TITRE: i64 = 0;
    pub const INTERTITRE: i64 = 1;
    pub const SOUS_TITRE: i64 = 2;
    pub const CASE_A_COCHER: i64 = 103;
}

pub struct Notes;

#[async_trait]
impl Source for Notes {
    fn id(&self) -> &'static str { "notes" }
    fn nom(&self) -> &'static str { "Notes" }
    fn cadence(&self) -> Option<i64> { Some(60) }

    async fn collecte(&self, ctx: &Contexte) -> Res<usize> {
        let lignes = lignes(&note(&lire()?)?);
        ctx.store.plan(&lignes)?;
        Ok(lignes.len())
    }
}

fn absente() -> Erreur { Erreur::NonConfigure("l'app Notes d'Apple n'existe que sur Mac".into()) }

#[cfg(not(target_os = "macos"))]
pub fn lire() -> Res<Vec<u8>> { Err(absente()) }

/// Le contenu de la note « Cockpit » modifiée en dernier, hors corbeille et hors note verrouillée.
#[cfg(target_os = "macos")]
pub fn lire() -> Res<Vec<u8>> {
    use crate::db::Database;
    use crate::p;
    use std::io::ErrorKind;

    let Some(maison) = dirs::home_dir() else { return Err(absente()) };
    let base = maison.join("Library").join("Group Containers").join("group.com.apple.notes").join("NoteStore.sqlite");
    if let Err(e) = std::fs::File::open(&base) {
        return Err(match e.kind() {
            ErrorKind::PermissionDenied => Erreur::NonConfigure(
                "Notes illisible : donner l'accès complet au disque à Cockpit (Réglages Système, Confidentialité et sécurité)".into()),
            ErrorKind::NotFound => absente(),
            _ => format_err(format!("base Notes illisible : {e}")),
        });
    }
    let db = Database::ouvrir(&base.to_string_lossy(), true)?;
    let lignes = db.query("
        select d.ZDATA as donnees from ZICCLOUDSYNCINGOBJECT n
        join ZICNOTEDATA d on d.Z_PK = n.ZNOTEDATA
        left join ZICCLOUDSYNCINGOBJECT f on f.Z_PK = n.ZFOLDER
        where trim(n.ZTITLE1) = ? collate nocase and coalesce(n.ZMARKEDFORDELETION, 0) = 0
            and coalesce(n.ZISPASSWORDPROTECTED, 0) = 0 and coalesce(f.ZFOLDERTYPE, 0) != 1
        order by n.ZMODIFICATIONDATE1 desc limit 1", p![TITRE])?;
    lignes.first().and_then(|l| l.donnees("donnees")).ok_or_else(|| Erreur::NonConfigure(
        format!("pas de note « {TITRE} » dans Notes : la créer, intertitres et cases à cocher")))
}

/// Le texte d'une note et ses morceaux de même style, dans l'ordre.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub texte: String,
    pub morceaux: Vec<Morceau>,
}

/// `longueur` en unités UTF-16, comme les chaînes de Notes ; `style` vaut -1 pour du texte simple.
#[derive(Debug, Clone, PartialEq)]
pub struct Morceau {
    pub longueur: i64,
    pub style: i64,
    pub fait: bool,
}

/// `ZDATA` est un protobuf en gzip : document (2), note (3), texte (2) et morceaux (5) ; un morceau porte sa
/// longueur (1) et son style de paragraphe (2), qui a son type (1) et sa case cochée ou non (5, puis 2).
pub fn note(zdata: &[u8]) -> Res<Note> {
    let octets = gunzip(zdata)?;
    let racine = Protobuf::lire(&octets)?;
    let note = match racine.message(2)? {
        Some(document) => document.message(3)?,
        None => None,
    };
    let Some(note) = note else { return Err(format_err("note Notes sans texte")) };
    let Some(texte) = note.octets(2).and_then(|o| String::from_utf8(o.to_vec()).ok()) else {
        return Err(format_err("note Notes sans texte"));
    };
    let morceaux = note.messages(5)?.iter().map(|m| {
        let style = m.message(2)?;
        let fait = match &style {
            Some(s) => s.message(5)?.and_then(|c| c.entier(2)).is_some_and(|v| v != 0),
            None => false,
        };
        Ok(Morceau {
            longueur: m.entier(1).unwrap_or(0) as i64,
            style: style.as_ref().and_then(|s| s.entier(1)).map(|v| v as i64).unwrap_or(-1),
            fait,
        })
    }).collect::<Res<_>>()?;
    Ok(Note { texte, morceaux })
}

/// Une ligne du plan par ligne de la note, sans le titre ni les lignes vides. Un titre, intertitre ou sous-titre
/// ouvre une section ; le style d'une ligne est celui de son premier caractère.
pub fn lignes(note: &Note) -> Vec<LignePlan> {
    let mut debuts: Vec<(i64, &Morceau)> = vec![];
    let mut position = 0;
    for m in &note.morceaux {
        debuts.push((position, m));
        position += m.longueur;
    }
    let mut section = SECTION_PAR_DEFAUT.to_string();
    let mut r = vec![];
    let mut debut: i64 = 0;
    let utf16: Vec<u16> = note.texte.encode_utf16().collect();
    for (n, ligne) in utf16.split(|u| *u == 0x0A).enumerate() {
        let ici = debut;
        debut += ligne.len() as i64 + 1;
        // U+FFFC marque une pièce jointe
        let texte = String::from_utf16_lossy(ligne).replace('\u{FFFC}', "");
        let texte = texte.trim_matches(blanc);
        if n == 0 || texte.is_empty() {
            continue;
        }
        let m = debuts.iter().rev().find(|(d, _)| *d <= ici).map(|(_, m)| *m);
        match m.map(|m| m.style) {
            Some(style::TITRE | style::INTERTITRE | style::SOUS_TITRE) => section = texte.to_string(),
            Some(style::CASE_A_COCHER) => r.push(LignePlan {
                section: section.clone(), texte: texte.into(), coche: true, fait: m.is_some_and(|m| m.fait),
            }),
            _ => r.push(LignePlan { section: section.clone(), texte: texte.into(), ..Default::default() }),
        }
    }
    r
}

/// Les blancs de `.whitespaces` en Swift : espaces et tabulation, pas les fins de ligne.
fn blanc(c: char) -> bool {
    c.is_whitespace() && !matches!(c, '\n' | '\r' | '\u{0B}' | '\u{0C}' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

/// Saute l'en-tête gzip et ses champs optionnels, inflate le deflate brut, ignore les 8 octets de fin.
pub fn gunzip(o: &[u8]) -> Res<Vec<u8>> {
    if o.len() <= 18 || o[0] != 0x1F || o[1] != 0x8B || o[2] != 8 {
        return Err(format_err("note Notes pas en gzip"));
    }
    let drapeaux = o[3];
    let mut i = 10;
    let sauter_chaine = |i: usize| (if i < o.len() { o[i..].iter().position(|b| *b == 0).map_or(o.len(), |p| i + p) } else { o.len() }) + 1;
    if drapeaux & 4 != 0 && i + 2 <= o.len() {
        i += 2 + (o[i] as usize | (o[i + 1] as usize) << 8);
    }
    if drapeaux & 8 != 0 {
        i = sauter_chaine(i);
    }
    if drapeaux & 16 != 0 {
        i = sauter_chaine(i);
    }
    if drapeaux & 2 != 0 {
        i += 2;
    }
    if i >= o.len() - 8 {
        return Err(format_err("gzip de Notes tronqué"));
    }
    let mut sortie = vec![];
    flate2::read::DeflateDecoder::new(&o[i..o.len() - 8]).read_to_end(&mut sortie)
        .map_err(|e| format_err(format!("note Notes illisible : {e}")))?;
    Ok(sortie)
}

/// Un message protobuf lu champ par champ, juste ce qu'il faut pour une note ; les champs de 32 et 64 bits sont sautés.
pub struct Protobuf<'a> {
    champs: Vec<Champ<'a>>,
}

/// (numéro, valeur varint, octets d'un champ de longueur)
type Champ<'a> = (u64, Option<u64>, Option<&'a [u8]>);

impl<'a> Protobuf<'a> {
    pub fn lire(o: &'a [u8]) -> Res<Protobuf<'a>> {
        let tronque = || format_err("protobuf tronqué");
        let mut i = 0;
        let varint = |i: &mut usize| -> Res<u64> {
            let mut v: u64 = 0;
            let mut decalage = 0;
            while *i < o.len() && decalage < 64 {
                let b = o[*i];
                *i += 1;
                v |= u64::from(b & 0x7F) << decalage;
                if b & 0x80 == 0 {
                    return Ok(v);
                }
                decalage += 7;
            }
            Err(tronque())
        };
        let mut champs = vec![];
        while i < o.len() {
            let cle = varint(&mut i)?;
            let numero = cle >> 3;
            match cle & 7 {
                0 => champs.push((numero, Some(varint(&mut i)?), None)),
                1 => i += 8,
                2 => {
                    let n = usize::try_from(varint(&mut i)?).ok().filter(|n| *n <= o.len() - i).ok_or_else(tronque)?;
                    champs.push((numero, None, Some(&o[i..i + n])));
                    i += n;
                }
                5 => i += 4,
                t => return Err(format_err(format!("protobuf : type de champ {t} inconnu"))),
            }
        }
        if i != o.len() {
            return Err(tronque());
        }
        Ok(Protobuf { champs })
    }

    pub fn entier(&self, numero: u64) -> Option<u64> { self.champs.iter().find(|c| c.0 == numero && c.1.is_some())?.1 }
    pub fn octets(&self, numero: u64) -> Option<&'a [u8]> { self.champs.iter().find(|c| c.0 == numero && c.2.is_some())?.2 }
    pub fn message(&self, numero: u64) -> Res<Option<Protobuf<'a>>> { self.octets(numero).map(Protobuf::lire).transpose() }
    pub fn messages(&self, numero: u64) -> Res<Vec<Protobuf<'a>>> {
        self.champs.iter().filter(|c| c.0 == numero).filter_map(|c| c.2).map(Protobuf::lire).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Une clé et sa valeur protobuf : un entier (type 0) ou des octets (type 2).
    fn varint(v: usize) -> Vec<u8> {
        let mut v = v as u64;
        let mut r = vec![];
        loop {
            r.push((v & 0x7F) as u8 | if v > 0x7F { 0x80 } else { 0 });
            v >>= 7;
            if v == 0 {
                return r;
            }
        }
    }
    fn entier(n: usize, v: usize) -> Vec<u8> { [varint(n << 3), varint(v)].concat() }
    fn octets(n: usize, v: &[u8]) -> Vec<u8> { [varint(n << 3 | 2), varint(v.len()), v.to_vec()].concat() }

    /// Le `ZDATA` d'une note : ses lignes, chacune avec son style et sa case cochée ou non.
    fn zdata(lignes: &[(&str, Option<i64>, bool)]) -> Vec<u8> {
        let texte = lignes.iter().map(|l| l.0).collect::<Vec<_>>().join("\n");
        let morceaux: Vec<u8> = lignes.iter().enumerate().flat_map(|(i, (t, s, fait))| {
            let longueur = t.encode_utf16().count() + usize::from(i < lignes.len() - 1);
            let mut style = vec![];
            if let Some(s) = s {
                style = entier(1, *s as usize);
            }
            if *s == Some(style::CASE_A_COCHER) {
                style.extend(octets(5, &[octets(1, b"id"), entier(2, usize::from(*fait))].concat()));
            }
            let style = if s.is_none() { vec![] } else { octets(2, &style) };
            octets(5, &[entier(1, longueur), style].concat())
        }).collect();
        let protobuf = octets(2, &octets(3, &[octets(2, texte.as_bytes()), morceaux].concat()));
        let mut e = flate2::write::DeflateEncoder::new(vec![], flate2::Compression::default());
        e.write_all(&protobuf).unwrap();
        [vec![0x1F, 0x8B, 8, 0, 0, 0, 0, 0, 0, 3], e.finish().unwrap(), vec![0; 8]].concat()
    }

    fn ligne(section: &str, texte: &str, coche: bool, fait: bool) -> LignePlan {
        LignePlan { section: section.into(), texte: texte.into(), coche, fait }
    }

    #[test]
    fn intertitres_et_cases() {
        let d = zdata(&[
            ("Cockpit", Some(style::TITRE), false),
            ("Finir l'accueil", None, false),
            ("", None, false),
            ("Cette semaine", Some(style::INTERTITRE), false),
            ("Tourner 3 vidéos", Some(style::CASE_A_COCHER), true),
            ("Relancer l'essai 3 j \u{FFFC}", Some(style::CASE_A_COCHER), false),
            ("Idées", Some(style::SOUS_TITRE), false),
            ("Carrousel « 5 erreurs » é", Some(100), false),
        ]);
        assert_eq!(lignes(&note(&d).unwrap()), [
            ligne("Plan", "Finir l'accueil", false, false),
            ligne("Cette semaine", "Tourner 3 vidéos", true, true),
            ligne("Cette semaine", "Relancer l'essai 3 j", true, false),
            ligne("Idées", "Carrousel « 5 erreurs » é", false, false),
        ]);
    }

    #[test]
    fn emojis_comptes_en_utf16() {
        let d = zdata(&[("Cockpit", Some(style::TITRE), false), ("🎬 Monter", Some(style::CASE_A_COCHER), false),
                        ("Texte", None, false)]);
        assert_eq!(lignes(&note(&d).unwrap()), [ligne("Plan", "🎬 Monter", true, false), ligne("Plan", "Texte", false, false)]);
    }

    #[test]
    fn gzip_avec_nom_de_fichier() {
        let mut d = zdata(&[("Cockpit", Some(0), false), ("A", None, false)]);
        d[3] = 8;
        d.splice(10..10, b"note.pb\0".iter().copied());
        assert_eq!(lignes(&note(&d).unwrap()), [ligne("Plan", "A", false, false)]);
    }

    #[test]
    fn refuse_ce_qui_n_est_pas_du_gzip() {
        assert!(note(&[1; 40]).is_err());
        assert!(Protobuf::lire(&[0x12, 0x09, 0x01]).is_err());
    }
}
