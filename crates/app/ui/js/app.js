// La racine du Cockpit : barre du haut, écrans, raccourcis, palette, rafraîchissement.
//
// Un écran est un module ecrans/<id>.js qui exporte par défaut :
//   { titre, sousTitre, rendre(racine, ctx), defile?: false, business?: false }
// rendre (synchrone ou async) remplit `racine`, le contenu sous l'en-tête : construire, puis racine.replaceChildren(...) d'un coup.
// Il est rappelé, en mode calme, à chaque nouvelle version de la base, au changement de business et toutes les 30 s.
//
// ctx :
//   appeler(cmd, args)       une commande Rust ; l'erreur s'affiche en toast puis se relance
//   lien(url), image(cle)    ouvrir une adresse ; l'url d'une image du cache (`icone:<appId>`, `avatar:<compte>`)
//   aller(ecran)             changer d'écran
//   business                 l'id du business choisi, null pour « Tous »
//   businesses               [{ id, nom, lettre, teinte, appStoreId }]
//   sources, enCours, pause  les sources et leur dernier passage, les ids en cours, la collecte en pause
//   plateforme               "macos", "windows" ou "linux"
//   version, maintenant      la version de la base, l'heure du rendu (Date)
//   rafraichi                faux au premier rendu, vrai ensuite
//   choix                    ce que la palette a choisi de montrer ({ genre: "video" | "app" | "tache", id }), au seul rendu qui suit, sinon null
//   page.sousTitre(texte), page.outils(...noeuds)   l'en-tête de la page
//   abonner(fn) -> arrêt     fn(ctx) remplace le re-rendu complet tant que l'écran est affiché

import { appeler, ecouter, lien, image, fenetre, toast } from "./pont.js";
import { h, remplacer, icone, marque, avatar, point, surtitre, segmente, glisseur, entree, rendu, sobre, jeton, nombreJeton } from "./composants.js";
import * as F from "./format.js";
import { ouvrirPalette } from "./palette.js";

const ECRANS = [
  { id: "app", titre: "Accueil" },
  { id: "data", titre: "Data" },
  { id: "videos", titre: "Vidéos" },
  { id: "veille", titre: "Veille" },
  { id: "sante", titre: "Santé" },
  { id: "aTraiter", titre: "SAV" },
  { id: "apps", titre: "Apps de potes" },
  { id: "reglages", titre: "Réglages" },
];
const NAVIGATION = 7;
const GAUCHE = 5;
const HORLOGE = 30_000;

const etat = { ecran: "app", business: null, fenetre: null, version: -1, choix: null };
let mac = true;
let courant = null;
let fermerPalette = null;
let file = Promise.resolve();
let rafraichissementPrevu = false;

function rang(id) {
  return ECRANS.findIndex((e) => e.id === id);
}

function raccourci(touche) {
  return mac ? "⌘" + touche : "Ctrl+" + touche;
}

function signaler(e) {
  console.error(e);
  if (!e?.affichee) toast(e?.message ?? String(e));
}

function enFile(tache) {
  file = file.then(tache).catch(signaler);
  return file;
}

function regler(cle, valeur) {
  appeler("regler", { cle, valeur }).catch(signaler);
}

// barre du haut

const barre = {};

function construireBarre() {
  barre.choix = h("button", { class: "bouton-capsule verre capsule choix-business", onclick: () => menuBusiness() });
  barre.items = ECRANS.slice(0, NAVIGATION).map((e, i) =>
    h("button", { class: "item-nav", title: `${e.titre} (${raccourci(i + 1)})`, onclick: () => aller(e.id) }, e.titre),
  );
  barre.compteur = h("span", { class: "compteur" });
  // une pilule par groupe : le verre de chaque groupe isole son empilement
  const gauche = h("div", { class: "groupe gauche verre capsule" }, barre.items.slice(0, GAUCHE));
  const droite = h("div", { class: "groupe droite verre capsule" }, barre.items.slice(GAUCHE));
  barre.pilules = [glisseur(gauche), glisseur(droite)];
  barre.navigation = h(
    "nav",
    { class: "navigation", "data-tauri-drag-region": true },
    gauche,
    h("div", { class: "marque", "data-tauri-drag-region": true }, marque(), "Cockpit"),
    droite,
  );
  barre.points = h("span", { class: "points" });
  barre.texteSources = h("span", { class: "tag" });
  barre.bulle = h("div", { class: "bulle verre" });
  barre.actualiser = h("button", { class: "bouton-capsule verre capsule rond", onclick: () => relancer() }, icone("relancer"));
  const outils = [
    (barre.sources = h("button", { class: "bouton-capsule verre capsule bande-sources", onclick: () => aller("sante") }, barre.points, barre.texteSources, barre.bulle)),
    h("button", { class: "bouton-capsule verre capsule", title: "Rechercher", onclick: () => palette() }, icone("loupe"), h("span", { class: "tag" }, raccourci("K"))),
    barre.actualiser,
    h("button", { class: "bouton-capsule verre capsule rond", title: `Réglages (${raccourci(",")})`, onclick: () => aller("reglages") }, icone("reglages")),
  ];
  if (!mac) outils.push(boutonsFenetre());
  return h(
    "header",
    { class: "barre-haut", "data-tauri-drag-region": true },
    h("div", { class: "tiers gauche", "data-tauri-drag-region": true }, barre.choix),
    barre.navigation,
    h("div", { class: "tiers droite", "data-tauri-drag-region": true }, outils),
  );
}

function boutonsFenetre() {
  const w = fenetre();
  const geste = (promesse) => promesse.catch(signaler);
  return h(
    "div",
    { class: "fenetre" },
    h("button", { title: "Réduire", onclick: () => geste(w.minimize()) }, icone("reduire")),
    h("button", { title: "Agrandir", onclick: () => geste(w.toggleMaximize()) }, icone("agrandir")),
    h("button", { class: "fermer", title: "Fermer", onclick: () => geste(w.close()) }, icone("fermer")),
  );
}

function businessChoisi() {
  return etat.fenetre.businesses.find((b) => b.id === etat.business) ?? null;
}

function majBarre() {
  const f = etat.fenetre;
  const b = businessChoisi();
  barre.choix.replaceChildren(avatar(b, { mini: true }), b ? b.nom : "Tous", h("span", { class: "chevrons" }, icone("chevrons")));

  const r = rang(etat.ecran);
  barre.items.forEach((el, i) => el.classList.toggle("actif", i === r));
  barre.pilules[0].aller(r < GAUCHE ? barre.items[r] : null);
  barre.pilules[1].aller(r >= GAUCHE && r < NAVIGATION ? barre.items[r] : null);
  barre.compteur.textContent = F.entier(f.aTraiter);
  if (f.aTraiter > 0) barre.items[5].append(barre.compteur);
  else barre.compteur.remove();

  const enCours = new Set(f.enCours);
  const maintenant = Date.now();
  const fins = f.sources.map((s) => s.passage && Date.parse(s.passage.fin)).filter(Boolean);
  const derniere = fins.length ? Math.max(...fins) : null;
  barre.points.replaceChildren(...f.sources.map((s) => point(etatSource(s, enCours).couleur, { mini: true, enCours: enCours.has(s.id) })));
  barre.texteSources.textContent = derniere ? "Collecte " + F.ilYa(derniere, maintenant) : "Aucune collecte";
  barre.sources.setAttribute("aria-label", "Santé des sources, " + barre.texteSources.textContent);
  remplacer(
    barre.bulle,
    surtitre("Sources"),
    f.sources.length === 0 && h("div", { class: "derniere" }, "Aucune source branchée."),
    f.sources.map((s) => {
      const e = etatSource(s, enCours);
      return [
        h("div", { class: "source" }, point(e.couleur, { mini: true, enCours: enCours.has(s.id) }), h("span", { class: "nom" }, s.nom), h("span", { class: "etat", style: { "--c": e.couleur } }, e.texte)),
        e.panne && s.passage.erreur && h("div", { class: "erreur" }, s.passage.erreur),
      ];
    }),
    derniere && h("div", { class: "derniere" }, "Dernier passage " + F.ilYa(derniere, maintenant) + (f.pause ? ", collecte en pause" : "")),
  );

  const tourne = f.enCours.length > 0;
  barre.actualiser.classList.toggle("tourne", tourne);
  barre.actualiser.title = tourne ? "Collecte en cours" : `Tout relancer (${raccourci("R")})`;
}

function etatSource(s, enCours) {
  if (enCours.has(s.id)) return { texte: "…", couleur: "var(--texte)" };
  if (!s.passage) return { texte: "jamais lue", couleur: "var(--tertiaire)" };
  if (s.passage.etat === "ok") return { texte: "ok", couleur: "var(--succes)" };
  if (s.passage.etat === "non_configure") return { texte: "à régler", couleur: "var(--tertiaire)" };
  return { texte: "panne", couleur: "var(--danger)", panne: true };
}

function menuBusiness() {
  const r = barre.choix.getBoundingClientRect();
  const fermer = () => {
    menu.remove();
    document.removeEventListener("mousedown", dehors, true);
    document.removeEventListener("keydown", echap, true);
  };
  const dehors = (e) => {
    if (!menu.contains(e.target)) fermer();
  };
  const echap = (e) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      fermer();
    }
  };
  const item = (b, texte, faire) =>
    h("button", { class: b !== undefined && (b?.id ?? null) === etat.business ? "actif" : null, onclick: () => (fermer(), faire()) }, b !== undefined && avatar(b, { mini: true }), texte);
  const menu = h(
    "div",
    { class: "menu verre", style: { left: r.left + "px", top: r.bottom + nombreJeton("--e-m") + "px" } },
    item(null, "Tous les business", () => choisirBusiness(null)),
    etat.fenetre.businesses.map((b) => item(b, b.nom, () => choisirBusiness(b.id))),
    h("hr"),
    item(undefined, "Réglages…", () => aller("reglages")),
  );
  document.body.append(menu);
  document.addEventListener("mousedown", dehors, true);
  document.addEventListener("keydown", echap, true);
  if (!sobre()) {
    menu.animate([{ opacity: 0, transform: `scale(${jeton("--appui")})` }, { opacity: 1, transform: "none" }], {
      duration: nombreJeton("--duree-survol"),
      easing: jeton("--ressort-survol"),
    });
  }
}

// écrans

function contexte(rafraichi) {
  const f = etat.fenetre;
  return {
    appeler,
    lien,
    image: (cle) => image(f.miniatures, cle),
    aller,
    business: etat.business,
    businesses: f.businesses,
    sources: f.sources,
    enCours: f.enCours,
    pause: f.pause,
    plateforme: f.plateforme,
    version: f.version,
    maintenant: new Date(),
    rafraichi,
    choix: etat.choix,
    page: courant.page,
    abonner: (fn) => {
      courant.abonnes.add(fn);
      return () => courant.abonnes.delete(fn);
    },
  };
}

function construirePage(module) {
  const sousTitre = h("div", { class: "sous-titre" }, module.sousTitre);
  const outils = h("div", { class: "outils" });
  const contenu = h("div", { class: module.defile === false ? "contenu fixe" : "contenu" });
  const choix =
    module.business !== false && etat.fenetre.businesses.length > 1
      ? segmente(
          [{ id: null, libelle: "Tous" }, ...etat.fenetre.businesses.map((b) => ({ id: b.id, libelle: b.nom, avatar: avatar(b, { mini: true }) }))],
          etat.business,
          choisirBusiness,
        )
      : null;
  const el = h(
    "section",
    { class: "ecran" },
    h(
      "div",
      { class: "page" },
      h("header", { class: "entete" }, h("div", { class: "titres" }, h("h1", { class: "titre" }, module.titre), sousTitre), h("div", { class: "outils" }, outils, choix)),
      contenu,
    ),
  );
  return {
    el,
    contenu,
    page: {
      sousTitre: (texte) => (sousTitre.textContent = texte),
      outils: (...noeuds) => outils.replaceChildren(...noeuds),
    },
  };
}

async function rendreCourant(rafraichi) {
  const c = courant;
  rendu.calme = rafraichi;
  const ctx = contexte(rafraichi);
  etat.choix = null;
  if (rafraichi && c.abonnes.size) {
    for (const fn of c.abonnes) await fn(ctx);
  } else {
    await c.module.rendre(c.contenu, ctx);
  }
  if (!rafraichi && c === courant) [...c.contenu.children].forEach((el, i) => entree(el, i));
}

function bascule(ancien, nouveau, avance) {
  if (!ancien) return;
  if (sobre()) return ancien.remove();
  const sens = avance ? 1 : -1;
  const decal = 2 * nombreJeton("--glisse") * sens;
  const reduit = nombreJeton("--reduit");
  const flou = `blur(${jeton("--flou")})`;
  const opts = { duration: nombreJeton("--duree-navigation"), easing: jeton("--ressort-navigation") };
  ancien.style.pointerEvents = "none";
  nouveau.animate([{ opacity: 0, transform: `translateX(${decal}px) scale(${reduit})`, filter: flou }, { opacity: 1, transform: "none", filter: "none" }], opts);
  ancien
    .animate([{ opacity: 1, transform: "none", filter: "none" }, { opacity: 0, transform: `translateX(${-decal}px) scale(${1 / reduit})`, filter: flou }], { ...opts, fill: "forwards" })
    .finished.then(() => ancien.remove());
}

function aller(id) {
  if (rang(id) < 0 || id === courant?.id) return;
  return enFile(async () => {
    if (id === courant?.id) return;
    const module = (await import(`./ecrans/${id}.js`)).default;
    const avance = !courant || rang(id) >= rang(courant.id);
    const ancien = courant?.el;
    etat.ecran = id;
    if (rang(id) < NAVIGATION) regler("ecran", id);
    courant = { id, module, abonnes: new Set(), ...construirePage(module) };
    document.querySelector(".zone").append(courant.el);
    majBarre();
    bascule(ancien, courant.el, avance);
    await rendreCourant(false);
  });
}

function choisirBusiness(id) {
  if (id === etat.business) return;
  etat.business = id;
  regler("business", id);
  majBarre();
  const actuel = courant;
  enFile(async () => {
    if (courant !== actuel) return;
    const nouvelle = construirePage(courant.module);
    courant.el.replaceWith(nouvelle.el);
    nouvelle.contenu.replaceChildren(...courant.contenu.childNodes);
    Object.assign(courant, nouvelle);
    await rendreCourant(true);
  });
}

// rafraîchissement

async function lireEtat() {
  etat.fenetre = await appeler("etat");
  rendu.miniatures = etat.fenetre.miniatures;
}

function rafraichir(force) {
  if (rafraichissementPrevu) return;
  rafraichissementPrevu = true;
  enFile(async () => {
    rafraichissementPrevu = false;
    await lireEtat();
    majBarre();
    if (force || etat.fenetre.version !== etat.version) {
      etat.version = etat.fenetre.version;
      await rendreCourant(true);
    }
  });
}

function relancer(source = null) {
  appeler("relancer", { source }).then(() => rafraichir(false), signaler);
}

function mettreEnPause(oui) {
  appeler("pause", { oui }).then(() => rafraichir(false), signaler);
}

// palette

// ce que trouve la base : le genre donne l'icône et l'écran qui le montre
const TROUVES = { video: "videos", app: "apps", tache: "aTraiter" };

function montrerTrouve(t) {
  const ecran = TROUVES[t.genre];
  etat.choix = { genre: t.genre, id: t.id };
  if (courant?.id === ecran) enFile(() => rendreCourant(true));
  else aller(ecran);
}

function chercher(texte) {
  return appeler("chercher", { business: etat.business, requete: texte }).then((trouves) =>
    trouves.map((t) => ({ icone: TROUVES[t.genre], titre: t.titre, detail: t.detail, faire: () => montrerTrouve(t) })),
  );
}

function palette() {
  // ⌘K referme une palette ouverte
  if (fermerPalette?.()) return;
  const f = etat.fenetre;
  fermerPalette = ouvrirPalette({
    resultats: () => [
      ...ECRANS.map((e, i) => ({ icone: e.id, titre: e.titre, detail: i < NAVIGATION ? raccourci(i + 1) : raccourci(","), faire: () => aller(e.id) })),
      { icone: "relancer", titre: "Tout relancer", detail: raccourci("R"), faire: () => relancer() },
      ...f.sources.map((s) => ({ icone: "relancer", titre: "Relancer " + s.nom, detail: "Source", faire: () => relancer(s.id) })),
      f.pause
        ? { icone: "lecture", titre: "Reprendre la collecte", detail: "Collecte", faire: () => mettreEnPause(false) }
        : { icone: "pause", titre: "Mettre la collecte en pause", detail: "Collecte", faire: () => mettreEnPause(true) },
      ...(f.businesses.length > 1 ? [{ avatar: null, titre: "Afficher tous les business", detail: "Business", faire: () => choisirBusiness(null) }] : []),
      ...f.businesses.map((b) => ({ avatar: b, titre: "Afficher " + b.nom, detail: "Business", faire: () => choisirBusiness(b.id) })),
    ],
    chercher,
  });
}

// raccourcis : ⌘ sur Mac, Ctrl ailleurs ; Ctrl+R et F5 ne rechargent jamais la page

function clavier(e) {
  if (e.key === "F5") {
    e.preventDefault();
    relancer();
    return;
  }
  if (!(mac ? e.metaKey : e.ctrlKey) || e.altKey) return;
  const chiffre = /^(?:Digit|Numpad)([1-7])$/.exec(e.code);
  const touche = e.key.toLowerCase();
  if (chiffre) aller(ECRANS[+chiffre[1] - 1].id);
  else if (touche === "k") palette();
  else if (touche === "r") relancer();
  else if (touche === ",") aller("reglages");
  else return;
  e.preventDefault();
}

async function demarrer() {
  await lireEtat();
  etat.version = etat.fenetre.version;
  mac = etat.fenetre.plateforme === "macos";
  document.documentElement.classList.add(mac ? "mac" : "pc");
  const r = etat.fenetre.reglages;
  etat.business = etat.fenetre.businesses.some((b) => b.id === r.business) ? r.business : null;
  const ecran = rang(r.ecran) >= 0 && rang(r.ecran) < NAVIGATION ? r.ecran : "app";
  document.body.append(h("div", { class: "lueur" }), h("div", { class: "racine" }, construireBarre(), h("main", { class: "zone" })));
  document.addEventListener("keydown", clavier);
  await aller(ecran);
  await ecouter("cockpit", () => rafraichir(false));
  await ecouter("ecran", (id) => aller(id));
  setInterval(() => rafraichir(true), HORLOGE);
}

demarrer().catch(signaler);
