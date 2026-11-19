// Apps de potes : les apps suivies dans l'App Store, leurs rangs et leurs notes.

import { vide } from "../composants.js";

export default {
  titre: "Apps de potes",
  sousTitre: "Aucune app suivie",
  rendre(racine) {
    racine.replaceChildren(vide("À venir."));
  },
};
