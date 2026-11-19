// Réglages : business, clés, notifications, collecte et sessions, portée de Reglages.swift.
// Un onglet à la fois, en formulaire groupé ; Business et Clés gardent leur saisie quand la base change.

import { h, segmente, surtitre, entree, teintes, interrupteur } from "../composants.js";

const ONGLETS = [
  { id: "business", libelle: "Business" },
  { id: "cles", libelle: "Clés" },
  { id: "notifs", libelle: "Notifications" },
  { id: "collecte", libelle: "Collecte" },
  { id: "sessions", libelle: "Sessions" },
];
// les onglets qui se relisent quand la base change ; les autres gardent la saisie en cours
const VIVANTS = new Set(["notifs", "collecte", "sessions"]);

const CHAMPS_SOURCES = [
  ["appStoreId", "App Store, id de l'app"],
  ["ascIssuer", "App Store Connect, issuer"],
  ["ascVendor", "App Store Connect, numéro de fournisseur", "8…"],
  ["playPackage", "Play Store, paquet", "com.exemple.app"],
  ["playBucket", "Play Console, bucket des rapports", "pubsite_prod_rev_…"],
  ["firebaseProject", "Firebase, projet"],
  ["posthogHost", "PostHog, hôte", "https://eu.posthog.com"],
  ["contenuDir", "Fiches de contenu, dossier"],
];
const NEUF = { id: "", nom: "", lettre: "", teinte: "menthe", appStoreId: "", ascIssuer: "", ascVendor: "", playPackage: "", playBucket: "", firebaseProject: "", posthogHost: "", contenuDir: "", pays: "", genre: "6017", objectif: "10000" };

// l'état de l'écran, gardé d'un rendu à l'autre
const vue = { onglet: "business", choisi: undefined, brouillon: null, message: null, supprimer: false };

// briques du formulaire

function groupe(titre, lignes, pied) {
  return h("section", { class: "groupe-reglages" }, titre && surtitre(titre), h("div", { class: "lignes verre" }, lignes), pied && h("div", { class: "pied" }, pied));
}

// sans libellé, les contrôles se calent à gauche (un bouton seul sur sa ligne)
function ligne(libelle, ...controles) {
  return h("div", { class: libelle == null ? "ligne-reglage seule" : "ligne-reglage" }, libelle != null && h("span", { class: "libelle" }, libelle), h("div", { class: "controles" }, controles));
}

function bouton(texte, faire, { principal, danger, inactif } = {}) {
  return h("button", { class: ["bouton-reglage", principal && "principal", danger && "danger"].filter(Boolean).join(" "), disabled: inactif, onclick: faire }, texte);
}

function champ(valeur, quandChange, { aide, secret, valider } = {}) {
  return h("input", {
    class: "champ",
    type: secret ? "password" : "text",
    value: valeur,
    placeholder: aide,
    spellcheck: "false",
    autocomplete: "off",
    oninput: (e) => quandChange(e.target.value),
    onkeydown: (e) => valider && e.key === "Enter" && valider(),
  });
}

function liste(options, choisi, quandChange) {
  return h(
    "select",
    { class: "champ", onchange: (e) => quandChange(e.target.value) },
    options.map((o) => h("option", { value: o.id, selected: o.id === choisi }, o.libelle)),
  );
}

function message() {
  if (!vue.message) return null;
  return h("span", { class: vue.message.erreur ? "message erreur" : "message" }, vue.message.texte);
}

// le business réglé dans Business et Clés : celui de la barre du haut, sinon le premier

function choixBusiness(ctx, brouillons, { nouveau = false } = {}) {
  const options = brouillons.map((b) => ({ id: b.id, libelle: b.nom }));
  if (vue.choisi === null && nouveau) options.push({ id: "", libelle: "Nouveau" });
  return liste(options, vue.choisi ?? "", (id) => {
    vue.choisi = id;
    vue.brouillon = null;
    vue.message = null;
    vue.supprimer = false;
    afficher(ctx);
  });
}

function premierChoix(ctx, brouillons) {
  if (vue.choisi === undefined || (vue.choisi && !brouillons.some((b) => b.id === vue.choisi))) {
    vue.choisi = brouillons.find((b) => b.id === ctx.business)?.id ?? brouillons[0]?.id ?? null;
  }
}

// onglets

async function business(ctx) {
  const brouillons = await ctx.appeler("reglages_business");
  premierChoix(ctx, brouillons);
  const b = (vue.brouillon ??= { ...(brouillons.find((x) => x.id === vue.choisi) ?? NEUF) });
  const nouveau = vue.choisi === null;
  const regler = (cle) => (v) => {
    b[cle] = v;
    if (vue.message && !vue.message.erreur) vue.message = null;
  };
  const enregistrer = async () => {
    try {
      const r = await ctx.appeler("enregistrer_business", { brouillon: b, nouveau });
      vue.choisi = r.id;
      vue.brouillon = r;
      vue.message = { texte: "Enregistré" };
    } catch (e) {
      vue.message = { texte: e.message, erreur: true };
    }
    afficher(ctx);
  };
  const supprimer = async () => {
    try {
      await ctx.appeler("supprimer_business", { id: vue.choisi });
      vue.choisi = undefined;
      vue.brouillon = null;
      vue.message = null;
    } catch (e) {
      vue.message = { texte: e.message, erreur: true };
    }
    vue.supprimer = false;
    afficher(ctx);
  };
  const f = (cle, aide) => champ(b[cle], regler(cle), { aide, valider: enregistrer });

  const actions = vue.supprimer
    ? h(
        "div",
        { class: "confirmation" },
        h("div", { class: "textes" }, h("span", { class: "h2" }, `Supprimer ${b.nom} ?`), h("span", { class: "pied" }, "Le business disparaît du cockpit. Ses chiffres déjà collectés restent dans la base.")),
        bouton("Annuler", () => ((vue.supprimer = false), afficher(ctx))),
        bouton("Supprimer", supprimer, { danger: true }),
      )
    : h(
        "div",
        { class: "actions" },
        !nouveau && bouton("Supprimer", () => ((vue.supprimer = true), afficher(ctx)), { danger: true, inactif: brouillons.length < 2 }),
        message(),
        h("span", { class: "espace" }),
        bouton("Enregistrer", enregistrer, { principal: true }),
      );

  return [
    groupe(null, [
      ligne("Business", choixBusiness(ctx, brouillons, { nouveau: true })),
      ligne(null, bouton("Nouveau business", () => ((vue.choisi = null), (vue.brouillon = null), (vue.message = null), afficher(ctx)))),
    ]),
    groupe("Identité", [
      ligne("Nom", f("nom")),
      ligne("Lettre", f("lettre")),
      ligne("Teinte", liste(teintes.map((t) => ({ id: t.cle, libelle: t.nom })), b.teinte, regler("teinte"))),
    ]),
    groupe("Sources", CHAMPS_SOURCES.map(([cle, libelle, aide]) => ligne(libelle, f(cle, aide))), "Un champ vide coupe la source pour ce business."),
    groupe("Classements et objectif", [ligne("Pays suivis", f("pays", "fr, es, us")), ligne("Catégorie App Store", f("genre")), ligne("Objectif mensuel (€)", f("objectif"))]),
    actions,
  ];
}

// les saisies de clés, jamais renvoyées par Rust ; vidées dès qu'elles sont rangées
const saisies = new Map();

async function cles(ctx) {
  const brouillons = await ctx.appeler("reglages_business");
  premierChoix(ctx, brouillons);
  if (vue.choisi === null) vue.choisi = brouillons[0]?.id ?? null;
  const d = vue.choisi ? await ctx.appeler("reglages_cles", { business: vue.choisi }) : null;
  const agir = async (cmd, args) => {
    try {
      await ctx.appeler(cmd, { business: vue.choisi, ...args });
      vue.message = null;
    } catch (e) {
      vue.message = { texte: e.message, erreur: true };
    }
    afficher(ctx);
  };
  const ranger = (nom) => {
    const valeur = (saisies.get(nom) ?? "").trim();
    if (!valeur) return;
    saisies.delete(nom);
    agir("ranger_cle", { nom, valeur });
  };
  const scrappeur = async (cmd, args) => {
    try {
      await ctx.appeler(cmd, args);
      vue.message = null;
    } catch (e) {
      vue.message = { texte: e.message, erreur: true };
    }
    afficher(ctx);
  };
  const rangerScrappeur = () => {
    const valeur = (saisies.get("__scrappeur") ?? "").trim();
    if (!valeur) return;
    saisies.delete("__scrappeur");
    scrappeur("ranger_cle_scrappeur", { valeur });
  };
  return [
    brouillons.length > 0 && groupe(null, [ligne("Business", choixBusiness(ctx, brouillons))]),
    d &&
      groupe(
        "Scrappeur de la veille",
        [
          d.scrappeur
            ? ligne("Clé du scrappeur", h("span", { class: "valeur ok" }, "dans la base"), bouton("Effacer", () => scrappeur("effacer_cle_scrappeur", {})))
            : ligne(
                "Clé du scrappeur",
                champ(saisies.get("__scrappeur") ?? "", (v) => saisies.set("__scrappeur", v), { aide: "LurkAPI, rangée dans la base", secret: true, valider: rangerScrappeur }),
                bouton("Ranger", rangerScrappeur),
              ),
        ],
        "Recherche TikTok et Instagram de la veille, sans session.",
      ),
    d &&
      groupe(
        d.coffre,
        d.cles.map((c) =>
          c.presente
            ? ligne(c.libelle, h("span", { class: "valeur ok" }, "dans le " + d.coffre), bouton("Effacer", () => agir("effacer_cle", { nom: c.nom })))
            : ligne(c.libelle, champ(saisies.get(c.nom) ?? "", (v) => saisies.set(c.nom, v), { aide: c.nom, secret: true, valider: () => ranger(c.nom) }), bouton("Ranger", () => ranger(c.nom))),
        ),
        d.pied,
      ),
    d && groupe("Fichiers", d.fichiers.map((f) => ligne(f.libelle, h("span", { class: f.present ? "valeur" : "valeur absente" }, f.valeur)))),
    vue.message && h("div", { class: "actions" }, message()),
  ];
}

async function notifs(ctx) {
  const d = await ctx.appeler("reglages_notifs");
  return [
    groupe(
      "Me notifier pour",
      d.map((n) => ligne(n.libelle, interrupteur(n.actif, (oui) => ctx.appeler("ecrire_reglage", { cle: n.cle, valeur: oui ? null : "0" }).catch(() => afficher(ctx))))),
      "Une notification par nouveauté de moins de 2 h ; au-delà de trois d'un coup, un résumé. Une source qui tombe en panne notifie toujours.",
    ),
  ];
}

async function collecte(ctx) {
  const d = await ctx.appeler("reglages_collecte");
  return [
    groupe(null, [
      ligne("Collecte en pause", interrupteur(d.pause, (oui) => ctx.appeler("pause", { oui }).catch(() => afficher(ctx)))),
      ligne(null, bouton("Tout relancer", () => ctx.appeler("relancer", { source: null }))),
    ]),
    groupe("Sources", d.sources.map((s) => ligne(s.nom, h("span", { class: "valeur" }, s.cadence)))),
    groupe("Base", [ligne("Fichier", h("span", { class: "valeur chemin" }, d.fichier)), ligne(null, bouton(d.montrer, () => ctx.appeler("montrer_base")))]),
  ];
}

async function sessions(ctx) {
  const d = await ctx.appeler("reglages_sessions");
  return [
    groupe(
      "Sessions",
      d.map((s) => ligne(s.nom, h("span", { class: s.connecte ? "valeur ok" : "valeur absente" }, s.etat), bouton(s.connecte ? "Reconnecter" : "Se connecter", () => ctx.appeler("session", { site: s.id })))),
      "La veille lit TikTok, Instagram et Facebook avec ta session de Safari, sinon celle ouverte ici par « Se connecter ».",
    ),
  ];
}

const RENDUS = { business, cles, notifs, collecte, sessions };

// rendu

let racine = null;
let tour = 0;

async function afficher(ctx, { entrer = false } = {}) {
  const t = ++tour;
  const blocs = (await RENDUS[vue.onglet](ctx)).filter(Boolean);
  if (t !== tour) return;
  const colonne = h("div", { class: "reglages", "data-onglet": vue.onglet }, blocs);
  racine.replaceChildren(colonne);
  if (entrer) [...colonne.children].forEach((el, i) => entree(el, i));
}

export default {
  titre: "Réglages",
  sousTitre: "Business, clés, notifications, collecte et sessions",
  business: false,
  async rendre(el, ctx) {
    racine = el;
    vue.brouillon = null;
    vue.message = null;
    vue.supprimer = false;
    let courant = ctx;
    ctx.page.outils(
      segmente(ONGLETS, vue.onglet, (id) => {
        vue.onglet = id;
        vue.message = null;
        vue.supprimer = false;
        afficher(courant, { entrer: true });
      }),
    );
    await afficher(ctx);
    ctx.abonner((c) => {
      courant = c;
      if (VIVANTS.has(vue.onglet)) return afficher(c);
    });
  },
};
