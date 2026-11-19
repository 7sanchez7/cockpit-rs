// Le héros de l'accueil : le revenu de la période, son écart, et la courbe du revenu cumulé jour par jour
// (aire lissée, axe en euros à gauche, viseur et bulle au survol, dernier point marqué).

import { h, s, pastille, enTetePanneau, couleurTon, vide, tracer, nombreJeton } from "../../composants.js";
import * as F from "../../format.js";
import { piluleMenu, lienSecondaire } from "./outils.js";

/** heros : la lecture `heros` de la commande accueil ; choisir(periode) relit l'écran. */
export function heros(ctx, heros, choisir) {
  const libelle = heros.periodes.find((p) => p.id === heros.periode)?.libelle ?? "";
  const periode = piluleMenu(libelle, "Changer de période", () =>
    heros.periodes.map((p) => ({ libelle: p.libelle, actif: p.id === heros.periode, faire: () => choisir(p.id) })),
  );
  return h(
    "section",
    { class: "panneau verre heros-panneau" },
    enTetePanneau("Revenu", periode),
    h("div", { class: "heros-valeur" }, h("span", { class: "heros" }, heros.texte), heros.delta != null && pastille(heros.delta, { couleur: couleurTon(heros.ton) })),
    h(
      "div",
      { class: "heros-detail" },
      h("span", null, heros.detail || F.vide),
      h("div", { class: "espace" }),
      lienSecondaire("Tous les chiffres", () => ctx.aller("data")),
    ),
    heros.points.length < 2 ? h("div", { class: "heros-courbe vide-courbe" }, vide("RevenueCat n'a pas encore écrit deux jours de revenu.")) : courbe(heros.points),
    h("div", { class: "heros-pied" }, heros.pied),
  );
}

let gradients = 0;

function courbe(points) {
  const n = points.length;
  const valeurs = points.map((p) => p.euros);
  const bas = Math.min(...valeurs);
  const haut = Math.max(...valeurs);
  // un plancher sous le plus bas : la courbe ne colle ni au bas ni au haut du cadre
  const marge = Math.max((haut - bas) * 0.3, 1);
  const min = bas - marge;
  const max = haut + marge / 2;
  const y = (v) => (1 - (v - min) / (max - min)) * 100;
  const x = (i) => (i / (n - 1)) * 100;
  const pts = valeurs.map((v, i) => [x(i), y(v)]);
  const ligne = lisse(pts);
  const id = "lavis-heros-" + ++gradients;

  const graduations = reperes(min, max);
  const pasX = Math.max(Math.floor(n / 5), 1);
  const jours = points.map((_, i) => i).filter((i) => i % pasX === 0);

  const trait = s("path", { class: "heros-trait", d: ligne, pathLength: 1 });
  const svg = s(
    "svg",
    { viewBox: "0 0 100 100", preserveAspectRatio: "none" },
    s("defs", null, s("linearGradient", { id, x1: 0, y1: 0, x2: 0, y2: 1 }, s("stop", { offset: 0, class: "lavis-haut" }), s("stop", { offset: 1, class: "lavis-bas" }))),
    graduations.map((v) => s("line", { class: "heros-grille", x1: 0, x2: 100, y1: y(v), y2: y(v) })),
    jours.map((i) => s("line", { class: "heros-grille", x1: x(i), x2: x(i), y1: 0, y2: 100 })),
    s("path", { class: "heros-aire", d: `${ligne}L100,100L0,100Z`, fill: `url(#${id})` }),
    trait,
  );
  tracer(trait);

  const dernier = h("i", { class: "heros-point dernier", style: { left: x(n - 1) + "%", top: pts[n - 1][1] + "%" } });
  const viseur = h("div", { class: "heros-viseur" });
  const vise = h("i", { class: "heros-point" });
  const bulle = h("div", { class: "heros-bulle" });
  const trace = h("div", { class: "heros-trace" }, svg, dernier, viseur, vise, bulle);
  trace.addEventListener("mousemove", (e) => {
    const r = trace.getBoundingClientRect();
    const i = Math.round(Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)) * (n - 1));
    viseur.style.left = vise.style.left = x(i) + "%";
    vise.style.top = pts[i][1] + "%";
    bulle.replaceChildren(h("span", { class: "jour" }, points[i].jour), h("b", null, points[i].texte));
    const l = (x(i) / 100) * r.width;
    bulle.style.left = Math.min(Math.max(0, l + nombreJeton("--e-xl")), r.width - bulle.offsetWidth) + "px";
  });

  return h(
    "div",
    { class: "heros-courbe" },
    h("div", { class: "heros-axe-y" }, graduations.map((v) => h("span", { style: { top: y(v) + "%" } }, F.entier(v) + " €"))),
    trace,
    h("div", { class: "heros-axe-x" }, jours.map((i) => h("span", { style: { left: x(i) + "%" } }, points[i].jour))),
  );
}

// des repères ronds (1, 2 ou 5 × 10^k), environ quatre dans le domaine
function reperes(min, max) {
  const brut = (max - min) / 4;
  const p = 10 ** Math.floor(Math.log10(brut));
  const pas = [1, 2, 5, 10].map((k) => k * p).find((c) => c >= brut);
  const out = [];
  for (let v = Math.ceil(min / pas) * pas; v <= max; v += pas) out.push(v);
  return out;
}

// une courbe monotone (Fritsch-Carlson) : lisse sans dépasser les points, comme .monotone en Swift Charts
function lisse(pts) {
  const n = pts.length;
  const d = pts.slice(1).map((p, i) => (p[1] - pts[i][1]) / (p[0] - pts[i][0]));
  const m = pts.map((_, i) => (i === 0 ? d[0] : i === n - 1 ? d[n - 2] : d[i - 1] * d[i] <= 0 ? 0 : (d[i - 1] + d[i]) / 2));
  for (let i = 0; i < n - 1; i++) {
    if (d[i] === 0) {
      m[i] = m[i + 1] = 0;
      continue;
    }
    const a = m[i] / d[i];
    const b = m[i + 1] / d[i];
    const t = a * a + b * b;
    if (t > 9) {
      const k = 3 / Math.sqrt(t);
      m[i] = k * a * d[i];
      m[i + 1] = k * b * d[i];
    }
  }
  let chemin = `M${pts[0][0].toFixed(2)},${pts[0][1].toFixed(2)}`;
  for (let i = 0; i < n - 1; i++) {
    const [x0, y0] = pts[i];
    const [x1, y1] = pts[i + 1];
    const dx = (x1 - x0) / 3;
    chemin += `C${(x0 + dx).toFixed(2)},${(y0 + m[i] * dx).toFixed(2)},${(x1 - dx).toFixed(2)},${(y1 - m[i + 1] * dx).toFixed(2)},${x1.toFixed(2)},${y1.toFixed(2)}`;
  }
  return chemin;
}
