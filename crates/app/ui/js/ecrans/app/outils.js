// Les petits outils de l'Accueil et de Data : le menu ancré, la pilule à menu (période, build), le choix de boutique,
// la carte KPI écrite par Rust, le taux, le bouton qui replie un panneau latéral et ses icônes.

import { h, s, jeton, nombreJeton, sobre, segmente, kpi } from "../../composants.js";

/** Un menu de verre sous `ancre` (un élément) ou au point { x, y } ; items : { libelle, actif, faire } ou "-". */
export function menu(ancre, items) {
  const r = ancre instanceof Element ? ancre.getBoundingClientRect() : null;
  const left = r ? r.left : ancre.x;
  const top = r ? r.bottom + nombreJeton("--e-m") : ancre.y;
  const fermer = () => {
    el.remove();
    document.removeEventListener("mousedown", dehors, true);
    document.removeEventListener("keydown", echap, true);
  };
  const dehors = (e) => {
    if (!el.contains(e.target)) fermer();
  };
  const echap = (e) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      fermer();
    }
  };
  const el = h(
    "div",
    { class: "menu verre", style: { left: left + "px", top: top + "px" } },
    items.map((it) => (it === "-" ? h("hr") : h("button", { class: it.actif ? "actif" : null, onclick: () => (fermer(), it.faire()) }, it.libelle))),
  );
  document.body.append(el);
  // le menu reste dans la fenêtre
  const depasse = el.getBoundingClientRect().right - innerWidth + nombreJeton("--e-m");
  if (depasse > 0) el.style.left = left - depasse + "px";
  document.addEventListener("mousedown", dehors, true);
  document.addEventListener("keydown", echap, true);
  if (!sobre()) {
    el.animate([{ opacity: 0, transform: `scale(${jeton("--appui")})` }, { opacity: 1, transform: "none" }], {
      duration: nombreJeton("--duree-survol"),
      easing: jeton("--ressort-survol"),
    });
  }
}

/** La pilule de verre « libellé ⇕ » qui ouvre un menu. */
export function piluleMenu(libelle, aide, items) {
  const b = h("button", { class: "bouton-capsule verre capsule", title: aide, onclick: () => menu(b, items()) }, libelle, h("span", { class: "chevrons" }, "⇕"));
  return b;
}

/** Total, App Store ou Play Store, gardé dans le réglage `boutique` ; la base change de version, les écrans se relisent. */
export function choixBoutique(ctx, boutique) {
  return segmente(
    [
      { id: null, libelle: "Total" },
      { id: "app_store", libelle: "App Store" },
      { id: "play_store", libelle: "Play Store" },
    ],
    boutique,
    (id) => ctx.appeler("ecrire_reglage", { cle: "boutique", valeur: id }),
  );
}

/** Une carte `Carte` de Rust, déjà écrite ; `cle` garde le chiffre d'un rendu à l'autre. */
export function carteKpi(c, cle, { grande = false } = {}) {
  return kpi({ titre: c.titre, valeur: c.valeur, format: () => c.texte, delta: c.delta, ton: c.ton, note: c.note, serie: c.serie, inverse: c.inverse, grande, cle });
}

/** Un taux `Taux` : accent et gras s'il est le meilleur de sa colonne, gris si sa base est trop petite. */
export function taux(t) {
  return h("span", { class: ["taux", t.meilleur && "meilleur", !t.fiable && "fragile"].filter(Boolean).join(" "), title: t.aide || null }, t.texte);
}

// icônes sur la grille de 24 de composants.js
const ICONES = {
  barreGauche: [["rect", { x: 3, y: 5, width: 18, height: 14, rx: 3 }], ["path", { d: "M9,5v14" }]],
  barreDroite: [["rect", { x: 3, y: 5, width: 18, height: 14, rx: 3 }], ["path", { d: "M15,5v14" }]],
  oeilBarre: [["path", { d: "M2,12s3.5-7,10-7,10,7,10,7-3.5,7-10,7S2,12,2,12Z" }], ["circle", { cx: 12, cy: 12, r: 3 }], ["path", { d: "M4,4L20,20" }]],
  liste: [["path", { d: "M10,7h10M10,12h10M10,17h10" }], ["path", { d: "M4,7l1.5,1.5L8,6M4,12l1.5,1.5L8,11M4,17l1.5,1.5L8,16" }]],
};

export function icone(nom) {
  return s("svg", { class: "icone", viewBox: "0 0 24 24", "aria-hidden": "true" }, ICONES[nom].map(([tag, attrs]) => s(tag, attrs)));
}

/** Le bouton carré qui replie ou déplie un panneau latéral. */
export function boutonPanneau(nom, aide, faire) {
  return h("button", { class: "bouton-panneau survolable", title: aide, onclick: faire }, icone(nom));
}

/** « Texte → » : un lien discret vers un autre écran. */
export function lienSecondaire(texte, faire, fleche = "→") {
  return h("button", { class: "lien-secondaire", onclick: faire }, texte, h("span", { class: "fleche-lien" }, fleche));
}
