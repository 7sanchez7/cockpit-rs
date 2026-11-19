// Santé : chaque source, son dernier passage et ses pannes.

import { vide } from "../composants.js";
import * as F from "../format.js";

function resume(ctx) {
  const n = ctx.sources.length;
  const pannes = ctx.sources.filter((s) => s.passage?.etat === "erreur").length;
  const fins = ctx.sources.map((s) => s.passage && Date.parse(s.passage.fin)).filter(Boolean);
  return [
    F.nombre(n, "source") + (pannes ? `, ${F.entier(pannes)} en panne` : ", aucune en panne"),
    fins.length && "dernier passage " + F.ilYa(Math.max(...fins), ctx.maintenant),
    ctx.pause && "collecte en pause",
  ]
    .filter(Boolean)
    .join(", ");
}

export default {
  titre: "Santé",
  sousTitre: "",
  rendre(racine, ctx) {
    ctx.page.sousTitre(resume(ctx));
    racine.replaceChildren(vide("À venir."));
  },
};
