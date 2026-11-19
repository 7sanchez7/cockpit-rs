// SAV : les avis, messages et tâches qui attendent une réponse.

import { vide } from "../composants.js";

export default {
  titre: "SAV",
  sousTitre: "Rien à répondre",
  rendre(racine) {
    racine.replaceChildren(vide("À venir."));
  },
};
