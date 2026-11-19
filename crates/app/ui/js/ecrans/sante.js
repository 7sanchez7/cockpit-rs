// Santé : les sources répondent-elles, l'IA tient-elle (reprises, 429, latence), combien coûte la journée.
// Tout arrive formaté de la commande `sante` ; les couleurs y sont des noms de variables de theme.css.

import { h, panneau, rangee, segmente, grilleKpi, kpi, pastille, point, vide, legende, boutonRelancer } from "../composants.js";
import * as F from "../format.js";

const v = (nom) => `var(--${nom})`;
const FORMATS = { entier: F.entier, euros: F.euros };

// l'état local, gardé d'un rendu à l'autre : le jour du panneau IA, la source du journal
const local = { hier: false, source: null };

function charger(ctx) {
  return ctx.appeler("sante", { business: ctx.business, hier: local.hier, source: local.source });
}

/** Un tableau à colonnes fixes ; `classe` donne la grille (theme.css), `gauche` les colonnes alignées à gauche. */
function tableau(classe, titres, gauche, lignes) {
  const cellules = (c) => c.map((x, i) => h("div", { class: gauche.includes(i) ? "cellule gauche" : "cellule" }, x));
  return h(
    "div",
    { class: "tableau " + classe },
    h("div", { class: "rang tete" }, cellules(titres)),
    lignes.map((l) => h("div", { class: "rang" }, cellules(l.cellules), l.dessous)),
  );
}

function nomPoint(couleur, nom, aide) {
  return h("span", { class: "nom-point", title: aide }, point(v(couleur)), h("span", { class: "nom" }, nom));
}

function erreur(e) {
  return e && h("div", { class: "erreur-passage", style: { color: v(e.couleur) }, title: e.entiere }, e.ligne);
}

// graphiques

/** Un axe aux pas ronds (1, 2, 2,5 ou 5 × 10ⁿ), environ trois repères : le haut du domaine et les repères. */
function axeY(max) {
  if (!(max > 0)) return { haut: 1, reperes: [0] };
  const brut = max / 3;
  const p = 10 ** Math.floor(Math.log10(brut));
  const pas = [1, 2, 2.5, 5, 10].map((m) => m * p).find((x) => x >= brut);
  const haut = pas * Math.ceil(max / pas - 1e-9);
  return { haut, reperes: Array.from({ length: Math.round(haut / pas) + 1 }, (_, i) => i * pas) };
}

function graphiqueBarres(haut, reperes, format, colonnes, etiquettes, attrs) {
  const bas = (r) => ({ bottom: (r / haut) * 100 + "%" });
  return h(
    "div",
    { class: "graphique-barres", ...attrs },
    h("div", { class: "axe-y" }, reperes.map((r) => h("span", { style: bas(r) }, format(r)))),
    h("div", { class: "trace" }, reperes.map((r) => h("i", { class: "repere", style: bas(r) })), colonnes),
    h("div", { class: "axe-x" }, etiquettes.map((t) => h("span", null, t))),
  );
}

/** Les requêtes par heure, une part de barre par fournisseur. */
function barresHeures(ia) {
  const totaux = Array.from({ length: 24 }, (_, i) => ia.reduce((t, f) => t + f.heures[i], 0));
  const { haut, reperes } = axeY(Math.max(...totaux));
  const colonnes = totaux.map((_, i) =>
    h("div", { class: "colonne empilee" }, ia.map((f) => f.heures[i] > 0 && h("i", { style: { height: (f.heures[i] / haut) * 100 + "%", "--c": v(f.couleur) } }))),
  );
  return graphiqueBarres(haut, reperes, F.compact, colonnes, totaux.map((_, i) => (i % 4 === 0 ? `${i} h` : null)));
}

// sources

function interrupteur(oui, libelle, quandChange) {
  return h(
    "label",
    { class: oui ? "interrupteur oui" : "interrupteur" },
    h("input", { type: "checkbox", checked: oui, onchange: (e) => quandChange(e.currentTarget.checked) }),
    h("i"),
    libelle,
  );
}

function panneauSources(d, ctx, redessiner) {
  const lignes = d.sources.map((s) => ({
    cellules: [
      nomPoint(s.etat.couleur, s.nom, s.etat.texte),
      s.cadence,
      h("span", { title: s.dernierAide }, s.dernier),
      s.duree,
      s.lignes,
      s.succes,
      boutonRelancer(s.enCours, () => ctx.appeler("relancer", { source: s.id }).then(redessiner)),
    ],
    dessous: s.erreur && h("div", { class: "sous-ligne" }, erreur(s.erreur)),
  }));
  return panneau(
    {
      titre: "Sources",
      accessoire: interrupteur(d.pause, "Collecte en pause", (oui) => ctx.appeler("pause", { oui }).then(redessiner)),
    },
    tableau("table-sources", ["Source", "Cadence", "Dernier passage", "Durée", "Lignes", "Dernier succès", ""], [0], lignes),
  );
}

// IA

function contenuIA(ia) {
  if (!ia.length) return vide("Aucun appel IA relevé ce jour-là : la source Firebase doit tourner.");
  const taux = (t) => h("span", { title: t.aide, style: { color: t.alerte ? v("ambre") : null } }, t.texte);
  return [
    tableau(
      "table-ia",
      ["Fournisseur", "Requêtes", "Part", "429", "Erreurs", "Latence"],
      [0],
      ia.map((f) => ({
        cellules: [h("span", { class: "nom" }, f.nom), h("span", { class: "fort" }, f.requetes), f.part, taux(f.q429), taux(f.erreurs), f.latence],
      })),
    ),
    legende(ia.map((f) => ({ nom: f.nom, couleur: v(f.couleur) }))),
    barresHeures(ia),
  ];
}

function panneauIA(d, ctx) {
  const corps = h("div", { class: "corps-panneau" }, contenuIA(d.ia));
  const choisir = async (hier) => {
    local.hier = hier;
    corps.replaceChildren(contenuIA((await charger(ctx)).ia));
  };
  return panneau(
    { titre: "IA par fournisseur", accessoire: segmente([{ id: false, libelle: "Aujourd'hui" }, { id: true, libelle: "Hier" }], local.hier, choisir) },
    corps,
  );
}

// coût

function panneauCout(cout) {
  const jours = cout.jours;
  if (!jours.some((j) => j.valeur != null)) {
    return panneau({ titre: "Coût IA, 14 jours" }, vide("Aucun coût relevé : la source Firebase doit tourner."));
  }
  const aujourdhui = jours.length - 1;
  const texte = h("div", { class: "texte-cout" }, jours[aujourdhui].texte);
  const { haut, reperes } = axeY(Math.max(...jours.map((j) => j.valeur ?? 0)));
  const colonnes = jours.map((j, i) =>
    h(
      "div",
      { class: i === aujourdhui ? "colonne vue" : "colonne", onmouseenter: () => voir(i) },
      j.valeur != null && h("i", { style: { height: (j.valeur / haut) * 100 + "%" } }),
    ),
  );
  function voir(i) {
    colonnes.forEach((c, k) => c.classList.toggle("vue", k === i));
    texte.textContent = jours[i].texte;
  }
  const graphique = graphiqueBarres(haut, reperes, F.euros, colonnes, jours.map((j) => j.etiquette), { onmouseleave: () => voir(aujourdhui) });
  return panneau(
    { titre: "Coût IA, 14 jours", accessoire: cout.mois && pastille(cout.mois, { neutre: true }) },
    graphique,
    h("div", { class: "notes-cout" }, texte, h("div", { class: "note-cout" }, "Texte et voix ; la voix suit le jour UTC.")),
  );
}

// journal

function contenuJournal(journal) {
  if (!journal.length) return vide("Aucun passage : la collecte n'a pas encore tourné.");
  return tableau(
    "table-journal",
    ["Début", "Source", "Durée", "Lignes", "Erreur"],
    [0, 1, 4],
    journal.map((p) => ({
      cellules: [
        p.debut,
        nomPoint(p.couleur, p.source),
        p.duree,
        p.lignes,
        p.erreur && h("span", { class: "erreur-journal", style: { color: v(p.erreur.couleur) }, title: p.erreur.entiere }, p.erreur.ligne),
      ],
    })),
  );
}

function panneauJournal(d, ctx) {
  const corps = h("div", { class: "corps-panneau" }, contenuJournal(d.journal));
  const menu = h(
    "select",
    {
      class: "menu-source",
      onchange: async (e) => {
        local.source = e.currentTarget.value || null;
        corps.replaceChildren(contenuJournal((await charger(ctx)).journal));
      },
    },
    h("option", { value: "" }, "Toutes les sources"),
    d.choix.map((c) => h("option", { value: c.id, selected: c.id === local.source }, c.nom)),
  );
  return panneau({ titre: "Journal", accessoire: menu }, corps);
}

async function dessiner(racine, ctx) {
  const d = await charger(ctx);
  const redessiner = () => dessiner(racine, ctx);
  ctx.page.sousTitre(d.sousTitre);
  racine.replaceChildren(
    grilleKpi(
      ...d.kpis.map((k) =>
        kpi({ titre: k.titre, valeur: k.valeur, format: FORMATS[k.format], delta: k.delta, ton: k.ton, serie: k.serie, cle: "sante." + k.titre }),
      ),
    ),
    panneauSources(d, ctx, redessiner),
    rangee([3, 2], [panneauIA(d, ctx), panneauCout(d.cout)]),
    panneauJournal(d, ctx),
  );
}

export default {
  titre: "Santé",
  sousTitre: "",
  rendre: dessiner,
};
