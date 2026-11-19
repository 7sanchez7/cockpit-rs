// L'arbre des installs : le chemin des filtres posés, l'entonnoir et le tunnel du nœud ouvert, la découpe par une
// dimension. Une marche, un écran ou une valeur cliqué descend d'un niveau, une valeur écartée garde tous les autres,
// le chemin remonte. Chaque geste relit la commande `arbre` et ne redessine que ce conteneur.

import { h, rangee, bloc, enTetePanneau, pastille, point, vide } from "../../composants.js";
import { etat } from "../app/etat.js";
import { icone, taux } from "../app/outils.js";

// les valeurs montrées avant « + N autres »
const LIGNES = 12;

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
    const entete = enTetePanneau("Entonnoir du nœud", e.fragile && pastille("taux fragiles", { couleur: "var(--ambre)", aide: e.fragile }));
    if (e.vide) return bloc(entete, vide(e.vide));
    return bloc(
      entete,
      h("div", { class: "marches" }, e.pas.map((p) => (p.type === "marche" ? marche(p) : passage(p)))),
      h("div", { class: "espace" }),
      h(
        "div",
        { class: "tuiles" },
        e.tuiles.map((t) => h("div", { class: "tuile", title: t.aide }, h("span", { class: "libelle" }, t.libelle), h("span", { class: "valeur" }, t.valeur))),
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
            m.ecart && pastille(m.ecart.texte, { couleur: `var(--${m.ecart.couleur})`, aide: m.ecart.aide }),
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
      { class: "passage" },
      h("span", { class: "fleche" }, "↓"),
      h("span", null, p.texte),
      h("div", { class: "espace" }),
      p.perte && h("span", { title: "Perdus à ce passage" }, p.perte),
    );
  }

  function tunnel(t) {
    const entete = enTetePanneau("Tunnel d'onboarding", h("span", { class: "accessoire-texte" }, "clique un écran pour garder ceux qui l'ont passé"));
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
    const cellules = (cs) => cs.map((c) => h("span", { class: "cellule", title: c.aide || null }, c.n != null && h("span", { class: c.taux ? "n" : "n seul" }, c.n), c.taux && taux(c.taux)));
    return bloc(
      enTetePanneau(d.titre, h("span", { class: "accessoire-texte" }, d.texte)),
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
