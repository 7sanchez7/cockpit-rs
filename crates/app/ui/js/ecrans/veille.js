// Veille : le site de Viral Bank, dans une WebView native posée sur l'écran (src/ecrans/veille.rs). Ce module ne fait
// que lui donner sa place : le rectangle du cadre, recalé quand il change. La vue native passe au-dessus de la page :
// elle se cache quand on quitte l'écran et tant qu'une couche (palette, menu) est ouverte.

import { h } from "../composants.js";

// ce qui vit dans <body> sans couvrir l'écran ; tout autre enfant est une couche posée dessus
const FOND = ".lueur, .racine, .toasts";

let pose = null;
let file = Promise.resolve();

// une commande à la fois : un « cacher » ne doit pas passer avant le « montrer » qui le précède
function envoyer(appeler, cmd, args) {
  file = file.then(() => appeler(cmd, args)).catch(() => {});
}

function recaler(p) {
  if (p !== pose || !p.pret) return;
  if ([...document.body.children].some((el) => !el.matches(FOND))) return envoyer(p.appeler, "cacher_veille");
  const r = p.cadre.getBoundingClientRect();
  envoyer(p.appeler, "montrer_veille", { x: r.left, y: r.top, largeur: r.width, hauteur: r.height });
}

// la bascule et l'entrée de l'écran déplacent le cadre : la vue attend qu'elles finissent
async function poser(p) {
  await new Promise(requestAnimationFrame);
  const animations = p.cadre.closest(".ecran")?.getAnimations({ subtree: true }) ?? [];
  const finies = animations.filter((a) => a.effect?.getComputedTiming().endTime !== Infinity);
  await Promise.all(finies.map((a) => a.finished.catch(() => {})));
  p.pret = true;
  recaler(p);
}

function quitter() {
  if (!pose) return;
  pose.arrets.forEach((arreter) => arreter());
  envoyer(pose.appeler, "cacher_veille");
  pose = null;
}

export default {
  titre: "Veille",
  sousTitre: "Viral Bank",
  defile: false,
  business: false,
  rendre(racine, ctx) {
    // rappelé toutes les 30 s et au changement de business, qui déplace le cadre dans la nouvelle page
    if (pose && racine.contains(pose.cadre)) return;
    quitter();
    const cadre = h("div", { class: "vide remplit" }, "Viral Bank s'ouvre ici.");
    racine.replaceChildren(cadre);
    const p = { cadre, appeler: ctx.appeler, pret: false, arrets: [] };
    const taille = new ResizeObserver(() => recaler(p));
    const couches = new MutationObserver(() => recaler(p));
    taille.observe(cadre);
    couches.observe(document.body, { childList: true });
    p.arrets.push(() => taille.disconnect(), () => couches.disconnect());
    pose = p;
    poser(p);
  },
  quitter,
};
