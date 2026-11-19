// La carte d'une vidéo de la Veille au survol de sa miniature, et sa fiche au clic : la vidéo à gauche, les
// onglets Détails, Analyse et Légende à droite. La carte se lit dans la case (`carte` de `veille`), la fiche par
// `fiche_veille` ; la vidéo se télécharge par `lire_video` à l'ouverture.

import { h, icone, jeton, logoPlateforme, miniature, nombreJeton, pastille, segmente, sobre, surtitre } from "../../composants.js";

// carte au survol

// Un seul écouteur pour toute la page : une miniature inscrite par `survolCarte` porte `data-carte`, et la case sous
// la souris se relit à chaque mouvement, défilement ou nouveau rendu ; la carte ne capte pas la souris.
const videos = new Map();
let carte = null;
let minuteur = null;
let souris = null;

function fermerCarte() {
  clearTimeout(minuteur);
  carte?.el.remove();
  carte = null;
}

/** Inscrit la miniature `el` de `v` : la survoler montre sa carte. */
export function survolCarte(el, v) {
  el.dataset.carte = v.id;
  videos.set(v.id, v);
}

/** Après un nouveau rendu : la carte se refait sur la case encore sous la souris, ou se ferme. */
export function recalerCarte() {
  if (!carte) return;
  const el = caseSousLaSouris();
  if (el) montrerCarte(el, { anime: false });
  else fermerCarte();
}

const caseSous = (cible) => cible?.closest?.("[data-carte]") ?? null;
const caseSousLaSouris = () => souris && caseSous(document.elementFromPoint(souris.x, souris.y));

function suivre(el) {
  clearTimeout(minuteur);
  if (el && carte?.ancre === el) return;
  if (el) {
    // d'une case à sa voisine, la carte suit sans attendre
    if (carte) montrerCarte(el);
    else minuteur = setTimeout(() => (el = caseSousLaSouris()) && montrerCarte(el), nombreJeton("--delai-survol"));
  } else if (carte) {
    minuteur = setTimeout(fermerCarte, nombreJeton("--delai-fermeture"));
  }
}

document.addEventListener("pointermove", (e) => (souris = { x: e.clientX, y: e.clientY }), { passive: true, capture: true });
document.addEventListener("mouseover", (e) => suivre(caseSous(e.target)), true);
document.addEventListener("mouseout", (e) => !e.relatedTarget && fermerCarte(), true);
document.addEventListener("mousedown", fermerCarte, true);
document.addEventListener(
  "scroll",
  () => {
    if (!carte) return;
    const el = caseSousLaSouris();
    if (el?.dataset.carte === carte.id) placer(el);
    else fermerCarte();
  },
  true,
);
addEventListener("blur", fermerCarte);

function montrerCarte(el, { anime = true } = {}) {
  const v = videos.get(el.dataset.carte);
  fermerCarte();
  if (!v || !el.isConnected) return;
  const c = v.carte;
  const node = h(
    "div",
    { class: "carte-survol verre" },
    h("div", { class: "tete" }, logoPlateforme(v.plateforme), h("div", { class: "qui" }, h("div", { class: "compte" }, c.compte), h("div", { class: "meta" }, c.meta))),
    viralite(c.viralite),
    h(
      "div",
      { class: "chiffres" },
      c.chiffres.map((x) => h("div", { class: "chiffre" }, surtitre(x.nom), h("div", { class: "valeur" }, x.valeur))),
    ),
    h("div", { class: "filet" }),
    h("div", { class: "section-carte" }, surtitre("Anatomie du script"), c.anatomie.length ? anatomie(c.anatomie) : h("div", { class: "attente" }, c.attente)),
    h("div", { class: "indice" }, "Clic : la vidéo, l'analyse et les similaires"),
  );
  carte = { el: node, id: v.id, ancre: el };
  document.body.append(node);
  placer(el);
  if (anime && !sobre()) {
    node.animate([{ opacity: 0, transform: `scale(${jeton("--appui")})` }, { opacity: 1, transform: "none" }], {
      duration: nombreJeton("--duree-survol"),
      easing: jeton("--ressort-survol"),
    });
  }
}

/** À droite de la case, à gauche si la place manque ; gardée dans la fenêtre. */
function placer(el) {
  carte.ancre = el;
  const r = el.getBoundingClientRect();
  const marge = nombreJeton("--e-m");
  const { offsetWidth: l, offsetHeight: hauteur } = carte.el;
  const gauche = r.right + marge + l <= innerWidth - marge ? r.right + marge : r.left - marge - l;
  carte.el.style.left = Math.max(marge, gauche) + "px";
  carte.el.style.top = Math.max(marge, Math.min(r.top, innerHeight - hauteur - marge)) + "px";
}

/** Le coefficient de viralité en grand, coloré par son verdict. */
function viralite(x) {
  return h(
    "div",
    { class: "viralite", style: { "--c": `var(--${x.couleur})` } },
    surtitre("Viralité"),
    h("div", { class: "coef" }, h("span", { class: "valeur" }, x.valeur), h("span", { class: "verdict" }, x.verdict)),
    h("div", { class: "detail" }, x.detail),
  );
}

/** La barre segmentée des blocs du script et sa légende ; `detail` ajoute la phrase de chaque bloc. */
function anatomie(segments, { detail = false } = {}) {
  const part = (s) => (s.part != null ? `${s.part} %` : null);
  return h(
    "div",
    { class: "anatomie" + (detail ? " detaillee" : "") },
    h(
      "div",
      { class: "barre-anatomie" },
      segments.map((s) => h("span", { style: { "--c": `var(--${s.couleur})`, "flex-grow": s.part ?? 1 }, title: `${s.nom} : ${s.texte}` })),
    ),
    h(
      "div",
      { class: "legende-anatomie" },
      segments.map((s) =>
        h(
          "div",
          { class: "bloc-anatomie", style: { "--c": `var(--${s.couleur})` } },
          h("span", { class: "point" }),
          h("span", { class: "nom" }, s.nom),
          part(s) && h("span", { class: "part" }, part(s)),
          detail && h("div", { class: "texte" }, s.texte),
        ),
      ),
    ),
  );
}

// fiche au clic

let fiche = null;

/** Ouvre la fiche de la vidéo `id`, ou la remplace si une fiche est déjà ouverte ; Échap ou un clic dehors la ferme. */
export async function ouvrirFiche(id, ctx, onglet = "details") {
  fermerCarte();
  const d = await ctx.appeler("fiche_veille", { business: ctx.business, id });
  if (!fiche) fiche = voileFiche();
  fiche.id = d.id;
  const video = h("div", { class: "colonne-video" }, miniature(d.cles, { plateforme: d.plateforme, largeur: "100%", hauteur: "100%", rayon: "0" }), h("span", { class: "passage tourne" }, icone("relancer"), "Téléchargement"));
  const infos = h("div", { class: "colonne-infos" });
  const montrer = (o) => {
    onglet = o;
    infos.replaceChildren(tete(d), onglets(d, onglet, montrer), h("div", { class: "corps-fiche" }, corps(d, onglet, ctx)));
  };
  montrer(onglet === "analyse" || (onglet === "legende" && d.legende) ? onglet : "details");
  fiche.fenetre.replaceChildren(video, infos);
  let chemin;
  try {
    chemin = await ctx.appeler("lire_video", { id: d.id });
  } catch {
    // l'erreur est déjà en toast
    video.querySelector(".passage")?.replaceChildren("Vidéo indisponible");
    return;
  }
  if (fiche?.id === d.id && video.isConnected) video.replaceChildren(h("video", { src: ctx.fichier(chemin), controls: true, autoplay: true, loop: true }));
}

function voileFiche() {
  const fenetre = h("div", { class: "fenetre-fiche verre" });
  const voile = h("div", { class: "voile fiche", onmousedown: (e) => e.target === voile && fermer() }, fenetre);
  const echap = (e) => {
    if (e.key !== "Escape") return;
    e.stopPropagation();
    fermer();
  };
  function fermer() {
    voile.remove();
    document.removeEventListener("keydown", echap, true);
    fiche = null;
  }
  document.addEventListener("keydown", echap, true);
  document.body.append(voile);
  return { fenetre, fermer, id: null };
}

function tete(d) {
  return h(
    "div",
    { class: "tete-fiche" },
    h("span", { class: "logo-fiche" }, logoPlateforme(d.plateforme)),
    h("div", { class: "qui" }, h("div", { class: "compte" }, d.compte), h("div", { class: "meta" }, d.meta)),
    d.mot && pastille(`« ${d.mot} »`, { neutre: true }),
    h("button", { class: "fermer-fiche survolable", title: "Fermer (Échap)", onclick: () => fiche?.fermer() }, icone("fermer")),
  );
}

function onglets(d, choisi, montrer) {
  const liste = [
    { id: "details", libelle: "Détails" },
    { id: "analyse", libelle: "Analyse" },
    d.legende && { id: "legende", libelle: "Légende" },
  ].filter(Boolean);
  return h("div", { class: "onglets-fiche" }, segmente(liste, choisi, montrer));
}

function corps(d, onglet, ctx) {
  if (onglet === "legende") return h("div", { class: "legende-fiche" }, d.legende);
  if (onglet === "analyse") return analyse(d);
  return [
    h(
      "div",
      { class: "kpis-fiche" },
      viralite(d.viralite),
      h(
        "div",
        { class: "viralite", style: { "--c": "var(--texte)" } },
        surtitre("Engagement"),
        h("div", { class: "coef" }, h("span", { class: "valeur" }, d.engagement ?? "…")),
        d.rang && pastille(d.rang, { couleur: d.rang.startsWith("Top") ? "var(--succes)" : "var(--secondaire)" }),
      ),
    ),
    h(
      "div",
      { class: "tuiles-fiche" },
      d.chiffres.map((x) => h("div", { class: "tuile" }, surtitre(x.nom), h("div", { class: "valeur" }, x.valeur))),
    ),
    d.similaires.length > 0 &&
      h(
        "div",
        { class: "similaires" },
        surtitre("Vidéos similaires"),
        h(
          "div",
          { class: "grille-similaires" },
          d.similaires.map((s) =>
            h(
              "button",
              { class: "similaire survolable", title: s.legende, onclick: () => ouvrirFiche(s.id, ctx) },
              h("span", { class: "affiche" }, miniature(s.cles, { plateforme: s.plateforme, largeur: "100%", hauteur: "100%", rayon: "0" }), s.badge && h("span", { class: "badge pastille", style: { "--c": s.outlier ? "var(--succes)" : "var(--texte)" } }, s.badge)),
              h("span", { class: "vues" }, s.vues),
            ),
          ),
        ),
      ),
    h("button", { class: "voir-sur", onclick: () => ctx.lien(d.url) }, `Voir sur ${d.nomPlateforme} ↗`),
  ];
}

function analyse(d) {
  const a = d.analyse;
  if (!a) return h("div", { class: "attente" }, d.attente);
  const [hook, ...suite] = a.sections;
  const section = (s) => h("div", { class: "section-analyse" }, surtitre(s.titre), h("div", { class: s.fort ? "texte fort" : "texte" }, s.texte));
  return h(
    "div",
    { class: "analyse" },
    h("div", { class: "pastilles" }, a.pastilles.map((p) => pastille(p.texte, { couleur: p.couleur && `var(--${p.couleur})` }))),
    section(hook),
    d.anatomie.length > 0 && h("div", { class: "section-analyse" }, surtitre("Anatomie du script"), anatomie(d.anatomie, { detail: true })),
    suite.map(section),
  );
}
