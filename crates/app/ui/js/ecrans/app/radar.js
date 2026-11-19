// Le radar du score : un axe par composante, deux anneaux (50 et 100), le polygone des notes, les noms autour.

import { h, s } from "../../composants.js";

/** axes : [{ nom, note | null }] ; une note absente compte zéro et grise son nom. */
export function radar(axes, aide) {
  const n = axes.length;
  const angle = (i) => -Math.PI / 2 + (i * 2 * Math.PI) / n;
  // repère : rayon 1 pour l'anneau 100, les noms à 1 + écart
  const pt = (i, r) => [r * Math.cos(angle(i)), r * Math.sin(angle(i))];
  const anneau = (r) => axes.map((_, i) => pt(i, r).map((v) => v.toFixed(4)).join(",")).join(" ");
  const notes = axes.map((a, i) => pt(i, (a.note ?? 0) / 100).map((v) => v.toFixed(4)).join(",")).join(" ");
  return h(
    "div",
    { class: "radar", title: aide },
    s(
      "svg",
      { viewBox: "-1 -1 2 2", overflow: "visible" },
      s("polygon", { class: "radar-grille", points: anneau(0.5) }),
      s("polygon", { class: "radar-grille", points: anneau(1) }),
      axes.map((_, i) => s("line", { class: "radar-grille", x1: 0, y1: 0, x2: pt(i, 1)[0], y2: pt(i, 1)[1] })),
      s("polygon", { class: "radar-notes", points: notes }),
    ),
    axes.map((a, i) => {
      const c = Math.cos(angle(i));
      const sn = Math.sin(angle(i));
      return h(
        "span",
        {
          class: a.note == null ? "radar-nom absent" : "radar-nom",
          style: { "--x": c, "--y": sn, "--ax": 0.5 - 0.5 * c, "--ay": 0.5 - 0.5 * sn },
        },
        a.nom,
      );
    }),
  );
}
