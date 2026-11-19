// Apps de potes : nos apps et celles des potes dans le top 200 de l'App Store, pays par pays ; la courbe sur 30 jours
// et tous les classements de l'app choisie ; l'ajout et l'arrêt du suivi. Tout arrive formaté de la commande `apps` ;
// le champ d'ajout est gardé d'un rafraîchissement à l'autre.

import { h, s, panneau, rangee, segmente, pastille, vide } from "../composants.js";

const v = (nom) => `var(--${nom})`;

// l'état local, gardé d'un rendu à l'autre
const local = { liste: "gratuit", toutes: false, app: null, pays: null };

/** L'icône d'une app, mise en cache par Classements ; un fond neutre tant qu'elle n'est pas téléchargée. */
function iconeApp(id, ctx) {
  return h("span", { class: "icone-app" }, h("img", { src: ctx.image("icone:" + id), alt: "", onerror: (e) => e.currentTarget.remove() }));
}

function cellule(c) {
  if (!c.classee) return h("div", { class: "rang-app absent" }, c.texte);
  return h("div", { class: "rang-app" }, h("span", { class: "place" }, c.texte), c.gain && h("span", { class: "gain", style: { color: v(c.gain.couleur) } }, c.gain.texte));
}

// top 200

function table(d, ctx, choisir) {
  return h(
    "div",
    { class: "table-apps", style: { "--pays": d.pays.length } },
    h("div", { class: "rang-tete" }, h("span", { class: "nom-colonne" }, "App"), d.pays.map((p) => h("span", null, p.nom))),
    d.lignes.map((l, i) =>
      h(
        "button",
        { class: ["ligne-app", l.id === d.choisie && "choisie", i % 2 === 0 && "paire"].filter(Boolean).join(" "), onclick: () => choisir(l.id) },
        iconeApp(l.id, ctx),
        h(
          "div",
          { class: "nom-app" },
          h("div", { class: "haut" }, h("span", { class: "nom" }, l.nom), l.aNous && pastille("à nous", { couleur: v("accent") })),
          h("div", { class: "infos" }, l.infos),
        ),
        l.rangs.map(cellule),
      ),
    ),
  );
}

/** Le champ d'ajout : un id ou un lien App Store, l'app est cherchée puis suivie. Le résultat reste affiché. */
function ajoutApp(vue) {
  const champ = h("input", {
    placeholder: "Id ou lien App Store d'une app de pote",
    oninput: () => maj(),
    onkeydown: (e) => e.key === "Enter" && ajouter(),
  });
  const libelle = h("span", null, "Suivre");
  const bouton = h("button", { class: "suivre-app verre capsule", onclick: () => ajouter() }, h("span", { class: "plus" }, "+"), libelle);
  const retour = h("div", { class: "retour-app" });
  let enCours = false;

  function maj() {
    bouton.disabled = !champ.value || enCours;
    libelle.textContent = enCours ? "Recherche…" : "Suivre";
  }

  async function ajouter() {
    if (enCours || !champ.value) return;
    enCours = true;
    retour.hidden = true;
    maj();
    try {
      const r = await vue.ctx.appeler("apps_suivre", { saisie: champ.value });
      retour.textContent = r.texte;
      retour.classList.toggle("erreur", r.erreur);
      retour.hidden = false;
      if (r.id) {
        champ.value = "";
        local.app = r.id;
      }
    } finally {
      enCours = false;
      maj();
    }
  }

  maj();
  retour.hidden = true;
  return h("div", { class: "ajout-app" }, h("div", { class: "rangee-ajout" }, h("label", { class: "champ-app verre capsule" }, champ), bouton), retour);
}

// courbe

/** La meilleure place du jour sur 30 jours, 1er en haut ; un jour hors top coupe la courbe. */
function courbeRang(c) {
  if (!c.points.length) return h("div", { class: "courbe-vide" }, vide("Hors top 200 ces 30 derniers jours."));
  const pc = (f) => f * 100 + "%";
  const troncons = Object.values(Object.groupBy(c.points, (p) => p.troncon));
  return h(
    "div",
    { class: "courbe-rang" },
    h("div", { class: "axe-y" }, c.reperes.map((r) => h("span", { style: { top: pc(r.position) } }, r.texte))),
    h(
      "div",
      { class: "trace" },
      c.reperes.map((r) => h("i", { class: "repere", style: { top: pc(r.position) } })),
      h(
        "div",
        { class: "dessin" },
        s(
          "svg",
          { viewBox: "0 0 100 100", preserveAspectRatio: "none" },
          troncons.map((t) => s("polyline", { points: t.map((p) => `${p.x * 100},${p.y * 100}`).join(" ") })),
        ),
        c.points.map((p) => h("i", { class: "point-rang", title: `${p.jour} : ${p.rang}`, style: { left: pc(p.x), top: pc(p.y) } })),
      ),
    ),
    h("div", { class: "axe-x" }, c.axe.map((r) => h("span", { style: { left: pc(r.position) } }, r.texte))),
  );
}

function panneauCourbe(d, recharger) {
  const c = d.courbe;
  const choix = segmente(
    d.pays.map((p) => ({ id: p.id, libelle: p.id.toUpperCase() })),
    c.pays,
    (p) => {
      local.pays = p;
      recharger();
    },
  );
  return panneau({ titre: "Rang sur 30 jours", accessoire: choix }, h("div", { class: "legende-courbe" }, c.legende), courbeRang(c));
}

function panneauDetail(d, vue) {
  const t = d.detail;
  return panneau(
    { titre: "Tous les classements", accessoire: h("span", { class: "genre-app" }, t.genre), classe: "detail-app" },
    h(
      "div",
      { class: "grille-classements" },
      h("span"),
      t.listes.map((l) => h("span", { class: "tete" }, l)),
      t.lignes.map((l) => [h("span", { class: "pays" }, l.pays), l.rangs.map(cellule)]),
    ),
    t.retirable &&
      h(
        "button",
        { class: "retirer-app verre capsule", title: t.retirable, onclick: () => vue.ctx.appeler("apps_ne_plus_suivre", { id: d.choisie }) },
        "Ne plus suivre",
      ),
  );
}

// la page

/** L'app choisie dans la palette devient celle de la courbe. */
function prendreChoix(ctx) {
  if (ctx.choix?.genre === "app") local.app = ctx.choix.id;
}

async function rendre(racine, ctx) {
  const vue = { ctx };
  prendreChoix(ctx);
  const ajout = ajoutApp(vue);

  async function dessiner() {
    const d = await vue.ctx.appeler("apps", { business: vue.ctx.business, liste: local.liste, toutes: local.toutes, app: local.app, pays: local.pays });
    vue.ctx.page.sousTitre(d.sousTitre);
    const choisir = (id) => {
      local.app = id;
      dessiner();
    };
    const choix = h(
      "div",
      { class: "choix-apps" },
      segmente(
        d.listes.map((l) => ({ id: l.id, libelle: l.nom })),
        local.liste,
        (l) => {
          local.liste = l;
          dessiner();
        },
      ),
      segmente(
        [
          { id: false, libelle: "Sa catégorie" },
          { id: true, libelle: "Toutes" },
        ],
        local.toutes,
        (t) => {
          local.toutes = t;
          dessiner();
        },
      ),
    );
    racine.replaceChildren(
      panneau({ titre: "Top 200", accessoire: choix }, d.lu ? table(d, vue.ctx, choisir) : vide("La source Classements n'a encore rien lu."), ajout),
      d.courbe && rangee([2, 1], [panneauCourbe(d, dessiner), panneauDetail(d, vue)]),
    );
  }

  await dessiner();
  ctx.abonner((c) => {
    vue.ctx = c;
    prendreChoix(c);
    return dessiner();
  });
}

export default {
  titre: "Apps de potes",
  sousTitre: "Aucune app suivie",
  rendre,
};
