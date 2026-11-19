// Les panneaux latéraux de l'accueil : Réseaux à gauche, Plan et « À traiter » à droite. Repliés, chacun devient un rail.
// Le dépli est gardé dans les réglages `accueil.reseau` et `accueil.plan` ("0" replié, rien déplié), comme en Swift.

import { h, pastille, point, surtitre, vide, enTetePanneau, miniature, logoPlateforme, couleurTon, caseRonde } from "../../composants.js";
import { menu, boutonPanneau, icone, lienSecondaire } from "./outils.js";

const LIMITE_TACHES = 5;

/** reseau : la lecture `reseau` ; ouvert ; basculer() replie ou déplie. */
export function panneauReseau(ctx, reseau, ouvert, basculer) {
  const actifs = reseau.comptes.filter((c) => c.actif);
  if (!ouvert) {
    return h(
      "aside",
      { class: "lateral reseau-lateral rail verre" },
      boutonPanneau("barreGauche", "Déplier les réseaux", basculer),
      actifs.map((c) =>
        h(
          "button",
          { class: "rail-compte", title: c.aideRail, onclick: () => ctx.lien(c.profil) },
          avatarCompte(c, true),
          point(c.tenu ? "var(--succes)" : "var(--blanc-repere)", { mini: true }),
        ),
      ),
    );
  }
  const j = reseau.jauge;
  return h(
    "aside",
    { class: "lateral reseau-lateral ouvert verre" },
    h("div", { class: "marge-m" }, enTetePanneau("Réseaux", boutonPanneau("barreGauche", "Replier les réseaux", basculer))),
    j &&
      h(
        "div",
        { class: "jauge-jour marge-m" },
        h("div", { class: "jauge-tete" }, surtitre("Aujourd'hui"), h("span", { class: j.tenue ? "jauge-texte tenue" : "jauge-texte" }, j.texte)),
        h("div", { class: "jauge" }, h("i", { class: j.tenue ? "tenue" : null, style: { width: Math.min(Math.max(j.part, 0), 1) * 100 + "%" } })),
      ),
    reseau.comptes.length ? h("div", { class: "comptes" }, reseau.comptes.map((c) => ligneCompte(ctx, c))) : [vide("Aucun compte suivi."), h("div", { class: "espace" })],
    h("div", { class: "marge-m" }, lienSecondaire("Gérer les comptes", () => ctx.aller("videos"))),
  );
}

function avatarCompte(c, mini) {
  const t = mini ? "var(--t-avatar-mini)" : "var(--t-avatar)";
  return h(
    "span",
    { class: "avatar-compte" },
    miniature([c.avatar], { plateforme: c.plateforme, largeur: t, hauteur: t, rayon: "50%" }),
    h("span", { class: "badge-plateforme" }, logoPlateforme(c.plateforme)),
  );
}

function ligneCompte(ctx, c) {
  const ouvrir = () => ctx.lien(c.profil);
  return h(
    "button",
    {
      class: c.actif ? "ligne-compte" : "ligne-compte en-pause",
      title: c.aide,
      onclick: ouvrir,
      oncontextmenu: (e) => {
        e.preventDefault();
        menu({ x: e.clientX, y: e.clientY }, [
          { libelle: "Ouvrir le profil", faire: ouvrir },
          { libelle: "Relever maintenant", faire: () => ctx.appeler("relancer", { source: c.plateforme }) },
          { libelle: c.actif ? "Mettre en pause" : "Reprendre", faire: () => ctx.appeler("activer_compte", { id: c.id, actif: !c.actif }) },
        ]);
      },
    },
    avatarCompte(c, false),
    h("span", { class: "compte-texte" }, h("span", { class: "handle" }, "@" + c.handle), h("span", { class: c.actif && c.tenu ? "detail tenu" : "detail" }, c.detail)),
    h(
      "span",
      { class: "compte-chiffres" },
      h("span", { class: "abonnes" }, c.abonnes),
      c.gain != null && h("span", { class: "gain", style: { color: couleurTon(c.tonGain) } }, c.gain),
    ),
  );
}

/** plan, taches : les lectures ; ouvert ; basculer() replie ou déplie. */
export function panneauPlan(ctx, plan, taches, ouvert, basculer) {
  if (!ouvert) {
    return h(
      "aside",
      { class: "lateral plan-lateral rail verre" },
      boutonPanneau("barreDroite", "Déplier le plan", basculer),
      plan.restantes > 0 && h("div", { class: "rail-restantes", title: `${plan.restantes} cases à cocher dans Notes` }, icone("liste"), h("span", null, plan.restantes)),
      taches.length > 0 && h("button", { class: "rail-taches", title: "À traiter", onclick: () => ctx.aller("aTraiter") }, pastille(String(taches.length), { couleur: "var(--ambre)" })),
    );
  }
  return h(
    "aside",
    { class: "lateral plan-lateral ouvert" },
    h(
      "section",
      { class: "panneau verre plan" },
      enTetePanneau("Plan", boutonPanneau("barreDroite", "Replier le plan", basculer)),
      h("div", { class: "plan-date" }, plan.date),
      plan.sections.length
        ? h("div", { class: "plan-sections" }, plan.sections.map(section))
        : [vide(plan.vide), h("div", { class: "espace" })],
      h("div", null, lienSecondaire("Ouvrir Notes", () => ctx.appeler("ouvrir_notes"), "↗")),
    ),
    panneauTaches(ctx, taches),
  );
}

function section(s) {
  return h(
    "div",
    { class: "plan-section" },
    h("div", { class: "plan-tete" }, surtitre(s.nom), s.compte != null && h("span", { class: "plan-compte" }, s.compte)),
    s.lignes.map((l) =>
      h(
        "div",
        { class: l.fait ? "plan-ligne fait" : "plan-ligne" },
        h(
          "span",
          { class: "plan-coche", title: l.coche ? "Se coche dans Notes" : null },
          l.coche ? (l.fait ? h("span", { class: "rond fait" }, "✓") : h("span", { class: "rond" })) : h("span", { class: "puce" }),
        ),
        h("span", { class: "plan-texte" }, l.texte),
      ),
    ),
  );
}

function panneauTaches(ctx, taches) {
  return h(
    "section",
    { class: "panneau-taches verre" },
    h("div", { class: "marge-m tete-taches" }, enTetePanneau("À traiter", taches.length > 0 && pastille(String(taches.length), { couleur: "var(--ambre)" }))),
    taches.length === 0
      ? vide("Rien à traiter.")
      : [
          taches.slice(0, LIMITE_TACHES).map((t) =>
            h(
              "div",
              { class: t.faite ? "tache faite" : "tache" },
              caseRonde({ faite: t.faite, alerte: t.alerte, quandClic: () => ctx.appeler("cocher_tache", { id: t.id, fait: !t.faite }) }),
              h("div", { class: "corps" }, h("span", { class: "texte" }, t.texte), t.detail && h("span", { class: "detail" }, t.detail)),
              t.pastille != null && pastille(t.pastille, { couleur: t.pastilleAmbre ? "var(--ambre)" : "var(--secondaire)" }),
              t.lien != null && h("button", { class: "fleche-tache", title: t.lien, onclick: () => ctx.lien(t.lien) }, "↗"),
            ),
          ),
          taches.length > LIMITE_TACHES && h("button", { class: "voir-taches", onclick: () => ctx.aller("aTraiter") }, `Voir les ${taches.length}`),
        ],
  );
}
