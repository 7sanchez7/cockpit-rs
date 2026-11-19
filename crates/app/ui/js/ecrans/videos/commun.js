// Ce que partagent Vidéos et Veille : le menu du clic droit, le popover d'ajout, la case 9:16 d'une vidéo, le
// choix du business d'un ajout et le remplacement du contenu qui garde le défilement des colonnes.

import { h, jeton, logoPlateforme, miniature, nombreJeton, sobre } from "../../composants.js";

/** Pose `el` au-dessus de tout, à (x, y) gardé dans la fenêtre ; un clic dehors ou Échap le ferme. */
function flottant(el, x, y) {
  const fermer = () => {
    el.remove();
    document.removeEventListener("mousedown", dehors, true);
    document.removeEventListener("keydown", echap, true);
  };
  const dehors = (e) => {
    if (!el.contains(e.target)) fermer();
  };
  const echap = (e) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      fermer();
    }
  };
  document.body.append(el);
  const marge = nombreJeton("--e-m");
  el.style.left = Math.max(marge, Math.min(x, innerWidth - el.offsetWidth - marge)) + "px";
  el.style.top = Math.max(marge, Math.min(y, innerHeight - el.offsetHeight - marge)) + "px";
  document.addEventListener("mousedown", dehors, true);
  document.addEventListener("keydown", echap, true);
  if (!sobre()) {
    el.animate([{ opacity: 0, transform: `scale(${jeton("--appui")})` }, { opacity: 1, transform: "none" }], {
      duration: nombreJeton("--duree-survol"),
      easing: jeton("--ressort-survol"),
    });
  }
  return fermer;
}

/** Le clic droit sur `el` ouvre un menu ; items : `{ libelle, faire, danger }` ou `null` pour un filet. */
export function menuContextuel(el, items) {
  el.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    let fermer;
    const menu = h(
      "div",
      { class: "menu verre" },
      items.map((it) => (it ? h("button", { class: it.danger ? "danger" : null, onclick: () => (fermer(), it.faire()) }, it.libelle) : h("hr"))),
    );
    fermer = flottant(menu, e.clientX, e.clientY);
  });
  return el;
}

/** Un popover sous `ancre` ; `contenu(fermer)` rend son contenu. */
export function popover(ancre, contenu) {
  const r = ancre.getBoundingClientRect();
  let fermer;
  const el = h("div", { class: "popover verre" });
  el.append(contenu(() => fermer()));
  fermer = flottant(el, r.left, r.bottom + nombreJeton("--e-m"));
  el.style.left = Math.max(nombreJeton("--e-m"), Math.min(r.left + r.width / 2 - el.offsetWidth / 2, innerWidth - el.offsetWidth - nombreJeton("--e-m"))) + "px";
  el.querySelector("input")?.focus();
}

/** Le business d'un ajout : un menu s'il y en a plusieurs ; `choisir(id)` reçoit le choix. */
export function choixBusiness(ctx, choisi, choisir) {
  if (ctx.businesses.length < 2) return null;
  return h(
    "select",
    { class: "choix-ajout", onchange: (e) => choisir(e.target.value) },
    ctx.businesses.map((b) => h("option", { value: b.id, selected: b.id === choisi }, b.nom)),
  );
}

/** Le bouton d'un popover (Suivre, Ajouter) ; `disabled` le grise. */
export function bouton(libelle, faire) {
  return h("button", { class: "bouton-ajout", onclick: faire }, libelle);
}

/**
 * Une vidéo de grille : miniature 9:16 voilée en bas, vues, logo, mesure du tri en pastille ; dessous, `legende`
 * sur `lignes` lignes.
 */
export function caseVideo({ cles, plateforme, vues, badge, badgeCouleur, legende, lignes, choisie, aide, clic }) {
  return h(
    "button",
    { class: "case-video survolable" + (choisie ? " choisie" : ""), title: aide, onclick: clic },
    h(
      "span",
      { class: "affiche" },
      miniature(cles, { plateforme, largeur: "100%", hauteur: "100%", rayon: "0" }),
      h("span", { class: "voile-bas" }),
      h("span", { class: "vues" }, h("span", { class: "lecture" }, "▶"), vues),
      h("span", { class: "logo-coin" }, logoPlateforme(plateforme)),
      badge != null && h("span", { class: "badge pastille", style: { "--c": badgeCouleur } }, badge),
    ),
    h("span", { class: "legende", style: { "--lignes": lignes } }, legende),
  );
}

/** Remplace le contenu de `racine` en gardant le défilement de la page et des éléments marqués `data-defile`. */
export function remplacer(racine, ...enfants) {
  const gardes = new Map([...racine.querySelectorAll("[data-defile]")].map((el) => [el.dataset.defile, el.scrollTop]));
  const haut = racine.scrollTop;
  racine.replaceChildren(...enfants);
  racine.scrollTop = haut;
  for (const el of racine.querySelectorAll("[data-defile]")) el.scrollTop = gardes.get(el.dataset.defile) ?? 0;
}
