// Data : deux vues, l'app et l'argent. L'app : l'arbre des installs et les ronds de chaque nœud, puis la rétention, le
// rond et le bilan de la période ; un build choisi la remplace par l'arbre et la rétention de ses installs. L'argent : le revenu de la période, qui ne se découpe pas par build. Tout arrive écrit par la commande
// `data` ; l'arbre lit la sienne.

import { h, grilleKpi, rangee, bloc, panneau, enTetePanneau, pastille, courbe, barres, ligneFil, vide, segmente, rendu, actualiser } from "../composants.js";
import * as F from "../format.js";
import { etat } from "./app/etat.js";
import { choixBoutique, piluleMenu, carteKpi } from "./app/outils.js";
import { arbre } from "./data/arbre.js";

const MODES = [
  { id: "app", libelle: "App" },
  { id: "argent", libelle: "Argent" },
];

let dernier = null;

async function rendre(racine, ctx) {
  const app = etat.mode === "app";
  const vue = await ctx.appeler("data", { business: ctx.business, periode: etat.periode, build: app ? etat.build : null });
  dernier = { racine, ctx };
  // un build absent de ce business retombe sur la période
  if (app) etat.build = vue.build;
  ctx.page.sousTitre(vue.sousTitre);
  ctx.page.outils(
    ...[
      segmente(MODES, etat.mode, changerMode),
      choixBoutique(ctx, vue.boutique),
      app && choixBuild(vue),
      // en vue Argent, la période ne fait pas oublier le build de la vue App
      segmente(vue.periodes, vue.periode, (p) => choisir(p, app ? null : etat.build)),
    ].filter(Boolean),
  );
  const sections = app
    ? [
        titreSection("Arbre des installs", vue.arbre.texte, ["posthog", "revenuecat"]),
        await arbre(ctx, vue.arbre),
        retention(vue.retention),
        // null tant que les événements manquent, ou en vue build
        vue.rond && section(vue.rond, ["posthog"]),
        vue.bilan && section(vue.bilan, ["posthog"]),
      ]
    : argent(vue.argent);
  racine.replaceChildren(...sections.flat(Infinity).filter(Boolean));
}

function changerMode(mode) {
  etat.mode = mode;
  choisir(null, etat.build);
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

/** sources : celles que relance le bouton de la section. */
function titreSection(titre, texte, sources) {
  return h(
    "div",
    { class: "titre-data" },
    h("div", { class: "titre-ligne" }, h("span", { class: "titre" }, titre), h("div", { class: "espace" }), actualiser(...sources)),
    h("span", { class: "texte" }, texte),
  );
}

/** Le titre de la section et ses cartes. */
function section(s, sources) {
  const grille = grilleKpi(...s.kpis.map((c, i) => carteKpi(c, `data.${s.titre}.${i}`)));
  if ([3, 6].includes(s.kpis.length)) grille.classList.add("trois");
  return [titreSection(s.titre, s.texte, sources), grille];
}

function argent(a) {
  return [
    section(a.section, ["revenuecat", "firebase"]),
    objectif(a.objectif),
    rangee(
      [2, 1],
      [
        h(
          "div",
          { class: "pile" },
          bloc(enTetePanneau("MRR", actualiser("revenuecat")), courbe({ series: [{ nom: "MRR", valeurs: a.mrr.valeurs, couleur: "var(--accent)" }], etiquettes: a.mrr.etiquettes, format: (v) => F.entier(v) + " €", hauteur: "var(--t-graphique-haut)" })),
          bloc(enTetePanneau("Achats par jour", actualiser("revenuecat")), barres(a.achats.valeurs, { courante: a.achats.courante, axe: a.achats.axe, aide: (i) => a.achats.aides[i], hauteur: "var(--t-graphique-haut)" })),
        ),
        bloc(
          enTetePanneau("Achats et essais", h("span", { class: "accessoire-texte" }, a.ventes.compte), actualiser("revenuecat", "posthog")),
          a.ventes.evenements.length ? h("div", { class: "fil" }, a.ventes.evenements.map((e) => ligneFil(e, { jour: true }))) : vide("Aucun achat ni essai dans le fil sur la période."),
        ),
      ],
      { meneur: 0 },
    ),
  ];
}

function objectif(o) {
  if (!o) return panneau({ titre: "Objectif", accessoire: actualiser("revenuecat") }, vide("RevenueCat n'a pas encore écrit le MRR du jour."));
  return panneau(
    { titre: o.titre, accessoire: [pastille(o.pourcent, { couleur: "var(--accent)" }), actualiser("revenuecat")], classe: "objectif" },
    h("div", { class: "objectif-ligne" }, h("span", { class: "objectif-mrr" }, o.mrr), h("span", { class: "objectif-de" }, "de MRR"), h("div", { class: "espace" }), h("span", { class: "objectif-reste" }, o.reste)),
    h("div", { class: "objectif-jauge verre capsule" }, h("i", { style: { "--part": Math.min(Math.max(o.part, 0), 1) } })),
    o.rythme && h("div", { class: "objectif-rythme" }, o.rythme),
  );
}

function retention(r) {
  return [
    section(r.section, ["posthog"]),
    bloc(
      enTetePanneau("Cohortes", h("span", { class: "accessoire-texte" }, r.sousTitre), actualiser("posthog")),
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
