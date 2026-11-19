// Data : les courbes et les tableaux d'un business, ou de tous.

import { vide } from "../composants.js";
import * as F from "../format.js";

export default {
  titre: "Data",
  sousTitre: "",
  rendre(racine, ctx) {
    ctx.page.sousTitre(`${F.dateLongue(ctx.maintenant)}, comparé à ${F.jourSemaine(ctx.maintenant)} dernier à la même heure`);
    racine.replaceChildren(vide("À venir."));
  },
};
