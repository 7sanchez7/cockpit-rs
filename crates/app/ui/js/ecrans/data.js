// Data : toute la data de l'app sur la période (l'argent, l'acquisition, la conversion, l'arbre des installs, la
// rétention, l'usage). Un build choisi remplace la période : la conversion, l'arbre et la rétention de ses installs.
// Tout arrive écrit par la commande `data` ; l'arbre lit la sienne.

import { h, grilleKpi, rangee, bloc, panneau, enTetePanneau, pastille, courbe, barres, ligneFil, vide, segmente, rendu } from "../composants.js";
import * as F from "../format.js";
import { etat } from "./app/etat.js";
import { choixBoutique, piluleMenu, carteKpi, taux } from "./app/outils.js";
import { arbre } from "./data/arbre.js";

let dernier = null;

async function rendre(racine, ctx) {
  const vue = await ctx.appeler("data", { business: ctx.business, periode: etat.periode, build: etat.build });
  dernier = { racine, ctx };
  // un build absent de ce business retombe sur la période
  etat.build = vue.build;
  ctx.page.sousTitre(vue.sousTitre);
  ctx.page.outils(
    choixBoutique(ctx, vue.boutique),
    choixBuild(vue),
    segmente(vue.periodes, vue.periode, (p) => choisir(p, null)),
  );
  const a = await arbre(ctx, vue.arbre);
  const arbreSection = [titreSection("Arbre des installs", vue.arbre.texte), a];
  racine.replaceChildren(
    ...(vue.build
      ? [conversion(vue.conversion), arbreSection, retention(vue.retention)]
      : [argent(vue.argent), section(vue.acquisition), conversion(vue.conversion), arbreSection, retention(vue.retention), section(vue.usage)]
    )
      .flat(Infinity)
      .filter(Boolean),
  );
}

function choisir(periode, build) {
  if (!dernier) return;
  etat.periode = periode ?? etat.periode;
  etat.build = build;
  rendu.calme = true;
  rendre(dernier.racine, dernier.ctx);
}

function choixBuild(vue) {
  const libelle = vue.build ? `Build ${vue.build}` : "Tous les builds";
  return piluleMenu(libelle, "Voir les installs d'un seul build", () => [
    { libelle: "Tous les builds", actif: vue.build == null, faire: () => choisir(null, null) },
    ...vue.builds.map((b) => ({ libelle: b.libelle, actif: b.numero === vue.build, faire: () => choisir(null, b.numero) })),
  ]);
}

function titreSection(titre, texte) {
  return h("div", { class: "titre-data" }, h("span", { class: "titre" }, titre), h("span", { class: "texte" }, texte));
}

/** Le titre de la section et ses cartes. */
function section(s) {
  const grille = grilleKpi(...s.kpis.map((c, i) => carteKpi(c, `data.${s.titre}.${i}`)));
  if (s.kpis.length === 3) grille.classList.add("trois");
  return [titreSection(s.titre, s.texte), grille];
}

function argent(a) {
  return [
    section(a.section),
    objectif(a.objectif),
    rangee(
      [2, 1],
      [
        h(
          "div",
          { class: "pile" },
          bloc(enTetePanneau("MRR"), courbe({ series: [{ nom: "MRR", valeurs: a.mrr.valeurs, couleur: "var(--accent)" }], etiquettes: a.mrr.etiquettes, format: (v) => F.entier(v) + " €", hauteur: "var(--t-graphique-haut)" })),
          bloc(enTetePanneau("Achats par jour"), barres(a.achats.valeurs, { courante: a.achats.courante, axe: a.achats.axe, aide: (i) => a.achats.aides[i], hauteur: "var(--t-graphique-haut)" })),
        ),
        bloc(
          enTetePanneau("Achats et essais", h("span", { class: "accessoire-texte" }, a.ventes.compte)),
          a.ventes.evenements.length ? h("div", { class: "fil" }, a.ventes.evenements.map((e) => ligneFil(e, { jour: true }))) : vide("Aucun achat ni essai dans le fil sur la période."),
        ),
      ],
      { meneur: 0 },
    ),
  ];
}

function objectif(o) {
  if (!o) return panneau({ titre: "Objectif" }, vide("RevenueCat n'a pas encore écrit le MRR du jour."));
  return panneau(
    { titre: o.titre, accessoire: pastille(o.pourcent, { couleur: "var(--accent)" }), classe: "objectif" },
    h("div", { class: "objectif-ligne" }, h("span", { class: "objectif-mrr" }, o.mrr), h("span", { class: "objectif-de" }, "de MRR"), h("div", { class: "espace" }), h("span", { class: "objectif-reste" }, o.reste)),
    h("div", { class: "objectif-jauge verre capsule" }, h("i", { style: { "--part": Math.min(Math.max(o.part, 0), 1) } })),
    o.rythme && h("div", { class: "objectif-rythme" }, o.rythme),
  );
}

function conversion(c) {
  return [section(c.section), c.portes && portes(c.portes)];
}

function portes(lignes) {
  return bloc(
    enTetePanneau("Portes du paywall", h("span", { class: "accessoire-texte" }, "d'où il s'ouvre")),
    lignes.length
      ? h(
          "div",
          { class: "table-portes" },
          ["Porte", "Vues", "Essais", "Directs", "Conv."].map((t) => h("span", { class: "entete-table" }, t)),
          lignes.map((p) => [h("span", { class: "nom" }, p.nom), h("span", null, p.vues), h("span", null, p.essais), h("span", null, p.directs), taux(p.conversion)]),
        )
      : vide("Aucune vue du paywall sur la période."),
  );
}

function retention(r) {
  return [
    section(r.section),
    bloc(
      enTetePanneau("Cohortes", h("span", { class: "accessoire-texte" }, r.sousTitre)),
      r.cohortes.length
        ? h(
            "div",
            { class: "table-cohortes" },
            ["Jour", "Installs", "J1", "J7", "J14", "J30"].map((t) => h("span", { class: "entete-table" }, t)),
            r.cohortes.map((c) => {
              const jour = { class: c.ouverte ? "jour ouverte" : "jour", title: c.ouverte ? "Cohorte ouverte, J1 pas encore atteint" : null };
              return [h("span", jour, c.jour), h("span", jour, c.installs), c.cases.map((k) => h("span", { class: "case-cohorte", style: { "--opacite": k.opacite } }, k.texte))];
            }),
          )
        : vide("La source PostHog n'a pas encore calculé les cohortes."),
    ),
  ];
}

export default {
  titre: "Data",
  sousTitre: "",
  rendre,
};
