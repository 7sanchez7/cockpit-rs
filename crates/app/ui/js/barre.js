// La fenêtre de la barre de menus (Mac) ou de la barre des tâches (Windows), portée de BarreMenus.swift :
// les chiffres du jour, ce qu'il reste à traiter, les 5 derniers événements. Échap la cache, Entrée ouvre le cockpit.
// Elle se relit à chaque version de la base, quand elle reprend le focus et toutes les 30 s, puis donne sa hauteur à Rust.

import { appeler, ecouter } from "./pont.js";
import { h, pastille, ligneFil, vide } from "./composants.js";

const HORLOGE = 30_000;
let hauteur = 0;

function signaler(e) {
  console.error(e);
}

function ouvrir(ecran = null) {
  appeler("barre_ouvrir", { ecran }).catch(signaler);
}

async function rendre() {
  const d = await appeler("barre");
  const sav = d.aTraiter;
  const contenu = h(
    "div",
    { class: "barre-menus" },
    h("div", { class: "tete" }, h("span", { class: "h2" }, "Cockpit"), h("span", { class: "collecte" }, d.collecte)),
    h("div", { class: "chiffres-barre" }, d.chiffres.map((c) => h("div", { class: "chiffre-barre" }, h("span", { class: "libelle" }, c.libelle), h("span", { class: "valeur chiffres" }, c.valeur)))),
    h("button", { class: "ligne-sav", onclick: () => ouvrir("aTraiter") }, h("span", null, "SAV"), pastille(String(sav), { couleur: sav > 0 ? "var(--ambre)" : "var(--secondaire)" })),
    h("div", { class: "fil" }, d.fil.length ? d.fil.map((e) => ligneFil(e)) : vide("Rien de nouveau.")),
    h(
      "div",
      { class: "boutons" },
      h("button", { class: "bouton-reglage principal", onclick: () => ouvrir() }, "Ouvrir"),
      h("button", { class: "bouton-reglage", onclick: () => appeler("relancer", { source: null }).catch(signaler) }, "Actualiser"),
      h("span", { class: "espace" }),
      h("button", { class: "bouton-reglage", onclick: () => appeler("quitter").catch(signaler) }, "Quitter"),
    ),
  );
  document.body.replaceChildren(contenu);
  const mesure = Math.ceil(contenu.getBoundingClientRect().height);
  if (mesure !== hauteur) {
    hauteur = mesure;
    await appeler("barre_taille", { hauteur });
  }
}

function relire() {
  rendre().catch(signaler);
}

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") appeler("barre_cacher").catch(signaler);
  else if (e.key === "Enter") ouvrir();
});
window.addEventListener("focus", relire);
ecouter("cockpit", relire);
setInterval(relire, HORLOGE);
relire();
