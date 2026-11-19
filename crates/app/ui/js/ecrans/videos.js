// Vidéos : les comptes suivis et leurs vidéos.

import { vide } from "../composants.js";

export default {
  titre: "Vidéos",
  sousTitre: "Aucun compte suivi",
  rendre(racine) {
    racine.replaceChildren(vide("À venir."));
  },
};
