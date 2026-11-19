//! Les lectures propres à l'écran Santé.
use super::Lectures;
use crate::erreur::Res;
use crate::jour::Date;
use crate::p;
use std::collections::HashMap;

impl Lectures<'_> {
    /// La fin du dernier passage réussi de chaque source ; une source jamais réussie est absente.
    pub fn derniers_succes(&self) -> Res<HashMap<String, Date>> {
        let mut r = HashMap::new();
        for l in self.q("select source, max(fin) as fin from collecte where etat = 'ok' group by source", p![])? {
            if let (Some(s), Some(fin)) = (l.texte("source"), l.date("fin")) { r.entry(s).or_insert(fin); }
        }
        Ok(r)
    }
}
