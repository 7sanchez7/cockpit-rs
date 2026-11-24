// Veille : la bank des vidéos trouvées, et la conversation avec Claude qui l'enrichit. La page ne défile pas, ses
// colonnes défilent. Tout se lit par `veille` et `chat_veille` (src/ecrans/veille.rs) ; ce module n'affiche que ce
// qu'elles rendent.
// Bank : la grille à gauche, les filtres à droite ; le survol d'une vidéo montre sa carte, le clic ouvre sa fiche
// (veille/fiche.js). Les mots-clés qui nourrissent la bank s'ouvrent dans un popover.
// Claude : les conversations à gauche, celle ouverte à droite ; chaque message lance `demander_veille`.

import { enTetePanneau, h, icone, pilule, point, rendu, segmente, surtitre, vide } from "../composants.js";
import * as F from "../format.js";
import { bouton, caseVideo, choixBusiness, menuContextuel, popover, remplacer } from "./videos/commun.js";
import { ouvrirFiche, recalerCarte, survolCarte } from "./veille/fiche.js";

const VUES = [
  { id: "bank", libelle: "Bank" },
  { id: "claude", libelle: "Claude" },
];
const TRIS = [
  { id: "ratio", libelle: "× médiane" },
  { id: "vues", libelle: "Vues" },
  { id: "vitesse", libelle: "Vues/h" },
  { id: "engagement", libelle: "Engagement" },
  { id: "recentes", libelle: "Récentes" },
  { id: "ajoutees", libelle: "Ajoutées" },
];
const EXEMPLES = [
  "Trouve des vidéos scriptées qui font la promo d'une app d'apprentissage",
  "Les sketchs sur l'apprentissage de l'espagnol qui ont percé ce mois-ci",
  "Des hooks visuels forts sur les langues, publiés il y a moins d'un an",
];
const SEUIL = 20000;
// vidéos chargées à la fois, la grille en demande d'autres en approchant du bas
const PAGE = 120;
const OPTIONS_VISIBLES = 8;
const ATTENTE_RECHERCHE = 250;
// en deçà, la conversation est « en bas » et y reste quand un message arrive
const PRES_DU_BAS = 80;

const INITIAL = { vue: "bank", tri: "ratio", choix: {}, recherche: "", limite: PAGE, fil: null };
let etat = { ...INITIAL };
let dernier = null;
let tour = 0;
// la dernière lecture de la bank : le popover des mots-clés s'en sert
let bank = null;
// les facettes dépliées et les étapes ouvertes, d'un rendu à l'autre
const ouvertes = new Set();
const depliees = new Set();
// les cases déjà faites, par vue et par vidéo : une vidéo inchangée garde sa case (et sa miniature chargée)
let cases = new Map();
let observateur = null;
let haut = false;
let bas = false;
let businessNouveau = null;

function maj(changement = {}) {
  Object.assign(etat, changement);
  rendu.calme = true;
  return ecran.rendre(dernier.racine, { ...dernier.ctx, rafraichi: true });
}

/** Un filtre, un tri ou une recherche change : la grille repart du haut. */
function filtrer(changement) {
  haut = true;
  return maj({ ...changement, limite: PAGE });
}

// champs gardés d'un rendu à l'autre : la saisie en cours, son focus et sa sélection survivent au rafraîchissement

let attente = null;
const recherche = h("input", {
  class: "champ-recherche",
  type: "search",
  placeholder: "Légende, compte, app, hook…",
  spellcheck: false,
  oninput: () => {
    clearTimeout(attente);
    attente = setTimeout(() => filtrer({ recherche: recherche.value.trim() }), ATTENTE_RECHERCHE);
  },
  onkeydown: (e) => {
    if (e.key !== "Escape" || !recherche.value) return;
    e.stopPropagation();
    viderRecherche();
  },
});

function viderRecherche() {
  clearTimeout(attente);
  recherche.value = "";
  return filtrer({ recherche: "" });
}

const saisie = h("textarea", {
  class: "saisie-chat",
  rows: 1,
  placeholder: "Demande des vidéos à Claude",
  oninput: () => ajuster(),
  onkeydown: (e) => {
    if (e.key !== "Enter" || e.shiftKey || e.isComposing) return;
    e.preventDefault();
    envoyer();
  },
});

/** La saisie grandit avec son texte, jusqu'à sa hauteur maximale (CSS). */
function ajuster() {
  saisie.style.height = "auto";
  saisie.style.height = saisie.scrollHeight + "px";
}

/** Remplace la page sans perdre le champ en cours de saisie. */
function poser(racine, ...enfants) {
  const actif = document.activeElement;
  const garde = actif === recherche || actif === saisie ? { el: actif, debut: actif.selectionStart, fin: actif.selectionEnd } : null;
  remplacer(racine, ...enfants);
  if (garde?.el.isConnected) {
    garde.el.focus({ preventScroll: true });
    garde.el.setSelectionRange(garde.debut, garde.fin);
  }
}

// cases

/** Une vidéo : le survol montre sa carte, un clic ouvre sa fiche ; refaite seulement si elle a changé. */
function caseGrille(v, ctx, { portee, legende, lignes, faites }) {
  const cle = `${portee}|${v.id}`;
  const signature = JSON.stringify([v, legende]);
  faites.set(cle, cases.get(cle)?.signature === signature ? cases.get(cle) : { signature, el: nouvelleCase(v, ctx, legende, lignes) });
  return faites.get(cle).el;
}

function nouvelleCase(v, ctx, legende, lignes) {
  const el = caseVideo({
    cles: v.cles,
    plateforme: v.plateforme,
    vues: v.vues,
    badge: v.badge,
    badgeCouleur: v.outlier ? "var(--succes)" : "var(--texte)",
    legende,
    lignes,
    clic: () => ouvrirFiche(v.id, ctx),
  });
  if (v.app) el.querySelector(".legende").before(h("span", { class: "app-case", title: "L'app promue" }, v.app));
  survolCarte(el.querySelector(".affiche"), v);
  return menuContextuel(el, [{ libelle: `Ouvrir sur ${v.nomPlateforme}`, faire: () => ctx.lien(v.url) }]);
}

// bank

function compteur(d) {
  return h(
    "div",
    { class: "compteur-bank" },
    h("span", { class: "total" }, F.entier(d.total)),
    d.total > 1 ? " vidéos" : " vidéo",
    d.total !== d.bank && h("span", { class: "sur" }, ` sur ${F.entier(d.bank)}`),
  );
}

/** Les choix avec `valeurs` pour le filtre `cle` ; une liste vide retire le filtre. */
function avecChoix(cle, valeurs) {
  const choix = { ...etat.choix, [cle]: valeurs };
  if (valeurs.length === 0) delete choix[cle];
  return choix;
}

function jetons(d) {
  const retirer = (cle, valeur) => filtrer({ choix: avecChoix(cle, (etat.choix[cle] ?? []).filter((x) => x !== valeur)) });
  const jeton = (libelle, faire) => h("button", { class: "jeton-filtre", title: "Retirer ce filtre", onclick: faire }, libelle, icone("fermer"));
  if (d.actifs.length === 0 && !etat.recherche) return null;
  return h(
    "div",
    { class: "marge jetons" },
    etat.recherche && jeton(`« ${etat.recherche} »`, viderRecherche),
    d.actifs.map((a) => jeton(a.libelle, () => retirer(a.cle, a.valeur))),
    h(
      "button",
      {
        class: "lien-filtre",
        onclick: () => {
          clearTimeout(attente);
          recherche.value = "";
          return filtrer({ choix: {}, recherche: "" });
        },
      },
      "Tout effacer",
    ),
  );
}

function panneauGrille(d, ctx, faites) {
  const defile = h("div", { class: "defile", "data-defile": "bank" });
  if (d.videos.length === 0) {
    defile.append(vide(d.bank === 0 ? "La bank est vide : ajoute un mot-clé ou demande des vidéos à Claude." : "Aucune vidéo pour ces filtres."));
  } else {
    defile.append(h("div", { class: "grille-videos" }, d.videos.map((v) => caseGrille(v, ctx, { portee: "bank", legende: v.legende, lignes: 1, faites }))));
  }
  observateur?.disconnect();
  if (d.suite) {
    const sentinelle = h("div", { class: "sentinelle" });
    defile.append(sentinelle);
    observateur = new IntersectionObserver(
      (entrees) => {
        if (!entrees.some((e) => e.isIntersecting)) return;
        observateur.disconnect();
        maj({ limite: etat.limite + PAGE });
      },
      { root: defile, rootMargin: "0px 0px 100% 0px" },
    );
    observateur.observe(sentinelle);
  }
  return h(
    "section",
    { class: "panneau-videos verre" },
    h("div", { class: "marge" }, enTetePanneau(compteur(d), segmente(TRIS, etat.tri, (id) => filtrer({ tri: id })))),
    jetons(d),
    defile,
  );
}

function basculer(f, o) {
  const actuels = etat.choix[f.cle] ?? [];
  const valeurs = o.choisi ? actuels.filter((x) => x !== o.valeur) : f.unique ? [o.valeur] : [...actuels, o.valeur];
  return filtrer({ choix: avecChoix(f.cle, valeurs) });
}

/** Un filtre du panneau : ses options en puces, chacune avec les vidéos qu'elle donnerait. */
function facette(f) {
  const ouverte = ouvertes.has(f.cle);
  const visibles = ouverte ? f.options : f.options.filter((o, i) => i < OPTIONS_VISIBLES || o.choisi);
  const choisies = f.options.some((o) => o.choisi);
  const el = h(
    "section",
    { class: "facette" },
    h(
      "div",
      { class: "tete-facette" },
      surtitre(f.titre),
      choisies && h("button", { class: "lien-filtre", onclick: () => filtrer({ choix: avecChoix(f.cle, []) }) }, "Effacer"),
    ),
    h(
      "div",
      { class: "options" + (f.unique ? " unique" : "") },
      visibles.map((o) =>
        h(
          "button",
          { class: "option" + (o.choisi ? " choisie" : ""), disabled: o.n === 0 && !o.choisi, onclick: () => basculer(f, o) },
          h("span", { class: "libelle" }, o.libelle),
          h("span", { class: "n" }, F.compact(o.n)),
        ),
      ),
    ),
    f.options.length > OPTIONS_VISIBLES &&
      h(
        "button",
        {
          class: "lien-filtre",
          onclick: () => {
            if (ouverte) ouvertes.delete(f.cle);
            else ouvertes.add(f.cle);
            el.replaceWith(facette(f));
          },
        },
        ouverte ? "Moins" : `Tout voir (${f.options.length})`,
      ),
  );
  return el;
}

function panneauFiltres(d) {
  return h(
    "aside",
    { class: "panneau-filtres verre" },
    h(
      "label",
      { class: "recherche-bank" },
      icone("loupe"),
      recherche,
      etat.recherche && h("button", { class: "vider", title: "Effacer la recherche", onclick: viderRecherche }, icone("fermer")),
    ),
    h("div", { class: "defile", "data-defile": "filtres" }, d.facettes.map(facette)),
  );
}

// mots-clés

// la liste du popover ouvert, et de quoi le fermer
let mots = null;

/** Le popover des mots-clés : la liste, l'ajout, le curseur ; la liste se relit à chaque rendu de la bank. */
function popoverMots(ancre, ctx) {
  popover(ancre, (fermer) => {
    mots = { liste: h("div", { class: "liste-mots" }), fermer };
    remplirMots(ctx);
    return h(
      "div",
      { class: "popover-mots" },
      h("div", { class: "titre-popover" }, "Mots-clés"),
      h("div", { class: "note" }, "Ce que la veille cherche sur TikTok et Instagram quand tu la lances ; les vidéos trouvées restent dans la bank."),
      mots.liste,
      formulaireMot(ctx),
      curseur(bank.curseur, ctx),
    );
  });
}

function remplirMots(ctx) {
  mots?.liste.replaceChildren(...(bank.mots.length ? bank.mots.map((m) => ligneMot(m, ctx)) : [h("div", { class: "attente" }, "Aucun mot-clé.")]));
}

function ligneMot(m, ctx) {
  const cle = { business: m.business, mot: m.mot };
  const voir = () => {
    mots.fermer();
    return filtrer({ choix: { source: [`mot:${m.mot}`] } });
  };
  return h(
    "div",
    { class: "ligne-mot" + (m.actif ? "" : " en-pause"), title: m.aide },
    h("button", { class: "mot", title: "Voir ses vidéos dans la bank", onclick: voir }, m.mot),
    h("span", { class: "n" }, F.entier(m.videos)),
    h(
      "button",
      { class: "action-mot", title: m.actif ? "Mettre en pause" : "Reprendre", onclick: () => ctx.appeler("activer_mot", { ...cle, actif: !m.actif }) },
      icone(m.actif ? "pause" : "lecture"),
    ),
    h("button", { class: "action-mot danger", title: "Retirer", onclick: () => ctx.appeler("retirer_mot", cle) }, icone("fermer")),
  );
}

/** L'ajout d'un mot-clé : le mot, le seuil de vues, le business ; le popover reste ouvert pour en ajouter d'autres. */
function formulaireMot(ctx) {
  let business = ctx.business ?? ctx.businesses[0]?.id;
  const ajouterBouton = bouton("Ajouter", () => ajouter());
  const pret = () => champ.value.trim() !== "" && business != null;
  const champ = h("input", {
    class: "champ-ajout",
    placeholder: "apprendre l'espagnol",
    oninput: () => (ajouterBouton.disabled = !pret()),
    onkeydown: (e) => e.key === "Enter" && ajouter(),
  });
  const seuil = h("input", { class: "champ-ajout seuil", type: "number", min: 0, step: 1000, value: SEUIL, placeholder: "Seuil", title: "Vues minimum pour garder une vidéo" });
  ajouterBouton.disabled = true;
  async function ajouter() {
    if (!pret()) return;
    await ctx.appeler("ajouter_mot", { business, mot: champ.value, vuesMin: Math.round(Number(seuil.value) || 0) });
    champ.value = "";
    ajouterBouton.disabled = true;
  }
  return h(
    "div",
    { class: "formulaire-ajout" },
    h("div", { class: "champs" }, champ, seuil, ajouterBouton),
    choixBusiness(ctx, business, (id) => (business = id)),
  );
}

/** Les vidéos gardées par mot-clé et par plateforme ; le passage suivant les lit. */
function curseur(c, ctx) {
  const valeur = h("span", { class: "valeur" }, c.valeur);
  const remplir = () => champ.style.setProperty("--rempli", `${((champ.value - c.min) / (c.max - c.min)) * 100}%`);
  const champ = h("input", {
    type: "range",
    min: c.min,
    max: c.max,
    step: c.pas,
    value: c.valeur,
    oninput: () => {
      valeur.textContent = champ.value;
      remplir();
    },
    onchange: () => ctx.appeler("regler_videos", { nombre: Number(champ.value) }),
  });
  remplir();
  return h("label", { class: "curseur", title: "Vidéos gardées par mot-clé et par plateforme à chaque passage, les plus récentes d'abord" }, "Vidéos par mot-clé", champ, valeur);
}

// Claude

const ICONES_ETAPES = { pense: "etincelle", mots: "etiquette", cherche: "loupe", trouve: "videos", image: "image", analyse: "veille", retenue: "coche", echec: "alerte", info: "info" };

/** Les conversations ; le survol d'une ligne montre la poubelle, sauf pendant que Claude y cherche. */
function listeFils(d, ctx) {
  return h(
    "aside",
    { class: "panneau-fils verre" },
    h("button", { class: "nouveau-fil" + (d.fil == null ? " choisi" : ""), onclick: () => ouvrirFil(null) }, icone("plus"), "Nouvelle conversation"),
    h(
      "div",
      { class: "defile", "data-defile": "fils" },
      d.fils.length === 0
        ? h("div", { class: "attente" }, "Tes conversations avec Claude restent ici.")
        : d.fils.map((f) =>
            h(
              "div",
              { class: "ligne-conversation" },
              h(
                "button",
                { class: "fil" + (f.id === d.fil ? " choisi" : ""), title: f.titre, onclick: () => ouvrirFil(f.id) },
                h("span", { class: "titre" }, f.titre),
                h(
                  "span",
                  { class: "meta" },
                  f.etat === "en_cours" && point("var(--accent)", { mini: true, enCours: true, aide: "Claude cherche" }),
                  f.etat === "echouee" && point("var(--danger)", { mini: true, aide: "Échouée" }),
                  f.meta,
                ),
              ),
              f.etat !== "en_cours" &&
                h("button", { class: "action-mot danger retirer-fil", title: "Supprimer la conversation", onclick: () => ctx.appeler("supprimer_fil_veille", { fil: f.id }) }, icone("poubelle")),
            ),
          ),
    ),
  );
}

function ouvrirFil(fil) {
  bas = true;
  return maj({ fil });
}

function ligneEtape(e) {
  return h("div", { class: "pas-deroule " + e.genre }, icone(ICONES_ETAPES[e.genre] ?? "info"), h("span", { class: "texte" }, e.texte));
}

/** Le journal d'une demande : l'étape en cours en clair, les autres repliées. */
function etapes(dm) {
  const enCours = dm.etat === "en_cours";
  const passees = enCours ? dm.etapes.slice(0, -1) : dm.etapes;
  const courante = enCours ? (dm.etapes.at(-1)?.texte ?? "Démarrage") : null;
  const ouvert = depliees.has(dm.id);
  const el = h(
    "div",
    { class: "deroule" },
    passees.length > 0 &&
      h(
        "button",
        {
          class: "bascule-deroule" + (ouvert ? " ouvert" : ""),
          onclick: () => {
            if (ouvert) depliees.delete(dm.id);
            else depliees.add(dm.id);
            el.replaceWith(etapes(dm));
          },
        },
        icone("chevron"),
        `${passees.length} étape${passees.length > 1 ? "s" : ""}`,
      ),
    ouvert && h("div", { class: "liste-deroule" }, passees.map(ligneEtape)),
    courante && h("div", { class: "pas-deroule courante tourne" }, icone("relancer"), h("span", { class: "texte" }, courante)),
  );
  return el;
}

/** `relancable` : la dernière demande du fil, en échec, quand rien ne tourne. */
function echange(dm, ctx, faites, relancable) {
  const n = dm.retenues.length;
  return h(
    "div",
    { class: "echange", "data-demande": dm.id },
    h("div", { class: "bulle-moi" }, dm.prompt),
    h(
      "div",
      { class: "reponse" },
      h("div", { class: "auteur" }, h("span", { class: "avatar-claude" }, icone("etincelle")), "Claude", h("span", { class: "meta" }, dm.meta)),
      etapes(dm),
      dm.synthese && h("div", { class: "synthese" }, dm.synthese),
      n > 0 &&
        h(
          "div",
          { class: "retenues" },
          surtitre(`${n} vidéo${n > 1 ? "s" : ""} gardée${n > 1 ? "s" : ""} dans la bank`),
          h("div", { class: "grille-videos" }, dm.retenues.map(({ case: v, raison }) => caseGrille(v, ctx, { portee: dm.id, legende: raison, lignes: 3, faites }))),
        ),
      dm.etat === "finie" && n === 0 && !dm.synthese && h("div", { class: "attente" }, "Aucune vidéo retenue."),
      dm.erreur && h("div", { class: "erreur" }, dm.erreur),
      relancable && h("div", { class: "relance" }, pilule("Relancer", () => relancer(dm.prompt), { icone: "relancer" })),
    ),
  );
}

function accueilChat() {
  return h(
    "div",
    { class: "accueil-chat" },
    h("span", { class: "avatar-claude grand" }, icone("etincelle")),
    h("div", { class: "h2" }, "Quelles vidéos cherches-tu ?"),
    h("div", { class: "note" }, "Sonnet choisit les mots-clés, Haiku cherche sur TikTok et Instagram, analyse et garde les meilleures dans la bank. Quelques minutes, quelques centimes."),
    h(
      "div",
      { class: "exemples" },
      EXEMPLES.map((t) =>
        h(
          "button",
          {
            class: "exemple survolable",
            onclick: () => {
              saisie.value = t;
              ajuster();
              saisie.focus();
            },
          },
          t,
        ),
      ),
    ),
  );
}

let chat = null;

/** Lance une demande dans le fil ouvert, ou un nouveau fil ; rend le fil, `null` si rien n'est parti. */
async function demander(prompt) {
  const { d, ctx } = chat ?? {};
  if (!d || !prompt || d.occupe) return null;
  const business = d.business ?? businessNouveau ?? ctx.business ?? ctx.businesses[0]?.id;
  return ctx.appeler("demander_veille", { business, prompt, fil: d.fil });
}

async function envoyer() {
  const fil = await demander(saisie.value.trim());
  if (fil == null) return;
  saisie.value = "";
  ajuster();
  return ouvrirFil(fil);
}

async function relancer(prompt) {
  const fil = await demander(prompt);
  if (fil != null) return ouvrirFil(fil);
}

function barreSaisie(d, ctx) {
  saisie.maxLength = d.promptMax;
  saisie.placeholder = d.fil ? "Demande la suite à Claude" : "Demande des vidéos à Claude";
  if (d.fil == null) businessNouveau ??= ctx.business ?? ctx.businesses[0]?.id;
  return h(
    "div",
    { class: "barre-saisie" },
    h(
      "div",
      { class: "cadre-saisie" },
      saisie,
      h(
        "div",
        { class: "pied-saisie" },
        d.fil == null && choixBusiness(ctx, businessNouveau, (id) => (businessNouveau = id)),
        h("span", { class: "note" }, d.occupe ? "La veille tourne : envoie quand elle a fini." : "Entrée pour envoyer, Maj+Entrée pour une nouvelle ligne"),
        h("button", { class: "envoyer", title: "Envoyer", disabled: d.occupe, onclick: envoyer }, icone("envoyer")),
      ),
    ),
  );
}

function vueClaude(d, ctx, faites) {
  const fil = h("div", { class: "defile fil-chat", "data-defile": "chat" }, d.demandes.length === 0 ? accueilChat() : h("div", { class: "messages" }, d.demandes.map((dm, i) => echange(dm, ctx, faites, i === d.demandes.length - 1 && dm.etat === "echouee" && !d.occupe))));
  return h("div", { class: "veille-chat remplit" }, listeFils(d, ctx), h("section", { class: "panneau-chat verre" }, fil, barreSaisie(d, ctx)));
}

// page

/** Les outils de l'en-tête, les mêmes dans les deux vues ; reposés seulement quand ils changent, pour que la bascule
 *  Bank/Claude glisse sans que rien ne bouge. */
let outilsPoses = null;

function poserOutils(ctx) {
  const tourne = ctx.enCours.includes("veille");
  const cle = `${tourne}|${bank.mots.length}`;
  if (outilsPoses?.cle === cle && outilsPoses.el.isConnected) return;
  const vues = segmente(VUES, etat.vue, (id) => {
    bas = id === "claude";
    return maj({ vue: id });
  });
  const ouvrirMots = (e) => popoverMots(e.currentTarget, ctx);
  // les deux libellés dans la même case : la pilule garde sa largeur quand la veille démarre
  const lancer = h(
    "button",
    { class: "pilule lancer" + (tourne ? " tourne" : ""), disabled: tourne, onclick: () => ctx.appeler("relancer", { source: "veille" }) },
    icone("relancer"),
    h("span", { class: "libelles" }, h("span", { class: tourne ? "cache" : null }, "Lancer la veille"), h("span", { class: tourne ? null : "cache" }, "Veille en cours")),
  );
  ctx.page.outils(vues, pilule(["Mots-clés", h("span", { class: "nombre-pilule" }, bank.mots.length)], ouvrirMots, { icone: "etiquette", aide: "Les mots-clés qui nourrissent la bank" }), lancer);
  outilsPoses = { cle, el: vues };
}

const ecran = {
  titre: "Veille",
  sousTitre: "La bank",
  defile: false,
  async rendre(racine, ctx) {
    if (!ctx.rafraichi) {
      etat = { ...INITIAL };
      cases = new Map();
      ouvertes.clear();
      recherche.value = "";
      businessNouveau = null;
    }
    dernier = { racine, ctx };
    const n = ++tour;
    // la bank se lit toujours : le compte des mots-clés et leur popover s'en servent
    const d = await ctx.appeler("veille", { business: ctx.business, choix: etat.choix, recherche: etat.recherche, tri: etat.tri, limite: etat.limite });
    const c = etat.vue === "claude" ? await ctx.appeler("chat_veille", { business: ctx.business, fil: etat.fil }) : null;
    if (n !== tour) return;
    bank = d;
    if (mots?.liste.isConnected) remplirMots(ctx);
    else mots = null;
    const faites = new Map();
    let vue;
    if (c) {
      chat = { d: c, ctx };
      if (c.fil !== etat.fil) etat.fil = c.fil;
      ctx.page.sousTitre(c.fils.length ? F.nombre(c.fils.length, "conversation") : "Aucune conversation");
      const ancien = racine.querySelector("[data-defile=chat]");
      const enBas = !ancien || ancien.scrollHeight - ancien.scrollTop - ancien.clientHeight < PRES_DU_BAS;
      vue = vueClaude(c, ctx, faites);
      poserOutils(ctx);
      poser(racine, vue);
      ajuster();
      const fil = racine.querySelector("[data-defile=chat]");
      if (bas || enBas) fil.scrollTop = fil.scrollHeight;
    } else {
      chat = null;
      ctx.page.sousTitre(d.sousTitre);
      vue = h("div", { class: "veille-bank remplit" }, panneauGrille(d, ctx, faites), panneauFiltres(d));
      poserOutils(ctx);
      poser(racine, vue);
      if (haut) racine.querySelector("[data-defile=bank]").scrollTop = 0;
    }
    cases = faites;
    haut = bas = false;
    recalerCarte();
  },
};

export default ecran;
