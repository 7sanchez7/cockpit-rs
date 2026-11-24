// L'arbre des installs : le chemin des filtres posés, l'entonnoir et le tunnel du nœud ouvert, la découpe par une
// dimension. Une marche, un écran ou une valeur cliqué descend d'un niveau, une valeur écartée garde tous les autres,
// le chemin remonte. Chaque geste relit la commande `arbre` et ne redessine que ce conteneur.

import { h, s, rangee, bloc, enTetePanneau, pastille, point, vide, actualiser } from "../../composants.js";
import { etat } from "../app/etat.js";
import { icone, taux } from "../app/outils.js";

// l'anneau du premier rond, dans un carré de 120 : centre, rayon, épaisseur, le jour entre deux arcs (en part du tour)
const ANNEAU = { c: 60, r: 50, epaisseur: 12, jour: 0.006 };

// les valeurs montrées avant « + N autres »
const LIGNES = 12;
const SOURCES_ARBRE = ["posthog", "revenuecat"];

/** a : la lecture `arbre` de la commande data (debut, fin, racine). Rend le conteneur une fois l'arbre lu. */
export async function arbre(ctx, a) {
  const boite = h("div", { class: "arbre" });
  let tout = false;

  async function relire() {
    const v = await ctx.appeler("arbre", {
      business: ctx.business,
      debut: a.debut,
      fin: a.fin,
      racine: a.racine,
      chemin: etat.chemin,
      decoupe: etat.decoupe,
      tri: etat.tri,
    });
    boite.replaceChildren(filAriane(v.miettes), rangee([1, 1], [entonnoir(v.entonnoir), tunnel(v.tunnel)]), decoupe(v));
  }

  function aller(chemin) {
    etat.chemin = chemin;
    relire();
  }

  const meme = (x, y) => JSON.stringify(x) === JSON.stringify(y);
  const dimension = (f) => (f.valeur ?? f.sauf)?.[0];

  // pose le filtre, ou le retire s'il est déjà posé
  function basculer(filtre) {
    const i = etat.chemin.findIndex((f) => meme(f, filtre));
    aller(i < 0 ? [...etat.chemin, filtre] : etat.chemin.filter((_, k) => k !== i));
  }

  // garde ceux qui ont passé l'écran : un seul écran à la fois, le recliquer le retire
  function passer(rang) {
    const avant = etat.chemin.find((f) => f.ecran != null)?.ecran;
    aller([...etat.chemin.filter((f) => f.ecran == null), ...(avant === rang ? [] : [{ ecran: rang }])]);
  }

  // descend dans la valeur et découpe par la dimension suivante pas encore posée ; une valeur posée se retire
  function ouvrir(valeur, dimensions) {
    const filtre = { valeur: [etat.decoupe, valeur] };
    const posee = etat.chemin.some((f) => meme(f, filtre));
    if (!posee) {
      const i = dimensions.indexOf(etat.decoupe);
      const suite = [...dimensions.slice(i + 1), ...dimensions];
      const chemin = [...etat.chemin, filtre];
      etat.decoupe = suite.find((d) => !chemin.some((f) => dimension(f) === d)) ?? etat.decoupe;
      tout = false;
    }
    basculer(filtre);
  }

  function filAriane(miettes) {
    const fin = miettes.length - 1;
    return h(
      "div",
      { class: "fil-ariane verre capsule" },
      h(
        "div",
        { class: "miettes" },
        miettes.map((m, k) => [
          k > 0 && h("span", { class: "chevron-miette" }, "›"),
          h(
            "span",
            { class: k === fin ? "miette courante" : "miette" },
            h("button", { title: m.aide, onclick: () => aller(etat.chemin.slice(0, k)) }, h("span", { class: "libelle" }, m.libelle), h("span", { class: "installs" }, m.installs)),
            k > 0 && h("button", { class: "retirer", title: "Retirer ce seul filtre", onclick: () => aller(etat.chemin.filter((_, i) => i !== k - 1)) }, "✕"),
          ),
        ]),
      ),
      fin > 0 && h("button", { class: "tout-effacer", onclick: () => aller([]) }, "Tout effacer"),
    );
  }

  function entonnoir(e) {
    const entete = enTetePanneau("Entonnoir du nœud", e.fragile && pastille("taux fragiles", { couleur: "var(--ambre)", aide: e.fragile }), actualiser(...SOURCES_ARBRE));
    if (e.vide) return bloc(entete, vide(e.vide));
    return bloc(
      entete,
      h("div", { class: "marches" }, e.pas.map((p) => (p.type === "marche" ? marche(p) : passage(p)))),
      h("div", { class: "espace" }),
      e.premierRond && premierRond(e.premierRond),
      h(
        "div",
        { class: "tuiles" },
        e.tuiles.map((t) =>
          h(
            "div",
            { class: "tuile", title: t.aide },
            h("span", { class: "libelle" }, t.libelle),
            h("div", { class: "tuile-valeur" }, h("span", { class: "valeur" }, t.valeur), ecart(t.ecart)),
          ),
        ),
      ),
    );
  }

  function premierRond(p) {
    return h(
      "div",
      { class: "premier-rond" },
      h(
        "div",
        { class: "anneau", title: p.aide },
        anneau(p.parts),
        h("div", { class: "anneau-centre" }, h("span", { class: "valeur" }, p.centre), h("span", { class: "libelle" }, "commencent"), ecart(p.ecart)),
      ),
      h(
        "div",
        { class: "parts-rond" },
        p.parts.map((q) =>
          h(
            "div",
            { class: "part-rond", title: q.aide },
            point(`var(--${q.couleur})`),
            h("div", { class: "part-noms" }, h("span", { class: "nom" }, q.libelle), q.sous && h("span", { class: "sous" }, q.sous)),
            h("span", { class: "valeur" }, q.valeur),
            ecart(q.ecart) || h("span"),
          ),
        ),
      ),
    );
  }

  function marche(m) {
    const couleur = `var(--${m.couleur})`;
    const corps = [
      h(
        "div",
        { class: "marche-haut" },
        point(couleur),
        h("div", { class: "marche-noms" }, h("span", { class: "nom" }, m.nom), h("div", { class: "marche-nombre" }, h("span", { class: "nombre" }, m.nombre), m.sous && h("span", { class: "sous" }, m.sous))),
        h("div", { class: "espace" }),
        m.taux &&
          h(
            "div",
            { class: "marche-taux" },
            h("span", { class: "taux-marche", style: { color: couleur } }, m.taux),
            ecart(m.ecart),
          ),
        m.filtre && h("span", { class: m.posee ? "coche-marche posee" : "coche-marche", style: { "--c": couleur } }, m.posee ? "✓" : "+"),
      ),
      h("div", { class: "barre-marche" }, h("i", { style: { "--part": m.part, background: couleur } })),
    ];
    if (!m.filtre) return h("div", { class: "marche" }, corps);
    return h("button", { class: m.posee ? "marche posee" : "marche", title: m.aide, onclick: () => basculer(m.filtre) }, corps);
  }

  function passage(p) {
    return h(
      "div",
      { class: "passage-arbre" },
      h("span", { class: "fleche" }, "↓"),
      h("span", null, p.texte),
      h("div", { class: "espace" }),
      p.perte && h("span", { title: "Perdus à ce passage" }, p.perte),
    );
  }

  function tunnel(t) {
    const entete = enTetePanneau("Tunnel d'onboarding", h("span", { class: "accessoire-texte" }, "clique un écran pour garder ceux qui l'ont passé"), actualiser(...SOURCES_ARBRE));
    if (!t.etapes.length) return bloc(entete, vide("Aucun install sur la période."));
    return bloc(
      entete,
      h(
        "div",
        { class: "etapes" },
        t.etapes.map((e, i) => {
          const corps = [
            h("span", { class: "nom-etape" }, e.nom),
            h("div", { class: "barre-etape" }, h("i", { class: e.pire ? "pire" : null, style: { width: e.part * 100 + "%" } })),
            h("span", { class: "nombre" }, e.nombre),
            h("span", { class: "pourcent" }, e.pourcent),
            h("span", { class: e.pire ? "perte pire" : "perte" }, e.perte),
          ];
          const classe = e.choisie ? "etape choisie" : "etape";
          return i === 0 ? h("div", { class: classe }, corps) : h("button", { class: classe, title: e.aide, onclick: () => passer(i) }, corps);
        }),
      ),
    );
  }

  function decoupe(v) {
    const d = v.decoupe;
    const montrees = tout ? d.lignes : d.lignes.slice(0, LIGNES);
    const cellules = (cs) => cs.map((c) => h("span", { class: "cellule-arbre", title: c.aide || null }, c.n != null && h("span", { class: c.taux ? "n" : "n seul" }, c.n), c.taux && taux(c.taux)));
    return bloc(
      enTetePanneau(d.titre, h("span", { class: "accessoire-texte" }, d.texte), actualiser(...SOURCES_ARBRE)),
      h(
        "div",
        { class: "dimensions" },
        d.onglets.map((o) =>
          h(
            "button",
            {
              class: o.actif ? "dimension actif" : "dimension",
              title: o.aide,
              onclick: () => {
                etat.decoupe = o.id;
                tout = false;
                relire();
              },
            },
            h("span", { class: "nom" }, o.nom),
            o.posees && h("span", { class: "posees" }, o.posees),
          ),
        ),
      ),
      h(
        "div",
        { class: "table-decoupe", style: { "--colonnes-decoupe": ["minmax(0, 1fr)", ...d.colonnes.map((c) => `var(--t-arbre-${c.largeur})`)].join(" ") } },
        h(
          "div",
          { class: "rang-decoupe tete" },
          h("span", { class: "nom-decoupe" }, d.nom),
          d.colonnes.map((c) =>
            h(
              "button",
              {
                class: c.actif ? "actif" : null,
                title: c.aide,
                onclick: () => {
                  etat.tri = c.actif ? null : c.titre;
                  relire();
                },
              },
              c.actif && h("span", { class: "chevron-tri" }, "⌄"),
              c.titre,
            ),
          ),
        ),
        !d.lignes.length && vide("Aucune valeur sur ce nœud."),
        montrees.map((l, i) =>
          h(
            "div",
            { class: ["ligne-decoupe", l.posee ? "posee" : i % 2 === 0 && "paire"].filter(Boolean).join(" ") },
            h(
              "button",
              { class: "rang-decoupe", title: l.aide, onclick: () => ouvrir(l.valeur, v.dimensions) },
              h(
                "span",
                { class: "valeur-decoupe", style: { "--part": l.part } },
                h("span", { class: l.pasMesure ? "libelle pas-mesure" : "libelle" }, l.libelle),
                h("span", { class: "pourcent" }, l.pourcent),
              ),
              cellules(l.cellules),
            ),
            h("button", { class: "ecarter", title: l.aideEcarter, "aria-label": l.aideEcarter, disabled: l.posee, onclick: () => basculer({ sauf: [etat.decoupe, l.valeur] }) }, icone("oeilBarre")),
          ),
        ),
        d.lignes.length > LIGNES &&
          h(
            "button",
            {
              class: "plus-valeurs",
              onclick: () => {
                tout = !tout;
                relire();
              },
            },
            tout ? "Moins de valeurs" : `+ ${d.lignes.length - LIGNES} autres valeurs`,
          ),
        d.total && h("div", { class: "rang-decoupe total" }, h("span", { class: "nom-decoupe" }, "Tout le nœud"), cellules(d.total)),
      ),
    );
  }

  await relire();
  return boite;
}

function ecart(e) {
  return e && pastille(e.texte, { couleur: `var(--${e.couleur})`, aide: e.aide });
}

// un arc par part, dans le sens des aiguilles depuis le haut ; une part seule fait le tour entier
function anneau(parts) {
  const { c, r, epaisseur, jour } = ANNEAU;
  const sur = (f) => {
    const t = 2 * Math.PI * f - Math.PI / 2;
    return `${(c + r * Math.cos(t)).toFixed(2)} ${(c + r * Math.sin(t)).toFixed(2)}`;
  };
  const visibles = parts.filter((q) => q.part > 0);
  let debut = 0;
  const arcs = visibles.map((q) => {
    const trait = { fill: "none", "stroke-width": epaisseur, style: { stroke: `var(--${q.couleur})` } };
    const titre = s("title", {}, `${q.libelle} : ${q.valeur}`);
    if (visibles.length === 1) return s("circle", { cx: c, cy: c, r, ...trait }, titre);
    const [de, a] = [debut + jour, debut + q.part - jour];
    debut += q.part;
    return a > de && s("path", { d: `M ${sur(de)} A ${r} ${r} 0 ${a - de > 0.5 ? 1 : 0} 1 ${sur(a)}`, ...trait }, titre);
  });
  return s("svg", { viewBox: "0 0 120 120", "aria-hidden": "true" }, arcs);
}
