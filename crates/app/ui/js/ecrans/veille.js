// Veille : les vidéos qui montent sur les mots-clés suivis. La page ne défile pas, ses colonnes défilent.
// Tout se lit par la commande `veille` (src/ecrans/veille.rs) ; ce module n'affiche que ce qu'elle rend. Le lecteur
// demande le fichier de la vidéo à `lire_video`, qui le télécharge dans le cache au premier clic.

import { bloc, enTetePanneau, h, icone, logoPlateforme, miniature, pastille, pilule, rangee, rendu, segmente, surtitre, vide } from "../composants.js";
import { bouton, caseVideo, choixBusiness, menuContextuel, popover, remplacer } from "./videos/commun.js";

const TRIS = [
  { id: "ratio", libelle: "× médiane" },
  { id: "vues", libelle: "Vues" },
  { id: "vitesse", libelle: "Vues/h" },
  { id: "recentes", libelle: "Récentes" },
];
const SEUIL = 20000;

const etat = { tri: "ratio", plateforme: null, outliers: true, mot: null, choisie: null };
let dernier = null;

function maj(changement) {
  Object.assign(etat, changement);
  rendu.calme = true;
  return ecran.rendre(dernier.racine, { ...dernier.ctx, rafraichi: true });
}

// mots-clés

function piluleMot(m, ctx) {
  const el = h(
    "button",
    { class: "pilule-mot" + (m.mot === etat.mot ? " choisie" : "") + (m.actif ? "" : " en-pause"), title: m.aide, onclick: () => maj({ mot: etat.mot === m.mot ? null : m.mot }) },
    h("span", null, m.mot),
    h("span", { class: "nombre" }, m.outliers),
  );
  return menuContextuel(el, [
    { libelle: m.actif ? "Mettre en pause" : "Reprendre", faire: () => ctx.appeler("activer_mot", { business: m.business, mot: m.mot, actif: !m.actif }) },
    null,
    {
      libelle: "Retirer",
      danger: true,
      faire: () => {
        if (etat.mot === m.mot) etat.mot = null;
        return ctx.appeler("retirer_mot", { business: m.business, mot: m.mot });
      },
    },
  ]);
}

/** Le formulaire de la pilule + : le mot, le seuil de vues, le business. */
function formulaireMot(ctx, fermer) {
  let business = ctx.business ?? ctx.businesses[0]?.id;
  const ajouterBouton = bouton("Ajouter", () => ajouter());
  const pret = () => champ.value.trim() !== "" && business != null;
  const champ = h("input", {
    class: "champ-ajout",
    placeholder: "apprendre l'espagnol",
    oninput: () => (ajouterBouton.disabled = !pret()),
    onkeydown: (e) => e.key === "Enter" && ajouter(),
  });
  const seuil = h("input", { class: "champ-ajout seuil", type: "number", min: 0, step: 1000, value: SEUIL, placeholder: "Seuil", title: "Vues minimum pour garder une vidéo" });
  ajouterBouton.disabled = true;
  async function ajouter() {
    if (!pret()) return;
    await ctx.appeler("ajouter_mot", { business, mot: champ.value, vuesMin: Math.round(Number(seuil.value) || 0) });
    fermer();
  }
  return h(
    "div",
    { class: "formulaire-ajout" },
    h("div", { class: "h2" }, "Ajouter un mot-clé"),
    h("div", { class: "champs" }, champ, seuil),
    choixBusiness(ctx, business, (id) => (business = id)),
    h("div", { class: "pied" }, h("span", { class: "note" }, "TikTok et Instagram, toutes les 6 h."), ajouterBouton),
  );
}

function rangeeMots(d, ctx) {
  const ajout = h(
    "button",
    { class: "pilule-mot ajout", onclick: () => popover(ajout, (fermer) => formulaireMot(ctx, fermer)) },
    h("span", { class: "plus" }, "+"),
    "Mot-clé",
  );
  return h("div", { class: "rangee-mots" }, d.mots.map((m) => piluleMot(m, ctx)), ajout);
}

// lecteur

/** La vidéo par-dessus l'écran ; Échap ou un clic sur le voile la ferme. */
async function lire(v, ctx) {
  const cadre = h("div", { class: "cadre" }, h("span", { class: "passage tourne" }, icone("relancer"), "Téléchargement"));
  const voile = h("div", { class: "voile lecteur", onmousedown: (e) => e.target === voile && fermer() }, cadre);
  const echap = (e) => {
    if (e.key !== "Escape") return;
    e.stopPropagation();
    fermer();
  };
  function fermer() {
    voile.remove();
    document.removeEventListener("keydown", echap, true);
  }
  document.addEventListener("keydown", echap, true);
  document.body.append(voile);
  let chemin;
  try {
    chemin = await ctx.appeler("lire_video", { id: v.id });
  } catch {
    // l'erreur est déjà en toast
    fermer();
    return;
  }
  if (voile.isConnected) cadre.replaceChildren(h("video", { src: ctx.fichier(chemin), controls: true, autoplay: true }));
}

// vidéos

function panneauVeille(d, ctx) {
  return h(
    "section",
    { class: "panneau-videos verre" },
    h("div", { class: "marge" }, enTetePanneau("Ce qui a marché", segmente(TRIS, etat.tri, (id) => maj({ tri: id })))),
    h(
      "div",
      { class: "marge filtres" },
      segmente([{ id: null, libelle: "Toutes" }, ...d.plateformes], d.plateforme, (id) => maj({ plateforme: id })),
      h("span", { class: "espace" }),
      segmente(
        [
          { id: true, libelle: "Outliers" },
          { id: false, libelle: "Tout" },
        ],
        etat.outliers,
        (id) => maj({ outliers: id }),
      ),
    ),
    d.videos.length === 0
      ? vide(d.sansMot ? "Ajouter un mot-clé avec la pilule +." : "Aucune vidéo pour ce filtre.")
      : h(
          "div",
          { class: "defile", "data-defile": "veille" },
          h(
            "div",
            { class: "grille-videos" },
            d.videos.map((v) => {
              const el = caseVideo({
                cles: v.cles,
                plateforme: v.plateforme,
                vues: v.vues,
                badge: v.badge,
                badgeCouleur: v.outlier ? "var(--succes)" : "var(--texte)",
                legende: v.legende,
                lignes: 1,
                choisie: v.id === d.choisie,
                stats: v.stats,
                clic: () => maj({ choisie: v.id }),
              });
              el.addEventListener("dblclick", () => lire(v, ctx));
              return menuContextuel(el, [
                { libelle: "Lire ici", faire: () => lire(v, ctx) },
                { libelle: `Ouvrir sur ${v.nomPlateforme}`, faire: () => ctx.lien(v.url) },
              ]);
            }),
          ),
        ),
  );
}

// détail

function detail(d, ctx) {
  const v = d.detail;
  if (!v) return bloc(enTetePanneau("Détail"), vide(d.sansMot ? "Ajouter un mot-clé avec la carte +." : "Aucune vidéo pour ce filtre."));
  return bloc(
    enTetePanneau("Détail", h("button", { class: "voir-sur", onclick: () => ctx.lien(v.url) }, `Voir sur ${v.nomPlateforme} ↗`)),
    h(
      "div",
      { class: "defile detail-veille", "data-defile": "detail" },
      h(
        "div",
        { class: "entete-veille" },
        h(
          "button",
          { class: "apercu-lecture survolable", title: "Lire ici", onclick: () => lire(v, ctx) },
          miniature(v.cles, { plateforme: v.plateforme, largeur: "var(--t-apercu-l)", hauteur: "var(--t-apercu-h)", rayon: "var(--r-avatar)" }),
          h("span", { class: "lecture" }, "▶"),
        ),
        h(
          "div",
          { class: "infos" },
          h("div", { class: "compte" }, logoPlateforme(v.plateforme), v.compte),
          h("div", { class: "note" }, v.meta),
          v.legende && h("div", { class: "note legende-veille" }, v.legende),
        ),
      ),
      h(
        "div",
        { class: "mesures" },
        v.mesures.map((m) => [h("span", { class: "nom" }, m.nom), h("span", { class: "valeur" + (m.succes ? " succes" : "") }, m.valeur ?? "…")]),
      ),
      v.analyse
        ? h(
            "div",
            { class: "analyse" },
            h("div", { class: "pastilles" }, v.analyse.pastilles.map((p) => pastille(p.texte, { couleur: p.couleur && `var(--${p.couleur})` }))),
            v.analyse.sections.map((s) => h("div", { class: "section-analyse" }, surtitre(s.titre), h("div", { class: s.fort ? "texte fort" : "texte" }, s.texte))),
          )
        : h("div", { class: "attente" }, v.attente),
    ),
  );
}

function outils(ctx) {
  if (ctx.enCours.includes("veille")) return h("span", { class: "passage tourne" }, icone("relancer"), "Passage en cours");
  return pilule("Lancer maintenant", () => ctx.appeler("relancer", { source: "veille" }), { icone: "relancer" });
}

const ecran = {
  titre: "Veille",
  sousTitre: "Aucun mot-clé",
  defile: false,
  async rendre(racine, ctx) {
    if (!ctx.rafraichi) Object.assign(etat, { tri: "ratio", plateforme: null, outliers: true, mot: null, choisie: null });
    dernier = { racine, ctx };
    const d = await ctx.appeler("veille", { business: ctx.business, tri: etat.tri, plateforme: etat.plateforme, outliers: etat.outliers, mot: etat.mot, choisie: etat.choisie });
    ctx.page.sousTitre(d.sousTitre);
    ctx.page.outils(outils(ctx));
    const colonnes = rangee([2, 1], [panneauVeille(d, ctx), detail(d, ctx)]);
    colonnes.classList.add("remplit");
    remplacer(racine, rangeeMots(d, ctx), colonnes);
  },
};

export default ecran;
