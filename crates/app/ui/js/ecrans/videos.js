// Vidéos : les comptes suivis et leurs vidéos, la grille ou le tableau, et le détail de la vidéo choisie.
// Tout se lit par la commande `videos` (src/ecrans/videos.rs) ; ce module n'affiche que ce qu'elle rend.

import { actualiser, barres, bloc, chiffre, couleurTon, courbe, enTetePanneau, h, logoPlateforme, miniature, pastille, rangee, rendu, RESEAUX, segmente, sparkline, vide } from "../composants.js";
import * as F from "../format.js";
import { bouton, caseVideo, choixBusiness, menuContextuel, popover, remplacer } from "./videos/commun.js";

const TRIS = [
  { id: "recentes", libelle: "Récentes" },
  { id: "vues", libelle: "Vues" },
  { id: "gain", libelle: "24 h" },
  { id: "engagement", libelle: "Engagement" },
  { id: "conversion", libelle: "Conversion" },
];
const COLONNES = ["#", "Titre", "Âge", "Vues", "24 h", "Part.", "Enreg.", "Seuil"];

// l'état de l'écran ; la vidéo choisie survit à un changement d'écran, comme dans le Swift
const etat = { tri: "recentes", grille: true, plateforme: null, compte: null, choisie: null };
let dernier = null;

const couleur = (cle) => cle && `var(--${cle})`;

function maj(changement) {
  Object.assign(etat, changement);
  rendu.calme = true;
  return ecran.rendre(dernier.racine, { ...dernier.ctx, rafraichi: true });
}

// comptes

function carteCompte(c, ctx, choisi) {
  const carte = h(
    "button",
    { class: "carte-compte verre survolable" + (choisi ? " choisie" : "") + (c.actif ? "" : " en-pause"), title: c.aide, onclick: () => maj({ compte: choisi ? null : c.id }) },
    h(
      "span",
      { class: "haut" },
      miniature([c.avatar], { plateforme: c.plateforme, largeur: "var(--t-avatar-compte)", hauteur: "var(--t-avatar-compte)", rayon: "var(--r-capsule)" }),
      h("span", { class: "nom" }, h("span", { class: "handle" }, c.handle), h("span", { class: "note" }, c.nom)),
      logoPlateforme(c.plateforme),
    ),
    h(
      "span",
      { class: "bas" },
      h("span", { class: "abonnes" }, h("span", { class: "grand" }, chiffre(c.abonnes, { format: F.compact, cle: "videos.abonnes." + c.id })), h("span", { class: "note" }, c.detail)),
      h("span", { class: "tendance" }, c.gain != null && pastille(c.gain, { couleur: couleurTon(c.ton) }), sparkline(c.serie, { ton: c.ton })),
    ),
  );
  return menuContextuel(carte, [
    { libelle: "Ouvrir le profil", faire: () => ctx.lien(c.profil) },
    { libelle: "Relever maintenant", faire: () => ctx.appeler("relancer", { source: c.plateforme }) },
    { libelle: c.actif ? "Mettre en pause" : "Reprendre", faire: () => ctx.appeler("activer_compte", { id: c.id, actif: !c.actif }) },
    null,
    { libelle: "Ne plus suivre", danger: true, faire: () => ctx.appeler("retirer_compte", { id: c.id }) },
  ]);
}

/** Le formulaire de la carte + : plateforme, nom du compte ou lien de profil, business. */
function formulaireCompte(ctx, suivables, fermer) {
  let plateforme = "tiktok";
  let business = ctx.business ?? ctx.businesses[0]?.id;
  let handle = null;
  const note = h("span", { class: "note" }, "Relevé dans la minute.");
  const suivre = bouton("Suivre", () => ajouter());
  suivre.disabled = true;
  const champ = h("input", {
    class: "champ-ajout",
    placeholder: "@compte ou lien du profil",
    oninput: () => lire(),
    onkeydown: (e) => e.key === "Enter" && ajouter(),
  });
  async function lire() {
    const saisie = champ.value;
    const lu = saisie ? await ctx.appeler("handle_compte", { saisie }) : null;
    if (champ.value !== saisie) return;
    handle = lu;
    note.textContent = !saisie || handle ? "Relevé dans la minute." : "Nom de compte illisible.";
    suivre.disabled = !(handle && business);
  }
  async function ajouter() {
    if (!handle || !business) return;
    await ctx.appeler("suivre_compte", { business, plateforme, saisie: champ.value });
    fermer();
  }
  return h(
    "div",
    { class: "formulaire-ajout" },
    h("div", { class: "h2" }, "Suivre un compte"),
    segmente(suivables, plateforme, (id) => (plateforme = id)),
    champ,
    choixBusiness(ctx, business, (id) => (business = id)),
    h("div", { class: "pied" }, note, suivre),
  );
}

function rangeeComptes(d, ctx) {
  const ajout = h(
    "button",
    { class: "carte-ajout verre", onclick: () => popover(ajout, (fermer) => formulaireCompte(ctx, d.suivables, fermer)) },
    h("span", { class: "plus" }, "+"),
    h("span", null, "Suivre un compte"),
  );
  return h("div", { class: "rangee-comptes" }, d.comptes.map((c) => carteCompte(c, ctx, c.id === d.compte)), ajout);
}

// vidéos

function tableau(d) {
  const ligne = (v, i) =>
    h(
      "button",
      { class: "rang" + (v.id === d.choisie ? " choisie" : ""), title: v.aideLigne, onclick: () => maj({ choisie: v.id }) },
      h("span", null, i + 1),
      h("span", { class: "titre" }, v.titre),
      h("span", null, v.age),
      h("span", { class: "vues" }, v.vuesEntier),
      h("span", { class: v.gainPositif ? "gain positif" : "gain" }, v.gain),
      h("span", null, v.partPartages),
      h("span", null, v.partEnreg),
      h("span", null, v.seuil == null ? F.vide : pastille(v.seuil ? "oui" : "non", { couleur: v.seuil ? "var(--accent)" : "var(--secondaire)" })),
    );
  return h(
    "div",
    { class: "table-videos" },
    h("div", { class: "rang tete" }, COLONNES.map((t) => h("span", { class: t === "Vues" ? "vues" : null }, t))),
    h("div", { class: "lignes" }, d.videos.map(ligne)),
  );
}

function grille(d) {
  return h(
    "div",
    { class: "grille-videos" },
    d.videos.map((v) =>
      caseVideo({
        cles: v.cles,
        plateforme: v.plateforme,
        vues: v.vues,
        badge: v.badge,
        badgeCouleur: "var(--texte)",
        legende: v.titre,
        lignes: 2,
        choisie: v.id === d.choisie,
        aide: v.aide,
        clic: () => maj({ choisie: v.id }),
      }),
    ),
  );
}

function panneauVideos(d) {
  return h(
    "section",
    { class: "panneau-videos verre" },
    h("div", { class: "marge" }, enTetePanneau("Toutes les vidéos", segmente(TRIS, etat.tri, (id) => maj({ tri: id })), actualiser(...RESEAUX, "contenu"))),
    h(
      "div",
      { class: "marge filtres" },
      d.plateformes.length > 0 && segmente([{ id: null, libelle: "Toutes" }, ...d.plateformes], d.plateforme, (id) => maj({ plateforme: id, compte: null })),
      h("span", { class: "espace" }),
      pastille(d.seuil, { couleur: "var(--secondaire)" }),
      segmente(
        [
          { id: true, libelle: "Grille" },
          { id: false, libelle: "Tableau" },
        ],
        etat.grille,
        (id) => maj({ grille: id }),
      ),
    ),
    d.videos.length === 0
      ? vide("Aucune vidéo relevée pour ce filtre.")
      : h("div", { class: "defile", "data-defile": "videos" }, etat.grille ? grille(d) : tableau(d)),
  );
}

// détail

const titre = (t) => h("div", { class: "titre-section" }, t);

function lienDetail(texte, faire) {
  return h("button", { class: "lien-detail verre capsule", disabled: !faire, onclick: faire }, texte, h("span", { class: "fleche-lien" }, "↗"));
}

function compteurs(liste, levier) {
  return h(
    "div",
    { class: "compteurs" },
    liste.map((c) => {
      const teinte = c.levier ? levier : c.passe ? "var(--accent)" : c.valeur == null ? "var(--secondaire)" : "var(--texte)";
      return h(
        "div",
        { class: "case-compteur" + (c.levier ? " levier doux" : ""), style: { "--c": c.levier ? levier : null } },
        h("span", { class: "libelle" }, c.libelle),
        h("span", { class: "valeur", style: { color: teinte } }, c.valeur ?? F.vide),
      );
    }),
  );
}

function graphique(t, serie, dessin) {
  return h("div", { class: "section-detail" }, titre(t), h("div", { class: "cadre-graphique" }, serie.absent ? vide(serie.absent) : dessin()));
}

function courbeSimple(serie, nom, format) {
  return courbe({ series: [{ nom, valeurs: serie.valeurs, couleur: "var(--accent)" }], etiquettes: serie.etiquettes, format });
}

function parts(p) {
  return h(
    "div",
    { class: "section-detail parts" },
    titre(p.titre),
    h(
      "div",
      { class: "lignes-parts" },
      p.lignes.map((l) =>
        h(
          "div",
          { class: "ligne-part" + (l ? "" : " absente") },
          h("span", { class: "nom" }, l?.nom),
          h("span", { class: "piste" }, h("i", { style: { width: Math.min(Math.max(l?.part ?? 0, 0), 1) * 100 + "%" } })),
          h("span", { class: "valeur" }, l?.valeur),
        ),
      ),
      p.absent && h("div", { class: "absent" }, vide(p.absent)),
    ),
  );
}

function detail(d, ctx) {
  const v = d.detail;
  if (!v) return bloc(enTetePanneau("Détail", actualiser(...RESEAUX)), vide(d.sansCompte ? "Suivre un compte avec la carte +." : "Aucune vidéo relevée pour ce filtre."));
  const levier = couleurTon(v.percee.ton);
  return bloc(
    enTetePanneau(
      "Détail",
      v.verdict.couleur ? pastille(v.verdict.texte, { couleur: couleur(v.verdict.couleur) }) : h("span", { class: "sans-fiche" }, v.verdict.texte),
      v.seuil && pastille(v.seuil.texte, { couleur: couleur(v.seuil.couleur) }),
      actualiser(v.plateforme),
    ),
    h(
      "div",
      { class: "detail-video" },
      h(
        "div",
        { class: "entete-detail" },
        h(
          "div",
          { class: "ligne-titre" },
          h("span", { class: "titre-video" }, v.titre),
          lienDetail(v.nomPlateforme, v.url && (() => ctx.lien(v.url))),
          lienDetail("Fiche", v.fiche && (() => ctx.appeler("ouvrir_fiche", { video: v.id }))),
        ),
        h("div", { class: "meta" }, v.meta),
        h(
          "div",
          { class: "ligne-vues" },
          h("span", { class: "vues" }, h("span", { class: "grand" }, chiffre(v.vues, { cle: "videos.detail.vues" })), h("span", { class: "note" }, "vues")),
          v.gain && pastille(v.gain.texte, { couleur: couleur(v.gain.couleur) }),
        ),
      ),
      h(
        "div",
        { class: "percee doux", style: { "--c": levier } },
        titre(v.percee.titre),
        h("div", { class: "valeur", style: { color: v.percee.ton === "neutre" ? "var(--texte)" : levier } }, v.percee.valeur),
        h("div", { class: "explication" }, v.percee.explication),
      ),
      h("div", { class: "section-detail" }, titre("Visionnage"), compteurs(v.visionnage, levier), h("div", { class: "note-privee" }, v.note)),
      h("div", { class: "section-detail" }, titre("Engagement"), compteurs(v.engagement, levier)),
      v.retention && graphique("Rétention", v.retention, () => courbeSimple(v.retention, "Encore là", (x) => F.pourcent(x))),
      graphique("Vues relevées", v.releves, () => courbeSimple(v.releves, "Vues", F.entier)),
      v.septJours &&
        graphique("7 premiers jours", v.septJours, () =>
          h(
            "div",
            { class: "sept-jours" },
            barres(v.septJours.valeurs, { axe: v.septJours.etiquettes, aide: (i) => `${v.septJours.etiquettes[i]} : ${F.entier(v.septJours.valeurs[i])} vues`, hauteur: "100%" }),
          ),
        ),
      v.parts.map((rang) => h("div", { class: "rangee-parts" }, rang.map(parts))),
    ),
  );
}

const ecran = {
  titre: "Vidéos",
  sousTitre: "Aucun compte suivi",
  async rendre(racine, ctx) {
    if (!ctx.rafraichi) Object.assign(etat, { tri: "recentes", grille: true, plateforme: null, compte: null });
    // la vidéo choisie dans la palette, sans filtre qui la cacherait
    if (ctx.choix?.genre === "video") Object.assign(etat, { choisie: ctx.choix.id, plateforme: null, compte: null });
    dernier = { racine, ctx: { ...ctx, choix: null } };
    const d = await ctx.appeler("videos", { business: ctx.business, tri: etat.tri, plateforme: etat.plateforme, compte: etat.compte, choisie: etat.choisie });
    Object.assign(etat, { plateforme: d.plateforme, compte: d.compte });
    ctx.page.sousTitre(d.sousTitre);
    remplacer(racine, rangeeComptes(d, ctx), rangee([2, 1], [panneauVideos(d), detail(d, ctx)], { meneur: 1 }));
  },
};

export default ecran;
