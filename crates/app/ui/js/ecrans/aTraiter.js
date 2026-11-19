// SAV : les fils avec les users et la réponse qui part avec sa notif, les retours à traiter ; dessous, les autres
// tâches, l'ajout à la main et les faites. Tout arrive formaté de la commande `sav` ; les couleurs y sont des noms de
// variables de theme.css. La page est construite une fois : un rafraîchissement remplace les listes, jamais le champ
// de réponse ni celui d'ajout.

import { h, panneau, rangee, segmente, pastille, point, vide, pilule, caseRonde, boutonRelancer } from "../composants.js";

const v = (nom) => `var(--${nom})`;
const LIMITE_RETOURS = 10;
const LIMITE_GROUPE = 8;

// l'état local, gardé d'un rendu à l'autre
const local = { fil: null, traites: false, toutRetours: false, toutGroupes: new Set(), faitesOuvert: false };

const meme = (a, b) => a?.business === b?.business && a?.uid === b?.uid;

function etiquette(e) {
  return e && pastille(e.texte, { couleur: v(e.couleur) });
}

function boutonPrincipal(texte, quandClic) {
  return h("button", { class: "bouton-principal", onclick: quandClic }, texte);
}

/** « Voir les N », « Réduire ». */
function boutonVoir(n, tout, basculer) {
  return h("button", { class: "voir-tout", onclick: basculer }, tout ? "Réduire" : `Voir les ${n}`);
}

// fils

function ligneFil(f, choisi, choisir) {
  return h(
    "button",
    { class: choisi ? "fil-sav choisi" : "fil-sav", onclick: () => choisir(f.cle) },
    h("div", { class: "haut" }, h("span", { class: "nom" }, f.nom), h("span", { class: "quand" }, f.quand)),
    h("div", { class: "bas" }, h("span", { class: "apercu" }, f.apercu), etiquette(f.etat)),
  );
}

// conversation

function bulles(messages) {
  const el = h(
    "div",
    { class: "bulles" },
    messages.map((m) =>
      h(
        "div",
        { class: m.equipe ? "bulle-sav equipe" : "bulle-sav" },
        h("div", { class: "texte" }, m.texte),
        h("div", { class: "quand" }, m.quand),
      ),
    ),
    !messages.length && vide("Le premier message part d'ici, avec une notif sur son téléphone."),
  );
  // le dernier en bas
  requestAnimationFrame(() => (el.scrollTop = el.scrollHeight));
  return el;
}

/** Le champ de réponse et ses boutons ; gardé d'un rafraîchissement à l'autre, remis à zéro quand le fil change. */
function zoneReponse(vue) {
  const champ = h("textarea", {
    class: "reponse-sav",
    placeholder: "Ta réponse, elle arrive dans l'app avec une notif",
    oninput: () => majBoutons(),
    onkeydown: (e) => {
      if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        envoyer();
      }
    },
  });
  const compteur = h("span", { class: "compteur-sav" });
  const resultat = h("span", { class: "resultat-sav" });
  const fermer = pilule("Marquer traité sans répondre", () => agir(() => vue.ctx.appeler("sav_fermer", { cle: vue.d.choisi })));
  const envoi = boutonPrincipal("Envoyer", envoyer);
  envoi.title = vue.ctx.plateforme === "macos" ? "⌘ Entrée" : "Ctrl Entrée";
  let enCours = false;

  function majBoutons() {
    const n = [...champ.value.trim()].length;
    const max = vue.d.longueurReponse;
    compteur.textContent = `${n} / ${max}`;
    compteur.classList.toggle("trop", n > max);
    envoi.disabled = enCours || n === 0 || n > max;
    envoi.textContent = enCours ? "Envoi…" : "Envoyer";
    champ.disabled = enCours;
    fermer.disabled = enCours;
    fermer.hidden = !vue.d.conversation?.ouvert;
  }

  /** Lance une action réseau, son résultat ou son erreur sous le champ. */
  async function agir(action, apres) {
    enCours = true;
    resultat.textContent = "";
    majBoutons();
    try {
      resultat.textContent = await action();
      resultat.classList.remove("erreur");
      apres?.();
    } catch (e) {
      resultat.textContent = e.message;
      resultat.classList.add("erreur");
    }
    resultat.title = resultat.textContent;
    enCours = false;
    majBoutons();
  }

  function envoyer() {
    if (envoi.disabled) return;
    const cle = vue.d.choisi;
    agir(
      () => vue.ctx.appeler("sav_repondre", { texteReponse: champ.value, cle }),
      () => (champ.value = ""),
    );
  }

  return {
    el: h("div", { class: "zone-reponse" }, champ, h("div", { class: "pied-reponse" }, compteur, resultat, h("span", { class: "espace" }), fermer, envoi)),
    majBoutons,
    vider() {
      champ.value = "";
      resultat.textContent = "";
    },
  };
}

// retours

function carteRetour(r, vue) {
  const erreur = h("span", { class: "erreur-sav" });
  const bascule = pilule(r.traite ? "Rouvrir" : "Marquer traité", async () => {
    bascule.disabled = true;
    erreur.textContent = "";
    try {
      await vue.ctx.appeler("sav_traiter", { business: r.business, id: r.id, traite: !r.traite });
    } catch (e) {
      erreur.textContent = erreur.title = e.message;
    }
    bascule.disabled = false;
  });
  return h(
    "div",
    { class: "carte-retour" },
    h(
      "div",
      { class: "haut" },
      pastille(r.source, { couleur: v("secondaire") }),
      etiquette(r.categorie),
      etiquette(r.etoiles),
      h("span", { class: "espace" }),
      h("span", { class: "quand" }, r.quand),
    ),
    r.citation && h("div", { class: "citation" }, `« ${r.citation} »`),
    r.message && h("div", { class: "message" }, r.message),
    h(
      "div",
      { class: "pied" },
      h("span", { class: "detail" }, r.pied),
      h("span", { class: "espace" }),
      erreur,
      r.fil && pilule("Répondre", () => vue.ouvrir(r.fil, true)),
      bascule,
    ),
  );
}

function contenuRetours(d, vue) {
  if (!d.retours.length) return vide(local.traites === false ? "Aucun retour à traiter." : "Aucun retour.");
  const montres = local.toutRetours ? d.retours : d.retours.slice(0, LIMITE_RETOURS);
  return h(
    "div",
    { class: "retours" },
    montres.map((r) => carteRetour(r, vue)),
    d.retours.length > LIMITE_RETOURS &&
      boutonVoir(d.retours.length, local.toutRetours, () => {
        local.toutRetours = !local.toutRetours;
        vue.maj();
      }),
  );
}

// tâches

function ligneTache(t, vue) {
  return h(
    "div",
    { class: t.faite ? "tache faite" : "tache" },
    caseRonde({ faite: t.faite, alerte: t.alerte, quandClic: () => vue.ctx.appeler("sav_cocher", { id: t.id, fait: !t.faite }) }),
    h("div", { class: "corps" }, h("span", { class: "texte" }, t.texte), t.detail && h("span", { class: "detail" }, t.detail)),
    etiquette(t.pastille),
    t.lien && h("button", { class: "fleche", title: t.lien, onclick: () => vue.ctx.lien(t.lien) }, "↗"),
    t.supprimable && h("button", { class: "supprimer", title: "Supprimer", onclick: () => vue.ctx.appeler("sav_supprimer", { id: t.id }) }, "×"),
  );
}

function panneauGroupe(g, vue) {
  const tout = local.toutGroupes.has(g.titre);
  return panneau(
    { titre: g.titre, accessoire: etiquette(g.compte) },
    h(
      "div",
      { class: "taches" },
      (tout ? g.taches : g.taches.slice(0, LIMITE_GROUPE)).map((t) => ligneTache(t, vue)),
      g.taches.length > LIMITE_GROUPE &&
        boutonVoir(g.taches.length, tout, () => {
          if (tout) local.toutGroupes.delete(g.titre);
          else local.toutGroupes.add(g.titre);
          vue.maj();
        }),
    ),
  );
}

/** Aucune autre tâche : quand les sources qui en posent ont lu pour la dernière fois. */
function panneauRien(d) {
  return panneau(
    { titre: "Rien d'autre à traiter." },
    h(
      "div",
      { class: "rien-sav" },
      h("div", { class: "note-sav" }, "Les avis App Store et les verdicts de vidéos arrivent ici à chaque passage de leur source."),
      d.rien.map((s) => h("div", { class: "source-lue" }, point(v(s.couleur)), h("span", { class: "nom" }, s.nom), h("span", { class: "espace" }), h("span", { class: "lu" }, s.lu))),
    ),
  );
}

/** Ajoute une tâche à la main ; gardé d'un rafraîchissement à l'autre. */
function panneauAjout(vue) {
  const champ = h("input", {
    class: "champ",
    placeholder: "Une chose à faire",
    oninput: () => maj(),
    onkeydown: (e) => e.key === "Enter" && ajouter(),
  });
  const date = h("input", { type: "date", class: "date-sav", hidden: true });
  const avec = h("input", {
    type: "checkbox",
    onchange: () => {
      date.hidden = !avec.checked;
      if (avec.checked && !date.value) date.value = vue.d.ajout.jour;
    },
  });
  const pour = h("span", { class: "pour-sav" });
  const bouton = boutonPrincipal("Ajouter", ajouter);

  function maj() {
    bouton.disabled = !champ.value.trim();
    pour.textContent = vue.d.ajout.texte;
  }

  async function ajouter() {
    const texte = champ.value.trim();
    if (!texte) return;
    await vue.ctx.appeler("sav_ajouter", {
      texteTache: texte,
      business: vue.d.ajout.business,
      echeance: avec.checked && date.value ? date.value : null,
    });
    champ.value = "";
    avec.checked = false;
    date.hidden = true;
    maj();
  }

  return {
    el: panneau(
      { titre: "Ajouter" },
      champ,
      h("div", { class: "echeance-sav" }, h("label", null, avec, "Échéance"), date),
      h("div", { class: "pied-ajout" }, pour, h("span", { class: "espace" }), bouton),
    ),
    maj,
  };
}

/** Les tâches cochées ces 7 jours, repliées par défaut. */
function panneauFaites(d, vue) {
  const bouton = h(
    "button",
    {
      class: local.faitesOuvert ? "deplier ouvert" : "deplier",
      title: local.faitesOuvert ? "Replier" : "Déplier",
      onclick: () => {
        local.faitesOuvert = !local.faitesOuvert;
        vue.maj();
      },
    },
    pastille(String(d.faites.length), { couleur: v("secondaire") }),
    h("span", { class: "chevron" }, "⌄"),
  );
  const contenu = local.faitesOuvert && (d.faites.length ? h("div", { class: "taches" }, d.faites.map((t) => ligneTache(t, vue))) : vide("Rien de fait ces 7 jours."));
  return panneau({ titre: "Faites, 7 jours", accessoire: bouton }, contenu);
}

// la page

function outils(d, ctx) {
  return h(
    "div",
    { class: "outils-sav" },
    point(v(d.outils.couleur), { enCours: d.outils.enCours }),
    h("span", { class: "lu", title: d.outils.aide }, d.outils.texte),
    boutonRelancer(d.outils.enCours, () => ctx.appeler("relancer", { source: "firebase" })),
  );
}

async function rendre(racine, ctx) {
  const vue = { ctx, d: null };
  const filsCorps = h("div", { class: "fils-sav" });
  const filsCompte = h("span");
  const convEtat = h("span");
  const convCorps = h("div", { class: "corps-panneau" });
  const reponse = zoneReponse(vue);
  const retoursCorps = h("div", { class: "corps-panneau" });
  const groupes = h("div", { class: "colonne-sav" });
  const ajout = panneauAjout(vue);
  const faites = h("div", { class: "corps-panneau" });
  const segments = segmente(
    [
      { id: false, libelle: "À traiter" },
      { id: true, libelle: "Traités" },
      { id: null, libelle: "Tous" },
    ],
    local.traites,
    (t) => {
      local.traites = t;
      local.toutRetours = false;
      charger();
    },
  );

  /** Ouvre un fil ; depuis un retour, remonte en haut de l'écran. */
  vue.ouvrir = (cle, remonter) => {
    if (!meme(cle, local.fil)) reponse.vider();
    local.fil = cle;
    charger();
    if (remonter) racine.scrollTo({ top: 0, behavior: "smooth" });
  };

  /** Redessine les listes avec la dernière lecture. */
  vue.maj = () => {
    const d = vue.d;
    const conv = d.conversation;
    filsCompte.replaceChildren(pastille(String(d.fils.length), { couleur: v("secondaire") }));
    filsCorps.replaceChildren(
      ...(d.fils.length
        ? d.fils.map((f) => ligneFil(f, meme(f.cle, d.choisi), (cle) => vue.ouvrir(cle, false)))
        : [vide("Aucun fil. Un message écrit depuis l'app arrive ici au passage suivant de Firebase.")]),
    );
    convEtat.replaceChildren(etiquette(conv?.etat) ?? "");
    reponse.el.hidden = !conv;
    convCorps.replaceChildren(
      ...(conv
        ? [h("div", { class: "entete-conv" }, h("div", { class: "h2" }, conv.nom), h("div", { class: "ident" }, conv.ident)), bulles(conv.messages)]
        : [h("div", { class: "conv-vide" }, vide("Choisis un fil ou un retour pour lui répondre."))]),
    );
    reponse.majBoutons();
    retoursCorps.replaceChildren(contenuRetours(d, vue));
    groupes.replaceChildren(...(d.groupes.length ? d.groupes.map((g) => panneauGroupe(g, vue)) : [panneauRien(d)]), ajout.el, panneauFaites(d, vue));
    ajout.maj();
  };

  async function charger() {
    const d = await vue.ctx.appeler("sav", { business: vue.ctx.business, fil: local.fil, traites: local.traites });
    vue.d = d;
    vue.ctx.page.sousTitre(d.sousTitre);
    vue.ctx.page.outils(outils(d, vue.ctx));
    vue.maj();
  }

  await charger();
  const bas = rangee([3, 2], [panneau({ titre: "Retours", accessoire: segments }, retoursCorps), groupes]);
  bas.classList.add("haut");
  racine.replaceChildren(
    rangee(
      [2, 3],
      [
        panneau({ titre: "Fils", accessoire: filsCompte }, filsCorps),
        panneau({ titre: "Conversation", accessoire: convEtat }, convCorps, reponse.el),
      ],
      { meneur: 1 },
    ),
    bas,
  );
  ctx.abonner((c) => {
    vue.ctx = c;
    return charger();
  });
}

export default {
  titre: "SAV",
  sousTitre: "Rien à répondre",
  rendre,
};
