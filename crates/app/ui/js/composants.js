// Les composants partagés, portés de Composants.swift. Chacun rend un élément DOM, sans état caché.
//
//   h(tag, attrs, ...enfants)       un élément ; attrs : class, style (texte ou objet, --variables comprises), onX (écouteurs),
//                                   le reste en attributs ; enfants : nœuds, textes, nombres, tableaux, null ignoré
//   s(tag, attrs, ...enfants)       pareil en SVG
//   rendu                           { calme, memoire, miniatures } : calme pendant un re-rendu (ni entrée ni tracé rejoués)
//   sobre()                         vrai si le système demande moins d'animations
//   tracer(path)                    dessine un trait de pathLength 1 (sans tiret gardé : WebKit le casse)
//   entree(el, rang)                l'entrée décalée d'un bloc (fondu, glisse, flou)
//   glisseur(conteneur)             une pilule qui glisse vers l'élément actif, un enfant direct : .aller(el | null)
//   teinte(cle), couleurTon(ton), couleurPlateforme(p)   des couleurs CSS (`var(--aqua)`)
//   icone(nom), marque(), logoPlateforme(p)
//   surtitre(texte), pastille(texte, { couleur, neutre, largeurMin, aide }), point(couleur, { mini, enCours, aide }), vide(texte)
//   panneau({ titre, accessoire, classe }, ...enfants), bloc(...enfants), enTetePanneau(titre, ...accessoires)
//   rangee(poids, enfants, { meneur })   une rangée à poids ([2, 1]) ; meneur : l'index qui donne la hauteur, les autres défilent
//   segmente(options, choisi, quandChoisi)   options : [{ id, libelle, avatar? }]
//   pilule(libelle, quandClic, { icone, aide })
//   chiffre(valeur, { format, cle })     un nombre qui roule depuis des zéros, puis depuis sa valeur gardée sous `cle`
//   kpi({ titre, valeur, format, delta, ton, note, serie, inverse, grande, cle }), grilleKpi(...cartes)
//   sparkline(valeurs, { ton, couleur, inverse, large }) ; null dans valeurs coupe la courbe
//   courbe({ series: [{ nom, valeurs, couleur }], etiquettes, format, hauteur })   avec viseur et bulle au survol
//   barres(valeurs, { fantome, courante, axe, aide, hauteur })
//   legende([{ nom, couleur }])
//   avatar(business | null, { mini }), miniature(cles, { plateforme, largeur, hauteur, rayon })
//   ligneFil(evenement, { jour }), caseRonde({ faite, alerte, quandClic })
//   boutonRelancer(enCours, quandClic)   relance une source (Santé, SAV)
//   actualiser(...sources)          rafraîchit les seules sources d'un panneau (RESEAUX : les quatre réseaux)
//   suivreCollecte(sources, enCours)   app.js donne l'état de la collecte ; les boutons actualiser affichés suivent

import * as F from "./format.js";
import { appeler, image, lien } from "./pont.js";

const SVG = "http://www.w3.org/2000/svg";

export const rendu = { calme: false, memoire: new Map(), miniatures: "" };

const reduit = matchMedia("(prefers-reduced-motion: reduce)");
export function sobre() {
  return reduit.matches;
}

const style = getComputedStyle(document.documentElement);
export function jeton(nom) {
  return style.getPropertyValue(nom).trim();
}
export function nombreJeton(nom) {
  return parseFloat(jeton(nom));
}

function remplir(el, attrs, enfants) {
  for (const [cle, val] of Object.entries(attrs ?? {})) {
    if (val == null || val === false) continue;
    if (cle === "style" && typeof val === "object") {
      for (const [p, v] of Object.entries(val)) if (v != null) el.style.setProperty(p, v);
    } else if (cle.startsWith("on") && typeof val === "function") {
      el.addEventListener(cle.slice(2).toLowerCase(), val);
    } else {
      el.setAttribute(cle, val === true ? "" : val);
    }
  }
  for (const e of enfants.flat(Infinity)) {
    if (e == null || e === false) continue;
    el.append(e instanceof Node ? e : String(e));
  }
  return el;
}

export function h(tag, attrs, ...enfants) {
  return remplir(document.createElement(tag), attrs, enfants);
}

export function s(tag, attrs, ...enfants) {
  return remplir(document.createElementNS(SVG, tag), attrs, enfants);
}

// replaceChildren qui accepte, comme h, des tableaux et des valeurs nulles ou false
export function remplacer(el, ...enfants) {
  el.replaceChildren();
  return remplir(el, null, enfants);
}

// animations

export function entree(el, rang = 0) {
  if (rendu.calme || sobre()) return;
  el.animate(
    [
      { opacity: 0, transform: `translateY(${jeton("--glisse")})`, filter: `blur(${jeton("--flou")})` },
      { opacity: 1, transform: "none", filter: "none" },
    ],
    {
      duration: nombreJeton("--duree-entree"),
      easing: jeton("--ressort-entree"),
      delay: Math.min(rang, nombreJeton("--rang-max")) * nombreJeton("--decalage"),
      fill: "backwards",
    },
  );
}

export function glisseur(conteneur) {
  const g = h("div", { class: "glisseur" });
  conteneur.prepend(g);
  let cible = null;
  const placer = (anime) => {
    if (!cible || !cible.isConnected) {
      g.style.opacity = "0";
      return;
    }
    // offset* : la mesure ignore les transformations en cours (palette qui surgit, écran qui bascule)
    if (!anime) g.style.transition = "none";
    g.style.width = cible.offsetWidth + "px";
    g.style.height = cible.offsetHeight + "px";
    g.style.transform = `translate(${cible.offsetLeft}px, ${cible.offsetTop}px)`;
    g.style.opacity = "1";
    if (!anime) {
      g.getBoundingClientRect();
      g.style.transition = "";
    }
  };
  new ResizeObserver(() => placer(false)).observe(conteneur);
  return {
    aller(el) {
      const anime = cible != null && el != null && !sobre();
      cible = el;
      placer(anime);
    },
  };
}

// couleurs

const TEINTES = { ciel: "var(--bleu)", lilas: "var(--violet)", ambre: "var(--orange)" };
export const teintes = [
  { cle: "menthe", nom: "Aqua" },
  { cle: "ciel", nom: "Bleu" },
  { cle: "lilas", nom: "Violet" },
  { cle: "ambre", nom: "Orange" },
];
export function teinte(cle) {
  return TEINTES[cle] ?? "var(--aqua)";
}

/** positif, negatif ou neutre ; `inverse` retourne le sens (un rang qui baisse est une bonne nouvelle). */
export function couleurTon(ton, inverse = false) {
  const t = inverse ? { positif: "negatif", negatif: "positif" }[ton] ?? ton : ton;
  return t === "positif" ? "var(--succes)" : t === "negatif" ? "var(--danger)" : "var(--secondaire)";
}

const PLATEFORMES = { tiktok: "var(--aqua)", instagram: "var(--violet)", youtube: "var(--orange)", facebook: "var(--bleu)" };
export function couleurPlateforme(p) {
  return PLATEFORMES[p] ?? "var(--secondaire)";
}

// icônes, dessinées en traits sur une grille de 24

const ICONES = {
  loupe: ["circle cx=11 cy=11 r=7", "path d=M16.5,16.5L21,21"],
  reglages: [
    "circle cx=12 cy=12 r=3",
    "path d=M19.4,15a1.65,1.65,0,0,0,.33,1.82l.06.06a2,2,0,0,1-2.83,2.83l-.06-.06a1.65,1.65,0,0,0-1.82-.33,1.65,1.65,0,0,0-1,1.51V21a2,2,0,0,1-4,0v-.09A1.65,1.65,0,0,0,9,19.4a1.65,1.65,0,0,0-1.82.33l-.06.06a2,2,0,0,1-2.83-2.83l.06-.06A1.65,1.65,0,0,0,4.68,15a1.65,1.65,0,0,0-1.51-1H3a2,2,0,0,1,0-4h.09A1.65,1.65,0,0,0,4.6,9a1.65,1.65,0,0,0-.33-1.82l-.06-.06a2,2,0,0,1,2.83-2.83l.06.06A1.65,1.65,0,0,0,9,4.68a1.65,1.65,0,0,0,1-1.51V3a2,2,0,0,1,4,0v.09a1.65,1.65,0,0,0,1,1.51,1.65,1.65,0,0,0,1.82-.33l.06-.06a2,2,0,0,1,2.83,2.83l-.06.06A1.65,1.65,0,0,0,19.4,9a1.65,1.65,0,0,0,1.51,1H21a2,2,0,0,1,0,4h-.09A1.65,1.65,0,0,0,19.4,15Z",
  ],
  relancer: ["path d=M20,12a8,8,0,1,1-2.34-5.66", "path d=M20,4v5h-5"],
  chevrons: ["path d=M8,9l4-4,4,4", "path d=M8,15l4,4,4-4"],
  reduire: ["path d=M6,12h12"],
  agrandir: ["rect x=6 y=6 width=12 height=12 rx=1"],
  fermer: ["path d=M6,6L18,18M18,6L6,18"],
  pause: ["path d=M9,5v14M15,5v14"],
  lecture: ["path d=M7,5l12,7-12,7Z"],
  app: ["path d=M3,11l9-7,9,7v9a1,1,0,0,1-1,1h-5v-6h-6v6H4a1,1,0,0,1-1-1Z"],
  data: ["path d=M5,20V11M12,20V5M19,20v-6"],
  videos: ["rect x=3 y=5 width=18 height=14 rx=3", "path d=M10,9l5,3-5,3Z"],
  veille: ["path d=M2,12s3.5-7,10-7,10,7,10,7-3.5,7-10,7S2,12,2,12Z", "circle cx=12 cy=12 r=3"],
  sante: ["path d=M3,12h4l3-8,4,16,3-8h4"],
  aTraiter: ["path d=M3,13h5l2,3h4l2-3h5", "path d=M5,5h14l2,8v6H3v-6Z"],
  apps: ["rect x=4 y=4 width=7 height=7 rx=2", "rect x=13 y=4 width=7 height=7 rx=2", "rect x=4 y=13 width=7 height=7 rx=2", "rect x=13 y=13 width=7 height=7 rx=2"],
  etincelle: ["path d=M12,3l1.9,5.1L19,10l-5.1,1.9L12,17l-1.9-5.1L5,10l5.1-1.9Z", "path d=M19,15l.8,2.2L22,18l-2.2.8L19,21l-.8-2.2L16,18l2.2-.8Z"],
  coche: ["path d=M5,12.5l4.5,4.5L19,7.5"],
  alerte: ["path d=M12,3L22,20H2Z", "path d=M12,10v4", "path d=M12,17v.5"],
  image: ["rect x=3 y=5 width=18 height=14 rx=3", "circle cx=9 cy=10 r=1.5", "path d=M21,16l-5-5-8,8"],
  etiquette: ["path d=M3,12V4a1,1,0,0,1,1-1h8l9,9-9,9Z", "circle cx=8 cy=8 r=1.5"],
  chevron: ["path d=M9,6l6,6-6,6"],
  envoyer: ["path d=M12,19V5", "path d=M6,11l6-6,6,6"],
  info: ["circle cx=12 cy=12 r=9", "path d=M12,11v5", "path d=M12,8v.5"],
  plus: ["path d=M12,5v14M5,12h14"],
  poubelle: ["path d=M4,7h16", "path d=M9,7V4h6v3", "path d=M6,7l1,13h10l1-13", "path d=M10,11v5M14,11v5"],
};

export function icone(nom) {
  return s(
    "svg",
    { class: "icone", viewBox: "0 0 24 24", "aria-hidden": "true" },
    ICONES[nom].map((def) => {
      const [tag, ...paires] = def.split(" ");
      return s(tag, Object.fromEntries(paires.map((p) => p.split("="))));
    }),
  );
}

/** L'anneau et l'horizon du logo. */
export function marque() {
  return s(
    "svg",
    { class: "marque-svg", viewBox: "0 0 680 680", "aria-hidden": "true" },
    s("circle", { cx: 340, cy: 340, r: 319, fill: "none", stroke: "currentColor", "stroke-width": 42 }),
    s("path", { d: "M584.54 288.02 A250 250 0 0 1 95.46 391.98 Z", fill: "currentColor" }),
  );
}

export function logoPlateforme(p) {
  return h("img", { class: "logo", src: `img/logo-${p}.svg`, alt: p });
}

// éléments de base

export function surtitre(texte) {
  return h("div", { class: "surtitre" }, texte);
}

export function pastille(texte, { couleur, neutre, largeurMin, aide } = {}) {
  return h(
    "span",
    { class: neutre ? "pastille neutre" : "pastille doux", title: aide, style: { "--c": couleur, "min-width": largeurMin } },
    texte,
  );
}

export function point(couleur, { mini, enCours, aide } = {}) {
  return h("span", { class: ["point", mini && "mini", enCours && "en-cours"].filter(Boolean).join(" "), title: aide, style: { "--c": couleur } });
}

export function vide(texte) {
  return h("div", { class: "vide" }, texte);
}

export function enTetePanneau(titre, ...accessoires) {
  return h("div", { class: "entete-panneau" }, typeof titre === "string" ? surtitre(titre) : titre, h("div", { class: "espace" }), accessoires);
}

export function panneau({ titre, accessoire, classe } = {}, ...enfants) {
  return h("section", { class: "panneau verre" + (classe ? " " + classe : "") }, titre != null && enTetePanneau(titre, accessoire), enfants);
}

export function bloc(...enfants) {
  return h("div", { class: "bloc verre" }, enfants);
}

export function rangee(poids, enfants, { meneur } = {}) {
  const el = h(
    "div",
    { class: meneur == null ? "rangee" : "rangee meneur", style: { "--colonnes": poids.map((p) => `minmax(0, ${p}fr)`).join(" ") } },
    enfants,
  );
  if (meneur != null) el.children[meneur].classList.add("mene");
  return el;
}

export function segmente(options, choisi, quandChoisi) {
  const el = h("div", { class: "segmente verre", role: "tablist" });
  const pilule = glisseur(el);
  const boutons = options.map((o) =>
    h(
      "button",
      {
        class: o.id === choisi ? "actif" : null,
        role: "tab",
        onclick: (e) => {
          for (const b of boutons) b.classList.toggle("actif", b === e.currentTarget);
          pilule.aller(e.currentTarget);
          quandChoisi(o.id);
        },
      },
      o.avatar,
      o.libelle,
    ),
  );
  el.append(...boutons);
  pilule.aller(boutons.find((b) => b.classList.contains("actif")) ?? null);
  return el;
}

export function pilule(libelle, quandClic, { icone: nom, aide } = {}) {
  return h("button", { class: "pilule", title: aide, onclick: quandClic }, nom && icone(nom), libelle);
}

// chiffre qui roule

function rouler(el, versLeHaut, sortant) {
  const sens = versLeHaut ? 1 : -1;
  const opts = { duration: nombreJeton("--duree-chiffre"), easing: jeton("--ressort-chiffre") };
  const flou = `blur(${nombreJeton("--flou") / 2}px)`;
  el.animate([{ transform: `translateY(${sens * 60}%)`, filter: flou, opacity: 0 }, { transform: "none", filter: "none", opacity: 1 }], opts);
  if (sortant) {
    sortant.animate([{ transform: "none", filter: "none", opacity: 1 }, { transform: `translateY(${-sens * 60}%)`, filter: flou, opacity: 0 }], { ...opts, fill: "forwards" })
      .finished.then(() => sortant.remove());
  }
}

export function chiffre(valeur, { format = F.entier, cle } = {}) {
  const texte = valeur == null ? F.vide : format(valeur);
  const garde = cle != null ? rendu.memoire.get(cle) : null;
  if (cle != null) rendu.memoire.set(cle, { texte, valeur });
  const el = h("span", { class: "chiffre" });
  const avant = garde ? garde.texte : rendu.calme ? texte : texte.replace(/\d/g, "0");
  const monte = garde?.valeur == null || valeur == null || valeur >= garde.valeur;
  const decal = avant.length - texte.length;
  for (let i = 0; i < texte.length; i++) {
    const ancien = avant[i + decal];
    if (ancien === texte[i] || sobre()) {
      el.append(h("span", null, texte[i]));
      continue;
    }
    const entrant = h("span", null, texte[i]);
    const sortant = ancien?.trim() ? h("span", { class: "sortant" }, ancien) : null;
    el.append(h("span", { class: "roule" }, entrant, sortant));
    rouler(entrant, monte, sortant);
  }
  return el;
}

// KPI

export function kpi({ titre, valeur, format, delta, ton = "neutre", note, serie, inverse = false, grande = false, cle }) {
  const gauche = h(
    "div",
    { class: "gauche" },
    h("div", { class: "haut" }, surtitre(titre), delta != null && pastille(delta, { couleur: couleurTon(ton) })),
    h("div", { class: "valeur" }, chiffre(valeur, { format, cle })),
    note != null && h("div", { class: "note" }, note),
  );
  return h(
    "div",
    { class: "kpi verre survolable" + (grande ? " grande" : "") },
    gauche,
    serie && sparkline(serie, { ton, inverse, large: grande }),
  );
}

export function grilleKpi(...cartes) {
  cartes.forEach((c, i) => entree(c, i + 1));
  return h("div", { class: "grille-kpi" }, cartes);
}

// graphiques

function etendue(valeurs) {
  const v = valeurs.filter((x) => x != null);
  const min = Math.min(...v);
  const max = Math.max(...v);
  return { min, max, pas: max - min || 1 };
}

// le tracé SVG d'une série dans un repère 0..100, coupé à chaque valeur manquante
function chemin(valeurs, y) {
  const n = valeurs.length;
  let d = "";
  let leve = true;
  valeurs.forEach((v, i) => {
    if (v == null) {
      leve = true;
      return;
    }
    const x = n === 1 ? 50 : (i / (n - 1)) * 100;
    d += (leve ? "M" : "L") + x.toFixed(2) + "," + y(v).toFixed(2);
    leve = false;
  });
  return d;
}

/** Dessine un trait de `pathLength` 1. Le tiret ne vit que le temps du tracé : WebKit coupe en tirets un trait non-scaling-stroke. */
export function tracer(path) {
  if (rendu.calme || sobre()) return;
  path.animate([{ strokeDasharray: 1, strokeDashoffset: 1 }, { strokeDasharray: 1, strokeDashoffset: 0 }], { duration: nombreJeton("--duree-trace"), easing: jeton("--courbe-trace") });
}

function devoiler(el) {
  if (rendu.calme || sobre()) return;
  el.animate([{ clipPath: "inset(0 100% 0 0)" }, { clipPath: "inset(0 0 0 0)" }], { duration: nombreJeton("--duree-trace"), easing: jeton("--courbe-trace") });
}

export function sparkline(valeurs, { ton = "neutre", couleur, inverse = false, large = false } = {}) {
  const c = couleur ?? (ton === "neutre" ? "var(--blanc-trait)" : couleurTon(ton));
  const el = h("div", { class: large ? "sparkline large" : "sparkline", style: { "--c": c } });
  if (!valeurs.some((v) => v != null)) return el;
  const { min, pas } = etendue(valeurs);
  const marge = 100 / nombreJeton("--t-sparkline-h");
  const y = (v) => {
    const r = (v - min) / pas;
    return marge + (inverse ? r : 1 - r) * (100 - 2 * marge);
  };
  const path = s("path", { d: chemin(valeurs, y), pathLength: 1 });
  el.append(s("svg", { viewBox: "0 0 100 100", preserveAspectRatio: "none" }, path));
  tracer(path);
  const dernier = valeurs.findLastIndex((v) => v != null);
  const bout = h("i", {
    class: "bout",
    style: { left: (valeurs.length === 1 ? 50 : (dernier / (valeurs.length - 1)) * 100) + "%", top: y(valeurs[dernier]) + "%" },
  });
  el.append(bout);
  if (!rendu.calme && !sobre()) bout.animate([{ opacity: 0 }, { opacity: 1 }], { duration: nombreJeton("--duree-fondu"), delay: nombreJeton("--duree-trace"), fill: "backwards" });
  return el;
}

export function courbe({ series, etiquettes = [], format = F.compact, hauteur }) {
  const toutes = series.flatMap((s) => s.valeurs).filter((v) => v != null);
  const max = Math.max(0, ...toutes) || 1;
  const n = Math.max(...series.map((s) => s.valeurs.length));
  const y = (v) => (1 - v / max) * 100;
  const xDe = (i) => (n === 1 ? 50 : (i / (n - 1)) * 100);
  const svg = s(
    "svg",
    { viewBox: "0 0 100 100", preserveAspectRatio: "none" },
    series.map((serie) => s("path", { class: "serie", d: chemin(serie.valeurs, y), style: `--c:${serie.couleur ?? "var(--texte)"}` })),
  );
  const viseur = h("div", { class: "viseur" });
  const vises = series.map((serie) => h("i", { class: "vise", style: { "--c": serie.couleur ?? "var(--texte)" } }));
  const bulle = h("div", { class: "bulle-graphique" });
  const trace = h(
    "div",
    { class: "trace" },
    h("div", { class: "grille" }, h("i"), h("i"), h("i")),
    svg,
    viseur,
    vises,
    bulle,
  );
  trace.addEventListener("mousemove", (e) => {
    const r = trace.getBoundingClientRect();
    const i = Math.round(Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)) * (n - 1));
    const x = xDe(i);
    viseur.style.left = x + "%";
    series.forEach((serie, k) => {
      const v = serie.valeurs[i];
      vises[k].style.display = v == null ? "none" : "";
      vises[k].style.left = x + "%";
      vises[k].style.top = (v == null ? 0 : y(v)) + "%";
    });
    remplacer(
      bulle,
      etiquettes[i] != null && h("b", null, etiquettes[i]),
      series.map((serie) => h("div", { class: "ligne" }, h("span", { class: "point mini", style: { "--c": serie.couleur ?? "var(--texte)" } }), serie.nom, " ", h("b", null, serie.valeurs[i] == null ? F.vide : format(serie.valeurs[i])))),
    );
    const l = (x / 100) * r.width;
    bulle.style.left = Math.min(Math.max(0, l + 12), r.width - bulle.offsetWidth) + "px";
  });
  devoiler(svg);
  const axeX = etiquettes.length ? [etiquettes[0], etiquettes[Math.floor((etiquettes.length - 1) / 2)], etiquettes.at(-1)] : [];
  return h(
    "div",
    { class: "courbe", style: { "--hauteur": hauteur } },
    h("div", { class: "axe-y" }, h("span", null, format(max)), h("span", null, format(max / 2)), h("span", null, format(0))),
    trace,
    h("div", { class: "axe-x" }, axeX.map((t) => h("span", null, t))),
  );
}

export function barres(valeurs, { fantome = [], courante, axe = [], aide, hauteur } = {}) {
  const n = valeurs.length;
  const max = Math.max(0, ...valeurs.filter((v) => v != null), ...fantome.filter((v) => v != null)) || 1;
  const anime = !rendu.calme && !sobre();
  const colonnes = valeurs.map((v, i) => {
    const valeur = h("i", { class: "valeur", style: { height: ((v ?? 0) / max) * 100 + "%" } });
    const col = h(
      "div",
      { class: ["colonne", i === courante && "courante", courante != null && i > courante && "apres"].filter(Boolean).join(" "), title: aide?.(i) },
      fantome[i] != null && h("i", { class: "fantome", style: { height: (fantome[i] / max) * 100 + "%" } }),
      valeur,
    );
    if (anime) {
      valeur.animate([{ transform: "scaleY(0)" }, { transform: "scaleY(1)" }], {
        duration: nombreJeton("--duree-entree"),
        easing: jeton("--ressort-entree"),
        delay: (i * nombreJeton("--decalage-barre") * 24) / n,
        fill: "backwards",
      });
    }
    return col;
  });
  return h(
    "div",
    { class: "barres", style: { "--hauteur": hauteur, "--ecart": n > 31 ? "var(--e-xs)" : "var(--e-s)" } },
    h("div", { class: "colonnes" }, colonnes),
    axe.length > 0 && h("div", { class: "axe-x" }, axe.map((t) => h("span", null, t))),
  );
}

export function legende(items) {
  return h("div", { class: "legende" }, items.map((it) => h("span", null, h("i", { style: { "--c": it.couleur } }), it.nom)));
}

// avatars et images

function lettre(b, mini) {
  return h("span", { class: "avatar doux" + (mini ? " petit" : ""), style: { "--c": teinte(b.teinte), "--t": mini ? "var(--t-avatar-mini)" : null } }, b.lettre);
}

/** L'icône App Store du business, sinon sa lettre sur sa teinte ; « Tous » (null) prend la marque. */
export function avatar(b, { mini = false } = {}) {
  const t = mini ? "var(--t-avatar-mini)" : null;
  if (b == null) return h("span", { class: "avatar marque doux", style: { "--t": t } }, marque());
  if (!b.appStoreId) return lettre(b, mini);
  const el = h("span", { class: "avatar image", style: { "--t": t } });
  const img = h("img", { src: image(rendu.miniatures, "icone:" + b.appStoreId), alt: "" });
  img.addEventListener("error", () => el.replaceWith(lettre(b, mini)));
  el.append(img);
  return el;
}

/** Une image du cache : chaque clé à tour de rôle, sinon le logo de la plateforme, ou un fond uni. */
export function miniature(cles, { plateforme, largeur, hauteur, rayon } = {}) {
  const el = h("span", {
    class: plateforme ? "miniature doux" : "miniature",
    style: { width: largeur, height: hauteur, "--rayon": rayon, "--c": plateforme && couleurPlateforme(plateforme) },
  });
  if (plateforme) el.append(logoPlateforme(plateforme));
  const essayer = (i) => {
    if (i >= cles.length) return;
    const img = h("img", { src: image(rendu.miniatures, cles[i]), alt: "" });
    img.addEventListener("error", () => {
      img.remove();
      essayer(i + 1);
    });
    img.addEventListener("load", () => {
      if (!rendu.calme && !sobre()) img.animate([{ opacity: 0 }, { opacity: 1 }], { duration: nombreJeton("--duree-fondu") });
    });
    el.append(img);
  };
  essayer(0);
  return el;
}

// fil et tâches

const TAGS = { ESSAI: "var(--succes)", ACHAT: "var(--succes)", RANG: "var(--succes)", IA: "var(--ambre)" };

/** Une ligne du fil : heure, tag, texte, et la flèche si l'événement a un lien. */
export function ligneFil(ev, { jour = false } = {}) {
  return h(
    ev.lien ? "button" : "div",
    { class: ["ligne-fil", jour && "jour", ev.lien && "lien"].filter(Boolean).join(" "), onclick: ev.lien && (() => lien(ev.lien)) },
    h("span", { class: "heure" }, jour ? F.jourHeure(ev.at) : F.heure(ev.at)),
    pastille(ev.type === "VIDEO" ? "VIDÉO" : ev.type, { couleur: TAGS[ev.type] ?? "var(--secondaire)" }),
    h("span", { class: "texte" }, ev.texte),
    ev.lien && h("span", { class: "fleche" }, "↗"),
  );
}

export function caseRonde({ faite = false, alerte = false, quandClic }) {
  return h(
    "button",
    { class: ["case", faite && "faite", !faite && alerte && "alerte"].filter(Boolean).join(" "), title: faite ? "Remettre à faire" : "Marquer comme fait", onclick: quandClic },
    faite && "✓",
  );
}

// Réglages

/** Un interrupteur : quandChange(oui) à chaque bascule ; l'état affiché suit le clic. */
export function interrupteur(actif, quandChange) {
  const el = h("button", {
    class: "interrupteur",
    role: "switch",
    "aria-checked": String(actif),
    onclick: () => {
      const oui = el.getAttribute("aria-checked") !== "true";
      el.setAttribute("aria-checked", String(oui));
      quandChange(oui);
    },
  });
  return el;
}

// Santé, SAV

/** Relance une source ; tourne et se grise tant qu'elle est en cours. */
export function boutonRelancer(enCours, quandClic) {
  return h(
    "button",
    { class: enCours ? "relancer-source tourne" : "relancer-source", disabled: enCours, onclick: quandClic },
    icone("relancer"),
    enCours ? "En cours" : "Relancer",
  );
}

// le bouton de chaque panneau

export const RESEAUX = ["tiktok", "instagram", "youtube", "facebook"];

// la collecte vue par la barre : [{ id, nom, passage }], les ids en cours, et par id les clics encore dans leur tour minimum
const collecte = { sources: [], enCours: new Set(), clics: new Map() };
// la barre ne voit pas une source qui finit en moins de 400 ms : l'icône tourne au moins ce temps après un clic
const TOUR_MINIMUM = 2000;

export function suivreCollecte(sources, enCours) {
  collecte.sources = sources;
  collecte.enCours = new Set(enCours);
  majActualisers();
}

function majActualisers() {
  for (const el of document.querySelectorAll(".actualiser")) majActualiser(el);
}

function compterClics(sources, pas) {
  for (const id of sources) collecte.clics.set(id, (collecte.clics.get(id) ?? 0) + pas);
  majActualisers();
}

const tourneSource = (id) => collecte.enCours.has(id) || collecte.clics.get(id) > 0;

/** Relance les sources qui nourrissent un panneau ; tourne tant que l'une d'elles est en cours. */
export function actualiser(...sources) {
  const el = h(
    "button",
    {
      class: "bouton-panneau survolable actualiser",
      "data-sources": sources.join(" "),
      onclick: () => {
        compterClics(sources, 1);
        setTimeout(() => compterClics(sources, -1), TOUR_MINIMUM);
        Promise.all(sources.map((source) => appeler("relancer", { source }))).catch(console.error);
      },
    },
    icone("relancer"),
  );
  majActualiser(el);
  return el;
}

function majActualiser(el) {
  const ids = el.dataset.sources.split(" ");
  const tourne = ids.some(tourneSource);
  el.classList.toggle("tourne", tourne);
  el.disabled = tourne;
  const maintenant = Date.now();
  const lues = collecte.sources
    .filter((s) => ids.includes(s.id))
    .map((s) => s.nom + " " + (tourneSource(s.id) ? "en cours" : s.passage ? F.ilYa(s.passage.fin, maintenant) : "jamais lue"));
  el.title = "Rafraîchir ce panneau\n" + lues.join("\n");
}
