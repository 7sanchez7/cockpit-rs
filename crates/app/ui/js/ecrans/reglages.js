// Réglages : business, clés, notifications et collecte. Hors des pilules, sans choix de business.

import { vide } from "../composants.js";

export default {
  titre: "Réglages",
  sousTitre: "Business, clés, notifications et collecte",
  business: false,
  rendre(racine) {
    racine.replaceChildren(vide("À venir."));
  },
};
