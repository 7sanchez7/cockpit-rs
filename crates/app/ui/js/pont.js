// Le pont vers Rust.
//
//   appeler(cmd, args?) -> Promise    invoke Tauri ; une erreur s'affiche en toast puis se relance (err.affichee = true)
//   ecouter(nom, fn) -> Promise<arrêt>  un événement Tauri ("cockpit", "ecran"), fn reçoit le payload
//   lien(url)                          ouvre une adresse http(s) dans le navigateur, par la commande `lien`
//   image(dossier, cle) -> url          une image du cache local (clé `icone:123`, `avatar:tiktok:x`) servie par asset:
//   toast(texte)                       un message d'erreur discret, en bas à droite
//   fenetre()                          la fenêtre courante (réduire, agrandir, fermer, glisser)

const tauri = window.__TAURI__;
const DUREE_TOAST = 6000;

export function toast(texte) {
  let pile = document.querySelector(".toasts");
  if (!pile) {
    pile = document.createElement("div");
    pile.className = "toasts";
    document.body.append(pile);
  }
  const el = document.createElement("div");
  el.className = "toast verre";
  const point = document.createElement("span");
  point.className = "point";
  point.style.setProperty("--c", "var(--danger)");
  const message = document.createElement("span");
  message.textContent = texte;
  el.append(point, message);
  pile.append(el);
  setTimeout(() => {
    el.classList.add("part");
    const fondu = el.getAnimations().find((a) => a.transitionProperty === "opacity");
    if (fondu) fondu.finished.then(() => el.remove());
    else el.remove();
  }, DUREE_TOAST);
}

export async function appeler(cmd, args = {}) {
  try {
    return await tauri.core.invoke(cmd, args);
  } catch (e) {
    const err = e instanceof Error ? e : new Error(String(e));
    toast(err.message);
    err.affichee = true;
    throw err;
  }
}

export function ecouter(nom, fn) {
  return tauri.event.listen(nom, (ev) => fn(ev.payload));
}

export function lien(url) {
  return appeler("lien", { url });
}

export function image(dossier, cle) {
  const sep = dossier.includes("\\") ? "\\" : "/";
  return tauri.core.convertFileSrc(dossier + sep + cle.replace(/[:/]/g, "_"));
}

export function fenetre() {
  return tauri.window.getCurrentWindow();
}
