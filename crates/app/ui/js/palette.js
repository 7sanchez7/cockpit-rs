// La palette ⌘K (Ctrl+K hors Mac), portée de Palette.swift.
//
//   ouvrirPalette(actions) -> fermer() -> vrai si elle était ouverte
//     actions.resultats() -> [{ icone | avatar, titre, detail, faire() }]  tous les résultats, la palette filtre
//     actions.chercher(texte) -> Promise<[même forme]>  facultatif : ce que la base trouve, ajouté après, déjà filtré
// Flèches pour bouger, Entrée pour lancer, Échap ou un clic sur le voile pour fermer.

import { h, icone, avatar, vide, glisseur, sobre, jeton, nombreJeton } from "./composants.js";

const MAX_RESULTATS = 40;

function normaliser(t) {
  return t.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();
}

export function ouvrirPalette(actions) {
  const champ = h("input", { type: "text", placeholder: "Écran, vidéo, app, tâche, action", spellcheck: "false", autocomplete: "off" });
  const liste = h("div", { class: "liste", role: "listbox" });
  const pilule = glisseur(liste);
  const boite = h("div", { class: "palette verre" }, h("label", { class: "champ-palette" }, icone("loupe"), champ), liste);
  const voile = h("div", { class: "voile" }, boite);
  const precedent = document.activeElement;
  let lignes = [];
  let trouves = [];
  let choix = 0;
  let ouverte = true;

  const choisir = (i, anime = true) => {
    choix = i;
    lignes.forEach((l, k) => l.classList.toggle("actif", k === i));
    const l = lignes[i] ?? null;
    if (!anime) pilule.aller(null);
    pilule.aller(l);
    l?.scrollIntoView({ block: "nearest" });
  };

  const lancer = (r) => {
    fermer();
    r.faire();
  };

  let demande = 0;

  const montrer = (resultats, garder) => {
    trouves = resultats.slice(0, MAX_RESULTATS);
    lignes = trouves.map((r, i) =>
      h(
        "button",
        { class: "resultat", role: "option", onmousemove: () => choix !== i && choisir(i), onclick: () => lancer(r) },
        r.icone ? icone(r.icone) : avatar(r.avatar, { mini: true }),
        h("span", { class: "titre" }, r.titre),
        r.detail && h("span", { class: "detail" }, r.detail),
      ),
    );
    liste.replaceChildren(liste.firstChild, ...(lignes.length ? lignes : [vide("Rien ne correspond.")]));
    choisir(garder ? Math.min(choix, Math.max(0, lignes.length - 1)) : 0, garder);
  };

  const remplir = () => {
    const texte = champ.value.trim();
    const q = normaliser(texte);
    const fixes = actions.resultats().filter((r) => !q || normaliser(r.titre).includes(q) || normaliser(r.detail ?? "").includes(q));
    montrer(fixes, false);
    const n = ++demande;
    if (!q || !actions.chercher) return;
    actions.chercher(texte).then(
      (plus) => n === demande && ouverte && plus.length && montrer([...fixes, ...plus], true),
      (e) => console.error(e),
    );
  };

  function fermer() {
    if (!ouverte) return false;
    ouverte = false;
    document.removeEventListener("keydown", clavier, true);
    const fin = () => {
      voile.remove();
      precedent?.focus?.();
    };
    if (sobre()) fin();
    else voile.animate([{ opacity: 1 }, { opacity: 0 }], { duration: nombreJeton("--duree-fondu"), fill: "forwards" }).finished.then(fin);
    return true;
  }

  function clavier(e) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      fermer();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (trouves.length) choisir(Math.min(trouves.length - 1, Math.max(0, choix + (e.key === "ArrowDown" ? 1 : -1))));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (trouves[choix]) lancer(trouves[choix]);
    }
  }

  champ.addEventListener("input", remplir);
  voile.addEventListener("mousedown", (e) => {
    if (e.target === voile) fermer();
  });
  document.addEventListener("keydown", clavier, true);
  document.body.append(voile);
  remplir();
  champ.focus();
  if (!sobre()) {
    voile.animate([{ opacity: 0 }, { opacity: 1 }], { duration: nombreJeton("--duree-fondu") });
    boite.animate(
      [
        { opacity: 0, transform: `translateY(calc(${jeton("--glisse")} * -1)) scale(${jeton("--appui")})` },
        { opacity: 1, transform: "none" },
      ],
      { duration: nombreJeton("--duree-navigation"), easing: jeton("--ressort-navigation") },
    );
  }
  return fermer;
}
