//! Une demande libre à Claude depuis l'écran Veille : Sonnet 5.5 choisit les mots-clés, Haiku 5.5 cherche sur TikTok
//! et Instagram, analyse et retient les vidéos qui répondent, puis écrit une synthèse. Voir docs/sources/veille.md.
use super::analyse::{self, appeler, facturer, texte, HAIKU, SONNET};
use super::{chercher, classement, ouvrir, pause, videos_par_mot, PLATEFORMES};
use crate::business::Business;
use crate::collecte::fournisseurs::Fournisseurs;
use crate::collecte::{miniatures, Contexte, Navigateur};
use crate::erreur::{format_err, Erreur, Res};
use crate::jour::{self, Date};
use crate::lecture::veille::PLAFOND_VEILLE_USD;
use crate::lecture::Lectures;
use crate::modeles::Plateforme;
use crate::modeles_veille::{DemandeVeille, MotVeille, VideoVeille};
use crate::store::Store;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// Les recherches, analyses et relances de mots-clés d'une demande, au plus.
pub const CHERCHES_MAX: usize = 8;
pub const ANALYSES_MAX: usize = 10;
pub const AUTRES_MOTS_MAX: usize = 2;
/// Les allers-retours avec Haiku avant d'abandonner.
pub const TOURS_MAX: usize = 30;
/// Le seuil de vues d'une recherche quand Haiku n'en donne pas.
pub const VUES_MIN: i64 = 10_000;
/// Les mots-clés gardés d'une réponse de Sonnet.
pub const MOTS_MAX: usize = 8;
pub const PROMPT_MAX: usize = 2_000;
/// La légende d'une vidéo telle que Haiku la lit, en caractères.
const LEGENDE_MAX: usize = 160;
const MAX_TOKENS: i64 = 8_000;
const DELAI_S: u64 = 180;

pub const CHERCHER: &str = "chercher";
pub const ANALYSER: &str = "analyser";
pub const AUTRES_MOTS: &str = "autres_mots";
pub const RETENIR: &str = "retenir";

/// Un mot-clé proposé par Sonnet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotPropose {
    pub mot: String,
    pub plateforme: Plateforme,
    pub pourquoi: String,
}

/// Un appel d'outil demandé par Haiku.
#[derive(Debug, Clone, PartialEq)]
pub struct Appel {
    pub id: String,
    pub nom: String,
    pub entree: Value,
}

/// Écrit une demande qui commence, à la suite de la conversation `fil` ou dans une nouvelle ; celle restée en cours
/// passe en échec.
pub fn creer(store: &Store, business: &str, prompt: &str, fil: Option<&str>) -> Res<DemandeVeille> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err(non("la demande est vide"));
    }
    if prompt.chars().count() > PROMPT_MAX {
        return Err(non(format!("la demande dépasse {PROMPT_MAX} caractères")));
    }
    let id = uuid::Uuid::new_v4().to_string().to_uppercase();
    let fil = match fil {
        Some(f) => store.demande_veille(f)?.filter(|d| d.business == business).ok_or_else(|| non("conversation introuvable"))?.fil,
        None => id.clone(),
    };
    let d = DemandeVeille {
        id, business: business.into(), prompt: prompt.into(), debut: jour::maintenant(), fin: None,
        etat: DemandeVeille::EN_COURS.into(), journal: vec![], synthese: None, erreur: None, fil,
    };
    store.nouvelle_demande(&d)?;
    store.a_change();
    Ok(d)
}

/// Mène une demande jusqu'au bout et écrit sa fin : la synthèse, ou l'erreur qui l'a arrêtée.
pub async fn mener(ctx: &Contexte, d: &DemandeVeille) {
    let fournisseurs = Fournisseurs::resoudre(ctx).await;
    let mut e = Enquete {
        ctx, d, navigateurs: HashMap::new(), fournisseurs, cherches: 0, analyses: 0, autres: 0, essayes: vec![], avant: String::new(),
    };
    let r = e.enqueter().await;
    for nav in e.navigateurs.into_values().flatten() {
        nav.fermer().await;
    }
    // les vidéos trouvées entrent classées dans la bank
    for echec in classement::classer(ctx, &jour::maintenant()).await {
        let _ = ctx.store.noter_demande(&d.id, &format!("Échec du classement : {echec}"));
    }
    let fin = jour::maintenant();
    let ecrit = match &r {
        Ok(s) => ctx.store.finir_demande(&d.id, &fin, Ok(s)),
        Err(err) => ctx.store.finir_demande(&d.id, &fin, Err(&err.to_string())),
    };
    if let Err(err) = ecrit {
        eprintln!("demande {} : {err}", d.id);
    }
    ctx.store.a_change();
}

struct Enquete<'a> {
    ctx: &'a Contexte,
    d: &'a DemandeVeille,
    /// Une entrée par plateforme déjà cherchée ; TikTok y garde son navigateur caché, ouvert à sa première recherche,
    /// Instagram `None` : sa recherche se passe de session.
    navigateurs: HashMap<Plateforme, Option<Box<dyn Navigateur>>>,
    /// La clé TikHub : avec elle, la recherche et les profils passent par l'API, plus de page cachée ni de session.
    fournisseurs: Fournisseurs,
    cherches: usize,
    analyses: usize,
    autres: usize,
    /// Les recherches faites, pour que Sonnet ne les repropose pas.
    essayes: Vec<String>,
    /// Ce qui s'est dit avant dans la conversation, vide pour une première demande.
    avant: String,
}

impl Enquete<'_> {
    async fn enqueter(&mut self) -> Res<String> {
        let ctx = self.ctx;
        let b = ctx.businesses.iter().find(|b| b.id == self.d.business)
            .ok_or_else(|| non(format!("business inconnu : {}", self.d.business)))?;
        let cle = ctx.secrets.valeur(&b.config.claude, b).await?;
        self.avant = avant(&Lectures::new(&ctx.store.db, vec![b.clone()]), self.d)?;
        // une suite part de la conversation : Haiku répond, ou redemande des mots-clés s'il doit chercher
        let premier = if self.avant.is_empty() { premier_message(&self.mots(b, &cle, None).await?) } else { SUITE.into() };
        let systeme = systeme(&self.d.prompt, &b.nom, &jour::maintenant(), &self.avant);
        let mut messages = vec![json!({"role": "user", "content": premier})];
        for _ in 0..TOURS_MAX {
            plafond(&ctx.store)?;
            let o = appeler(&ctx.http, &cle, &corps_boucle(&systeme, &messages), DELAI_S).await?;
            facturer(ctx, &self.objet(), &HAIKU, &o, &jour::maintenant())?;
            if o.get("stop_reason").and_then(Value::as_str) != Some("tool_use") {
                return texte(&o);
            }
            let appels = appels(&o);
            if appels.is_empty() {
                return Err(format_err("Haiku demande un outil sans le nommer"));
            }
            let mut resultats = vec![];
            for a in &appels {
                resultats.push(match self.outil(a, b, &cle).await {
                    Ok(t) => json!({"type": "tool_result", "tool_use_id": a.id, "content": t}),
                    Err(e) => {
                        self.noter(&format!("Échec de {} : {e}", a.nom))?;
                        json!({"type": "tool_result", "tool_use_id": a.id, "content": e.to_string(), "is_error": true})
                    }
                });
            }
            // la réponse telle quelle, réflexion comprise
            messages.push(json!({"role": "assistant", "content": o.get("content").cloned().unwrap_or_else(|| json!([]))}));
            messages.push(json!({"role": "user", "content": resultats}));
        }
        Err(non(format!("arrêtée après {TOURS_MAX} tours sans synthèse")))
    }

    async fn outil(&mut self, a: &Appel, b: &Business, cle: &str) -> Res<String> {
        match a.nom.as_str() {
            CHERCHER => self.chercher(&a.entree).await,
            ANALYSER => self.analyser(&a.entree, b, cle).await,
            AUTRES_MOTS => {
                let raison = champ(&a.entree, "raison")?;
                if self.autres >= AUTRES_MOTS_MAX {
                    return Err(non(format!("{AUTRES_MOTS_MAX} relances au plus, déjà faites")));
                }
                self.autres += 1;
                Ok(json!({"mots": self.mots(b, cle, Some(&raison)).await?}).to_string())
            }
            RETENIR => {
                let v = self.video(&champ(&a.entree, "id")?)?;
                self.ctx.store.retenir_video(&self.d.id, &v.id, &champ(&a.entree, "raison")?)?;
                self.noter(&format!("Retenue : @{} sur {}", v.compte, v.plateforme.nom()))?;
                Ok("retenue".into())
            }
            autre => Err(non(format!("outil inconnu : {autre}"))),
        }
    }

    /// Les mots-clés de Sonnet ; `raison` quand Haiku en redemande.
    async fn mots(&mut self, b: &Business, cle: &str, raison: Option<&str>) -> Res<Vec<MotPropose>> {
        plafond(&self.ctx.store)?;
        self.noter(if raison.is_some() { "Sonnet cherche d'autres mots-clés" } else { "Sonnet choisit les mots-clés" })?;
        let suivis: Vec<String> = self.ctx.store.mots_veille()?.into_iter().filter(|m| m.actif && m.business == b.id).map(|m| m.mot).collect();
        let consigne = consigne_mots(&self.d.prompt, &b.nom, &suivis, &self.essayes, raison, &self.avant);
        let o = appeler(&self.ctx.http, cle, &corps_mots(&consigne), DELAI_S).await?;
        facturer(self.ctx, &self.objet(), &SONNET, &o, &jour::maintenant())?;
        let mots = lire_mots(&o)?;
        let liste: Vec<String> = mots.iter().map(|m| format!("« {} » ({})", m.mot, m.plateforme.nom())).collect();
        self.noter(&format!("Mots-clés : {}", liste.join(", ")))?;
        Ok(mots)
    }

    async fn chercher(&mut self, entree: &Value) -> Res<String> {
        let (mot, p, vues_min) = entree_chercher(entree)?;
        if self.cherches >= CHERCHES_MAX {
            return Err(non(format!("{CHERCHES_MAX} recherches au plus, déjà faites")));
        }
        self.noter(&format!("Recherche « {mot} » sur {}", p.nom()))?;
        // sans fournisseur, un humain qui cherche, pas une rafale : la pause de la veille entre deux recherches d'un même site
        if !self.fournisseurs.configure() {
            if let Some(nav) = self.navigateurs.get(&p) {
                pause(nav.as_deref()).await;
            } else {
                let nav = if p == Plateforme::Tiktok { Some(ouvrir(self.ctx, p).await?) } else { None };
                self.navigateurs.insert(p, nav);
            }
        }
        let m = MotVeille { business: self.d.business.clone(), mot: mot.clone(), vues_min, actif: true, cherche: None };
        let voulues = videos_par_mot(&self.ctx.store)?;
        let nav = self.navigateurs.get(&p).and_then(|n| n.as_deref());
        let gardees = chercher(self.ctx, p, &m, voulues, &self.fournisseurs, nav, &jour::maintenant()).await?;
        // une recherche qui échoue ne compte pas
        self.cherches += 1;
        let images: HashMap<String, String> = gardees.iter().filter_map(|t| Some((t.id(), t.image.clone()?))).collect();
        let echecs = miniatures::telecharger(&images, &self.ctx.http).await;
        if !echecs.is_empty() {
            self.noter(&format!("Miniatures : {} échecs ({})", echecs.len(), echecs.join(" ; ")))?;
        }
        let ids: Vec<String> = gardees.iter().map(|t| t.id()).collect();
        let mut videos = self.ctx.store.videos_veille_par_ids(&ids)?;
        videos.sort_by_key(|v| std::cmp::Reverse(v.publiee));
        self.essayes.push(format!("« {mot} » sur {} : {} vidéos", p.nom(), videos.len()));
        self.noter(&format!("« {mot} » sur {} : {} vidéos", p.nom(), videos.len()))?;
        Ok(json!({"videos": videos.iter().map(fiche).collect::<Vec<_>>()}).to_string())
    }

    async fn analyser(&mut self, entree: &Value, b: &Business, cle: &str) -> Res<String> {
        let v = self.video(&champ(entree, "id")?)?;
        if let Some(a) = &v.analyse {
            return Ok(serde_json::to_string(a)?);
        }
        if self.analyses >= ANALYSES_MAX {
            return Err(non(format!("{ANALYSES_MAX} analyses au plus, déjà faites")));
        }
        plafond(&self.ctx.store)?;
        self.analyses += 1;
        self.noter(&format!("Analyse de @{} sur {}", v.compte, v.plateforme.nom()))?;
        analyse::analyser_une(self.ctx, &v, cle, &analyse::formats(b.config.contenu_dir.as_deref()), &jour::maintenant()).await?;
        let a = self.video(&v.id)?.analyse.ok_or_else(|| format_err("analyse écrite mais introuvable"))?;
        Ok(serde_json::to_string(&a)?)
    }

    fn video(&self, id: &str) -> Res<VideoVeille> {
        self.ctx.store.videos_veille("where v.id = ?", crate::p![id])?.into_iter().next()
            .ok_or_else(|| non(format!("aucune vidéo {id}, prendre un id rendu par {CHERCHER}")))
    }

    /// Une ligne au journal, que l'écran montre tout de suite.
    fn noter(&self, ligne: &str) -> Res<()> {
        self.ctx.store.noter_demande(&self.d.id, ligne)?;
        self.ctx.store.a_change();
        Ok(())
    }

    /// Le nom des appels de la demande dans `veille_cout`.
    fn objet(&self) -> String { format!("demande:{}", self.d.id) }
}

/// Une erreur dite telle quelle.
fn non(m: impl Into<String>) -> Erreur { Erreur::Commande(m.into()) }

pub(crate) fn plafond(store: &Store) -> Res<()> {
    let depense = store.depense_veille_usd()?;
    if depense >= PLAFOND_VEILLE_USD {
        return Err(non(format!("plafond atteint : {} $ dépensés sur {} $", depense as i64, PLAFOND_VEILLE_USD as i64)));
    }
    Ok(())
}

// la conversation

/// Le premier message d'une suite.
const SUITE: &str = "Voici la nouvelle demande, à la suite de la conversation.";

/// Les demandes d'avant dans la conversation : chacune, sa réponse et les vidéos qu'elle a retenues ; vide pour une
/// première demande.
pub fn avant(l: &Lectures, d: &DemandeVeille) -> Res<String> {
    let mut lignes = vec![];
    for x in l.fil_veille(&d.fil)?.into_iter().take_while(|x| x.id != d.id) {
        lignes.push(format!("- Demande : {}", x.prompt));
        if let Some(r) = x.synthese.as_deref().or(x.erreur.as_deref()) {
            lignes.push(format!("  Réponse : {r}"));
        }
        for (v, raison) in l.retenues_veille(&x.id)? {
            lignes.push(format!("  Retenue : {} @{} sur {} : {raison}", v.id, v.compte, v.plateforme.nom()));
        }
    }
    Ok(if lignes.is_empty() { String::new() } else { format!("Avant, dans cette conversation :\n{}", lignes.join("\n")) })
}

// Sonnet : les mots-clés

pub fn consigne_mots(prompt: &str, business: &str, suivis: &[String], essayes: &[String], raison: Option<&str>, avant: &str) -> String {
    let suivis = if suivis.is_empty() { "aucun".into() } else { suivis.join(", ") };
    let faites = if essayes.is_empty() { String::new() } else { format!("\nRecherches déjà faites, qui ne suffisent pas : {}.", essayes.join(" ; ")) };
    let relance = match raison {
        Some(r) => format!("{faites}\nPourquoi : {r}\nPropose d'autres angles."),
        None => String::new(),
    };
    let avant = if avant.is_empty() { String::new() } else { format!("{avant}\n") };
    format!("\
Tu prépares une veille de vidéos courtes sur TikTok et Instagram pour {business}.
{avant}La demande : {prompt}
Mots-clés déjà suivis par la veille régulière : {suivis}.{relance}
Propose de 4 à {MOTS_MAX} recherches à taper dans la barre de recherche de TikTok ou d'Instagram pour trouver les vidéos qui
répondent à la demande : des requêtes courtes comme un humain les tape, dans la langue des vidéos visées, pas forcément en
français. Varie les angles et les deux plateformes. `pourquoi` : une phrase en français.")
}

pub fn schema_mots() -> Value {
    json!({
        "type": "object",
        "properties": {"mots": {"type": "array", "items": {
            "type": "object",
            "properties": {
                "mot": {"type": "string", "description": "La recherche telle qu'on la tape"},
                "plateforme": {"type": "string", "enum": PLATEFORMES.map(Plateforme::brut)},
                "pourquoi": {"type": "string"},
            },
            "required": ["mot", "plateforme", "pourquoi"],
            "additionalProperties": false,
        }}},
        "required": ["mots"],
        "additionalProperties": false,
    })
}

pub fn corps_mots(consigne: &str) -> Value {
    json!({
        "model": SONNET.modele,
        "max_tokens": MAX_TOKENS,
        "output_config": {"effort": "medium", "format": {"type": "json_schema", "schema": schema_mots()}},
        "messages": [{"role": "user", "content": consigne}],
    })
}

/// Les mots-clés d'une réponse de Sonnet, vides et doublons écartés, `MOTS_MAX` au plus.
pub fn lire_mots(o: &Map<String, Value>) -> Res<Vec<MotPropose>> {
    #[derive(Deserialize)]
    struct Reponse { mots: Vec<MotPropose> }
    let r: Reponse = serde_json::from_str(&texte(o)?).map_err(|e| format_err(format!("mots-clés illisibles : {e}")))?;
    let mut mots: Vec<MotPropose> = vec![];
    for mut m in r.mots {
        m.mot = m.mot.trim().to_string();
        let double = mots.iter().any(|x| x.plateforme == m.plateforme && x.mot.to_lowercase() == m.mot.to_lowercase());
        if !m.mot.is_empty() && PLATEFORMES.contains(&m.plateforme) && !double {
            mots.push(m);
        }
    }
    mots.truncate(MOTS_MAX);
    if mots.is_empty() {
        return Err(format_err("Sonnet n'a proposé aucun mot-clé"));
    }
    Ok(mots)
}

// Haiku : la boucle

pub fn systeme(prompt: &str, business: &str, maintenant: &Date, avant: &str) -> String {
    let suite = if avant.is_empty() { String::new() } else { format!("\
{avant}
C'est une suite : réponds à la nouvelle demande en t'appuyant sur ce qui précède. Si elle ne demande pas de nouvelles
vidéos (une question, un avis, un script), réponds directement sans outil ; sinon cherche, `{AUTRES_MOTS}` te donne des
mots-clés. Une vidéo déjà retenue se cite par son compte.
") };
    format!("\
Tu fais la veille de vidéos courtes de {business} sur TikTok et Instagram. Nous sommes le {}.
{suite}La demande : {prompt}

Ta méthode, avec tes outils :
- `{CHERCHER}` tape un mot-clé dans la recherche d'une plateforme et rend ses vidéos de moins d'un an, les plus récentes
  d'abord, au-dessus de `vues_min` vues ({VUES_MIN} par défaut), avec leurs stats ; `ratio` = vues / médiane du compte, quand on la connaît.
  {CHERCHES_MAX} recherches au plus, une recherche prend une minute.
- `{ANALYSER}` regarde une vidéo (images clés et ce qui est dit) : hook, format, structure, ce qui la distingue.
  {ANALYSES_MAX} analyses au plus : garde-les pour les vidéos qui semblent répondre à la demande.
- `{AUTRES_MOTS}` demande d'autres mots-clés quand les recherches ne rendent rien d'utile, {AUTRES_MOTS_MAX} fois au plus.
- `{RETENIR}` garde une vidéo qui répond à la demande, avec sa raison en une phrase en français. Retiens-en de 3 à 12, la
  meilleure d'abord, seulement des vidéos rendues par une recherche.
Une vidéo qui a marché : beaucoup de vues pour son compte, récente (à égalité, prends la plus récente), beaucoup de partages et d'enregistrements. Juge si
elle répond à la demande sur sa légende, et sur son analyse quand tu hésites.
Quand tu as fini, réponds en français, sans markdown, en 3 à 6 phrases : ce que tu as trouvé, les mécaniques qui
reviennent, la vidéo à tourner en premier.", maintenant.format("%Y-%m-%d"))
}

pub fn premier_message(mots: &[MotPropose]) -> String {
    let lignes: Vec<String> = mots.iter().map(|m| format!("- « {} » sur {} : {}", m.mot, m.plateforme.brut(), m.pourquoi)).collect();
    format!("Les mots-clés proposés :\n{}\nCommence par les recherches les plus prometteuses.", lignes.join("\n"))
}

pub fn outils() -> Value {
    let id = json!({"type": "string", "description": format!("L'id d'une vidéo rendue par {CHERCHER}")});
    let objet = |proprietes: Value, requis: &[&str]| {
        json!({"type": "object", "properties": proprietes, "required": requis, "additionalProperties": false})
    };
    json!([
        {"name": CHERCHER,
         "description": "Cherche un mot-clé sur TikTok ou Instagram et rend les vidéos de moins d'un an, les plus récentes d'abord, au-dessus de vues_min vues, avec leurs stats.",
         "input_schema": objet(json!({
             "mot": {"type": "string", "description": "La recherche telle qu'un humain la tape"},
             "plateforme": {"type": "string", "enum": PLATEFORMES.map(Plateforme::brut)},
             "vues_min": {"type": "integer", "description": format!("Le seuil de vues, {VUES_MIN} par défaut")},
         }), &["mot", "plateforme"])},
        {"name": ANALYSER,
         "description": "Analyse une vidéo : hook mot pour mot, type de hook, format, ses 5 blocs, ce qui la distingue, l'idée à tourner pour nous.",
         "input_schema": objet(json!({"id": id}), &["id"])},
        {"name": AUTRES_MOTS,
         "description": "Demande d'autres mots-clés quand ceux essayés ne rendent rien d'utile.",
         "input_schema": objet(json!({"raison": {"type": "string", "description": "Ce qui manque dans les résultats"}}), &["raison"])},
        {"name": RETENIR,
         "description": "Garde une vidéo qui répond à la demande ; la garder de nouveau change sa raison.",
         "input_schema": objet(json!({
             "id": id,
             "raison": {"type": "string", "description": "Pourquoi elle répond à la demande, en une phrase en français"},
         }), &["id", "raison"])},
    ])
}

pub fn corps_boucle(systeme: &str, messages: &[Value]) -> Value {
    json!({
        "model": HAIKU.modele,
        "max_tokens": MAX_TOKENS,
        // le cache suit la conversation : chaque tour relit le précédent à un dixième du prix
        "cache_control": {"type": "ephemeral"},
        "system": systeme,
        "tools": outils(),
        "messages": messages,
    })
}

/// Les appels d'outils d'une réponse, dans l'ordre.
pub fn appels(o: &Map<String, Value>) -> Vec<Appel> {
    o.get("content").and_then(Value::as_array).into_iter().flatten()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
        .filter_map(|b| Some(Appel {
            id: b.get("id")?.as_str()?.into(), nom: b.get("name")?.as_str()?.into(), entree: b.get("input").cloned().unwrap_or(Value::Null),
        }))
        .collect()
}

/// Un champ texte non vide d'une entrée d'outil.
pub fn champ(entree: &Value, cle: &str) -> Res<String> {
    entree.get(cle).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(String::from)
        .ok_or_else(|| non(format!("`{cle}` manquant")))
}

/// Le mot, la plateforme et le seuil de vues d'un appel à `chercher`.
pub fn entree_chercher(entree: &Value) -> Res<(String, Plateforme, i64)> {
    let mot = champ(entree, "mot")?;
    let brut = champ(entree, "plateforme")?;
    let p = Plateforme::depuis(&brut).filter(|p| PLATEFORMES.contains(p)).ok_or_else(|| non(format!("plateforme inconnue : {brut}")))?;
    let vues_min = match entree.get("vues_min") {
        None | Some(Value::Null) => VUES_MIN,
        Some(v) => v.as_i64().filter(|n| *n >= 0).ok_or_else(|| non("`vues_min` doit être un entier positif"))?,
    };
    Ok((mot, p, vues_min))
}

/// Une vidéo telle que Haiku la lit.
pub fn fiche(v: &VideoVeille) -> Value {
    json!({
        "id": v.id,
        "compte": v.compte,
        "publiee": v.publiee.map(|d| d.format("%Y-%m-%d").to_string()),
        "vues": v.vues,
        "likes": v.likes,
        "commentaires": v.commentaires,
        "partages": v.partages,
        "enregistrements": v.enregistrements,
        "ratio": v.ratio().map(|r| (r * 10.0).round() / 10.0),
        "dureeS": v.duree_s.map(f64::round),
        "legende": v.legende.as_deref().map(|l| l.chars().take(LEGENDE_MAX).collect::<String>()),
        "analysee": v.analyse.is_some(),
        "app": v.app_promue(),
        "langue": v.langue,
        "genre": v.genre,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lecture::Lectures;
    use crate::modeles_veille::Trouvaille;
    use crate::p;
    use chrono::{Duration, TimeZone, Utc};

    fn reponse(contenu: Value, arret: &str) -> Map<String, Value> {
        serde_json::from_value(json!({"stop_reason": arret, "content": contenu, "usage": {"input_tokens": 10, "output_tokens": 5}})).unwrap()
    }

    #[test]
    fn corps_des_deux_modeles() {
        let m = corps_mots(&consigne_mots("des vidéos scriptées", "Roleo", &[], &[], None, ""));
        assert_eq!(m["model"], SONNET.modele);
        // Sonnet 5.5 refuse `thinking: disabled` : sa réflexion reste adaptative
        assert!(m.get("thinking").is_none());
        assert_eq!(m["output_config"]["format"]["schema"]["properties"]["mots"]["items"]["additionalProperties"], false);
        let b = corps_boucle("systeme", &[json!({"role": "user", "content": "go"})]);
        assert_eq!(b["model"], HAIKU.modele);
        assert_eq!(b["cache_control"]["type"], "ephemeral");
        let noms: Vec<&str> = b["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(noms, [CHERCHER, ANALYSER, AUTRES_MOTS, RETENIR]);
        assert_eq!(b["tools"][0]["input_schema"]["properties"]["plateforme"]["enum"], json!(["tiktok", "instagram"]));
    }

    #[test]
    fn consigne_de_relance() {
        let c = consigne_mots("x", "Roleo", &["espagnol".into()], &["« a » sur TikTok : 0 vidéos".into()], Some("rien de scripté"), "");
        assert!(c.contains("Mots-clés déjà suivis par la veille régulière : espagnol."));
        assert!(c.contains("« a » sur TikTok : 0 vidéos"));
        assert!(c.contains("Pourquoi : rien de scripté"));
        assert!(!consigne_mots("x", "Roleo", &[], &[], None, "").contains("Pourquoi"));
        // une suite, sans recherche faite : la conversation passe, pas de liste vide
        let suite = consigne_mots("x", "Roleo", &[], &[], Some("plus récent"), "Avant, dans cette conversation :\n- Demande : a");
        assert!(suite.contains("- Demande : a\nLa demande : x") && !suite.contains("déjà faites"));
    }

    #[test]
    fn mots_de_sonnet() {
        let texte = json!({"mots": [
            {"mot": " app espagnol ", "plateforme": "tiktok", "pourquoi": "a"},
            {"mot": "App Espagnol", "plateforme": "tiktok", "pourquoi": "doublon"},
            {"mot": "app espagnol", "plateforme": "instagram", "pourquoi": "b"},
            {"mot": "  ", "plateforme": "tiktok", "pourquoi": "vide"},
        ]}).to_string();
        let mots = lire_mots(&reponse(json!([{"type": "text", "text": texte}]), "end_turn")).unwrap();
        assert_eq!(mots.iter().map(|m| (m.mot.as_str(), m.plateforme)).collect::<Vec<_>>(),
                   [("app espagnol", Plateforme::Tiktok), ("app espagnol", Plateforme::Instagram)]);
        let vide = json!({"mots": []}).to_string();
        assert!(lire_mots(&reponse(json!([{"type": "text", "text": vide}]), "end_turn")).is_err());
        assert!(lire_mots(&reponse(json!([{"type": "text", "text": texte}]), "max_tokens")).is_err());
        let youtube = json!({"mots": [{"mot": "a", "plateforme": "youtube", "pourquoi": "c"}]}).to_string();
        assert!(lire_mots(&reponse(json!([{"type": "text", "text": youtube}]), "end_turn")).is_err());
    }

    #[test]
    fn appels_et_entrees() {
        let o = reponse(json!([
            {"type": "thinking", "thinking": "", "signature": "s"},
            {"type": "text", "text": "Je cherche."},
            {"type": "tool_use", "id": "t1", "name": "chercher", "input": {"mot": "app", "plateforme": "tiktok"}},
            {"type": "tool_use", "id": "t2", "name": "retenir", "input": {"id": "tiktok:1", "raison": " r "}},
        ]), "tool_use");
        let a = appels(&o);
        assert_eq!(a.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["t1", "t2"]);
        assert_eq!(entree_chercher(&a[0].entree).unwrap(), ("app".into(), Plateforme::Tiktok, VUES_MIN));
        assert_eq!(champ(&a[1].entree, "raison").unwrap(), "r");
        assert!(entree_chercher(&json!({"mot": "a", "plateforme": "youtube"})).is_err());
        assert!(entree_chercher(&json!({"mot": "", "plateforme": "tiktok"})).is_err());
        assert!(entree_chercher(&json!({"mot": "a", "plateforme": "tiktok", "vues_min": -1})).is_err());
        assert_eq!(entree_chercher(&json!({"mot": "a", "plateforme": "instagram", "vues_min": 500})).unwrap().2, 500);
        assert!(champ(&json!({}), "id").is_err());
    }

    #[test]
    fn demande_en_base() {
        let s = Store::memoire().unwrap();
        assert!(creer(&s, "roleo", "   ", None).is_err());
        assert!(creer(&s, "roleo", &"x".repeat(PROMPT_MAX + 1), None).is_err());
        let premiere = creer(&s, "roleo", " trouve des vidéos ", None).unwrap();
        assert_eq!(premiere.prompt, "trouve des vidéos");
        // l'app quittée en route : la suivante la marque interrompue
        let d = creer(&s, "roleo", "encore", None).unwrap();
        let interrompue = s.demande_veille(&premiere.id).unwrap().unwrap();
        assert_eq!((interrompue.etat.as_str(), interrompue.erreur.as_deref()), (DemandeVeille::ECHOUEE, Some("interrompue")));

        s.noter_demande(&d.id, "Mots-clés : « a »").unwrap();
        s.noter_demande(&d.id, "Retenue : @x").unwrap();
        for (brut, compte) in [("1", "a"), ("2", "b")] {
            let mut t = Trouvaille::nouvelle(Plateforme::Tiktok, brut, compte);
            t.vues = Some(50_000);
            s.trouvaille(&t, "roleo", Some("app"), &d.debut).unwrap();
        }
        let ids: Vec<String> = s.videos_veille("order by v.compte", p![]).unwrap().into_iter().map(|v| v.id).collect();
        s.retenir_video(&d.id, &ids[1], "la meilleure").unwrap();
        s.retenir_video(&d.id, &ids[0], "une autre").unwrap();
        s.retenir_video(&d.id, &ids[1], "la meilleure, vraiment").unwrap();
        s.cout_veille(&format!("demande:{}", d.id), &(d.debut + Duration::seconds(5)), SONNET.modele, 1_000, 200, 0.004).unwrap();
        s.cout_veille("tiktok:9", &(d.debut - Duration::days(1)), HAIKU.modele, 1_000, 200, 0.5).unwrap();
        s.finir_demande(&d.id, &(d.debut + Duration::minutes(4)), Ok("Synthèse.")).unwrap();

        let b: Business = serde_json::from_str(r#"{"id": "roleo", "nom": "Roleo"}"#).unwrap();
        let l = Lectures::new(&s.db, vec![b]);
        let lue = l.demande_veille().unwrap().unwrap();
        assert_eq!(lue.id, d.id);
        assert_eq!(lue.journal, ["Mots-clés : « a »", "Retenue : @x"]);
        assert_eq!((lue.etat.as_str(), lue.synthese.as_deref()), (DemandeVeille::FINIE, Some("Synthèse.")));
        let retenues: Vec<(String, String)> = l.retenues_veille(&d.id).unwrap().into_iter().map(|(v, r)| (v.compte, r)).collect();
        assert_eq!(retenues, [("b".into(), "la meilleure, vraiment".into()), ("a".into(), "une autre".into())]);
        assert!((l.cout_demande(&lue).unwrap() - 0.004).abs() < 1e-9);
        let autre: Business = serde_json::from_str(r#"{"id": "app2", "nom": "App 2"}"#).unwrap();
        assert!(Lectures::new(&s.db, vec![autre]).demande_veille().unwrap().is_none());

        // la bibliothèque garde pour toujours ce qu'une demande a retenu, et oublie le reste après 30 jours
        for (brut, compte) in [("3", "vieille-retenue"), ("4", "vieille-oubliee")] {
            let mut t = Trouvaille::nouvelle(Plateforme::Tiktok, brut, compte);
            t.vues = Some(50_000);
            s.trouvaille(&t, "roleo", Some("app"), &(d.debut - Duration::days(100))).unwrap();
        }
        let vieille = s.videos_veille("where v.compte = 'vieille-retenue'", p![]).unwrap().remove(0);
        s.retenir_video(&d.id, &vieille.id, "toujours bonne").unwrap();
        let comptes: Vec<String> = l.videos_veille(30).unwrap().into_iter().map(|v| v.compte).collect();
        assert!(comptes.contains(&"vieille-retenue".to_string()) && !comptes.contains(&"vieille-oubliee".to_string()));
        assert!(comptes.contains(&"a".to_string()));

        // une suite : même conversation, et Claude relit ce qui s'est dit avant
        let suite = creer(&s, "roleo", "et en plus récent ?", Some(&d.id)).unwrap();
        assert_eq!(suite.fil, d.fil);
        assert!(creer(&s, "app2", "x", Some(&d.id)).is_err());
        assert!(creer(&s, "roleo", "x", Some("inconnue")).is_err());
        let contexte = avant(&l, &suite).unwrap();
        assert!(contexte.contains("- Demande : encore\n  Réponse : Synthèse.\n  Retenue : 2 @b sur TikTok : la meilleure, vraiment"));
        assert!(!contexte.contains("trouve des vidéos") && !contexte.contains("plus récent"));
        assert!(avant(&l, &d).unwrap().is_empty());
        let fils = l.fils_veille().unwrap();
        assert_eq!((fils[0].id.as_str(), fils[0].titre.as_str(), fils[0].demandes), (d.fil.as_str(), "encore", 2));
        assert_eq!(l.fil_veille(&d.fil).unwrap().len(), 2);
    }

    #[test]
    fn recherche_sans_resultat_ne_plante_pas() {
        let s = Store::memoire().unwrap();
        assert!(s.videos_veille_par_ids(&[]).unwrap().is_empty());
    }

    #[test]
    fn fiche_courte() {
        let s = Store::memoire().unwrap();
        let t0 = Utc.timestamp_opt(1_800_000_000, 0).unwrap();
        let mut t = Trouvaille::nouvelle(Plateforme::Tiktok, "1", "profe.ana");
        (t.vues, t.publiee, t.legende) = (Some(40_000), Some(t0), Some("é".repeat(400)));
        s.trouvaille(&t, "roleo", None, &t0).unwrap();
        s.base(Plateforme::Tiktok, "profe.ana", Some(10_000), 20, None, &t0).unwrap();
        let f = fiche(&s.videos_veille("", p![]).unwrap()[0]);
        assert_eq!(f["ratio"], 4.0);
        assert_eq!(f["publiee"], t0.format("%Y-%m-%d").to_string());
        assert_eq!(f["legende"].as_str().unwrap().chars().count(), LEGENDE_MAX);
        assert_eq!(f["analysee"], false);
        assert!(systeme("x", "Roleo", &t0, "").contains(&format!("Nous sommes le {}.", t0.format("%Y-%m-%d"))));
    }
}
