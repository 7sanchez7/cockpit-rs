// Veille : les vidéos qui montent sur les mots-clés suivis. La page ne défile pas, ses colonnes défilent.

import { vide } from "../composants.js";

export default {
  titre: "Veille",
  sousTitre: "Aucun mot-clé",
  defile: false,
  rendre(racine) {
    racine.replaceChildren(vide("À venir."));
  },
};
