// Accueil : le revenu de la période en héros, six chiffres, la semaine, le score, les actifs, le fil et les vidéos ;
// les réseaux à gauche et le plan de Notes à droite, repliables. Tout arrive écrit par la commande `accueil`.

import { h, grilleKpi, rangee, bloc, enTetePanneau, legende, barres, surtitre, point, ligneFil, vide, pastille, chiffre, rendu } from "../composants.js";
import { etat } from "./app/etat.js";
import { choixBoutique, carteKpi } from "./app/outils.js";
import { heros } from "./app/heros.js";
import { radar } from "./app/radar.js";
import { panneauReseau, panneauPlan } from "./app/lateraux.js";

const LEGENDE = ["var(--blanc-barre)", "var(--blanc-repere)"];

// le dernier rendu : la période et les panneaux latéraux se redessinent sans attendre la base
let dernier = null;

async function rendre(racine, ctx) {
  const vue = await ctx.appeler("accueil", { business: ctx.business, periode: etat.periode });
  dernier = { racine, ctx, vue };
  ctx.page.sousTitre(vue.sousTitre);
  ctx.page.outils(choixBoutique(ctx, vue.boutique));
  lateraux(racine, ctx, vue);
  racine.replaceChildren(
    heros(ctx, vue.heros, choisirPeriode),
    kpis(vue.kpis),
    rangee([1, 1], [semaine(vue.semaine), score(vue.score)]),
    rangee([3, 2], [actifs(vue.actifs), fil(vue.fil)], { meneur: 0 }),
    videos(ctx, vue.videos),
  );
}

function choisirPeriode(p) {
  if (p === etat.periode || !dernier) return;
  etat.periode = p;
  rendu.calme = true;
  rendre(dernier.racine, dernier.ctx);
}

function kpis(cartes) {
  const grille = grilleKpi(...cartes.map((c, i) => carteKpi(c, "accueil.kpi." + i, { grande: true })));
  grille.classList.add("trois");
  return grille;
}

// les panneaux vivent dans la section de l'écran, de part et d'autre de la page ; un changement de business la reconstruit
function lateraux(racine, ctx, vue) {
  const page = racine.parentElement;
  const ecran = page.parentElement;
  ecran.classList.add("accueil");
  for (const el of ecran.querySelectorAll(":scope > .lateral")) el.remove();
  const reseau = () => panneauReseau(ctx, vue.reseau, vue.reseauOuvert, () => basculer("reseauOuvert", "accueil.reseau", gauche, reseau));
  const plan = () => panneauPlan(ctx, vue.plan, vue.taches, vue.planOuvert, () => basculer("planOuvert", "accueil.plan", droite, plan));
  let gauche = reseau();
  let droite = plan();
  page.before(gauche);
  page.after(droite);

  function basculer(champ, cle, el, construire) {
    vue[champ] = !vue[champ];
    ctx.appeler("regler", { cle, valeur: vue[champ] ? null : "0" });
    const neuf = construire();
    el.replaceWith(neuf);
    if (champ === "reseauOuvert") gauche = neuf;
    else droite = neuf;
  }
}

function semaine(s) {
  const b = s.barres;
  return bloc(
    enTetePanneau("Cette semaine", legende(b.legende.map((nom, i) => ({ nom, couleur: LEGENDE[i] })))),
    h(
      "div",
      { class: "mini-chiffres" },
      s.chiffres.map((c) => h("div", { class: "mini-chiffre" }, surtitre(c.libelle), h("span", { class: "valeur" }, chiffre(c.valeur, { format: () => c.texte, cle: "accueil.semaine." + c.libelle })))),
    ),
    h("div", { class: "pousse" }, serie(b)),
  );
}

function serie(b) {
  return barres(b.valeurs, { fantome: b.avant, courante: b.courante, axe: b.axe, aide: (i) => b.aides[i] });
}

function score(sc) {
  return bloc(
    enTetePanneau("Score", h("span", { class: "accessoire-tag" }, "7 j contre 7 j")),
    radar(sc.axes, sc.aide),
    h(
      "div",
      { class: "score-pied" },
      h("span", { class: "score-global" }, chiffre(sc.global, { cle: "accueil.score" })),
      h("span", { class: "score-sur" }, "/ 100"),
      h("div", { class: "espace" }),
      h("span", { class: "score-faible" }, sc.faible),
    ),
  );
}

function actifs(b) {
  return bloc(enTetePanneau(b.titre, legende(b.legende.map((nom, i) => ({ nom, couleur: LEGENDE[i] })))), serie(b));
}

function fil(evenements) {
  return h(
    "div",
    { class: "panneau-fil verre" },
    h("div", { class: "fil-tete" }, point("var(--succes)"), h("span", { class: "h2" }, "En direct"), h("div", { class: "espace" }), h("span", { class: "fil-compte" }, `${evenements.length} récents`)),
    evenements.length ? h("div", { class: "fil" }, evenements.map((e) => ligneFil(e))) : vide("Rien sur la période : le fil se remplit à chaque collecte."),
  );
}

const COLONNES = ["#", "Titre", "Âge", "Vues", "24 h", "Part.", "Enreg.", "Seuil"];

function videos(ctx, v) {
  return h(
    "section",
    { class: "panneau-top verre" },
    h("div", { class: "videos-tete" }, enTetePanneau(v.titre, pastille(v.seuil, { couleur: "var(--secondaire)" }))),
    v.lignes.length
      ? h(
          "div",
          { class: "table-videos" },
          h("div", { class: "rang tete" }, COLONNES.map((c) => h("span", { class: c === "Vues" ? "vues" : null }, c))),
          v.lignes.map((l, i) =>
            h(
              "button",
              { class: "rang", title: l.aide, onclick: () => ctx.montrer("video", l.id) },
              h("span", null, i + 1),
              h("span", { class: "titre" }, l.titre),
              h("span", null, l.age),
              h("span", { class: "vues" }, l.vues),
              h("span", { class: l.gagne ? "gagne" : null }, l.gain),
              h("span", null, l.partages),
              h("span", null, l.enreg),
              h("span", null, l.seuil == null ? "…" : l.seuil ? pastille("oui", { couleur: "var(--accent)" }) : pastille("non", { couleur: "var(--secondaire)" })),
            ),
          ),
        )
      : vide(v.vide),
  );
}

export default {
  titre: "Accueil",
  sousTitre: "",
  rendre,
};
