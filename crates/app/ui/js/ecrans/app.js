// Accueil : les chiffres du jour et ce qui a bougé, comparés au même jour de la semaine dernière.
// Un écran exporte { titre, sousTitre, rendre(racine, ctx), defile?, business? } ; le contexte est décrit en tête de ../app.js.

import { vide } from "../composants.js";
import * as F from "../format.js";

export default {
  titre: "Accueil",
  sousTitre: "",
  rendre(racine, ctx) {
    ctx.page.sousTitre(`${F.dateLongue(ctx.maintenant)}, comparé à ${F.jourSemaine(ctx.maintenant)} dernier à la même heure`);
    racine.replaceChildren(vide("À venir."));
  },
};
