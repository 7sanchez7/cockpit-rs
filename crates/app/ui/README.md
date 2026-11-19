# L'interface du Cockpit

HTML, CSS et JavaScript en modules ES natifs, servis tels quels par Tauri (`frontendDist: "ui"`). Pas de npm, pas de bundler, aucune dépendance. Elle doit marcher pareil dans WebKit (Mac) et WebView2 (Windows) : aucun `-webkit-` sans son équivalent standard.

```
index.html          charge les trois feuilles et js/app.js
css/theme.css       tous les jetons : couleurs, rayons, espaces, tailles, polices, ressorts, durées
css/composants.css  les composants partagés
css/app.css         la barre du haut, la zone d'écran, la palette, les toasts
js/pont.js          appeler, ecouter, lien, image, toast, fenetre
js/format.js        nombres, argent, dates (heure de Paris) au format français
js/composants.js    h(), animations, composants
js/palette.js       la palette ⌘K (Ctrl+K hors Mac)
js/app.js           la racine : barre, écrans, raccourcis, rafraîchissement
js/ecrans/<id>.js   un module par écran, chargé à la demande
```

Aucune couleur ni taille en dur hors de `theme.css`. Rien ne boucle, sauf le point d'une source en cours et l'icône d'actualisation pendant une collecte. `prefers-reduced-motion` coupe tout.

## Écrire un écran

Les écrans, dans l'ordre des raccourcis : `app`, `data`, `videos`, `veille`, `sante`, `aTraiter`, `apps`, puis `reglages` (⌘,). Un écran est `js/ecrans/<id>.js` :

```js
import { panneau, grilleKpi, kpi, ligneFil, vide } from "../composants.js";
import * as F from "../format.js";

export default {
  titre: "Data",
  sousTitre: "",           // le texte de départ ; ctx.page.sousTitre() le change
  defile: true,            // false : la page ne défile pas, ses panneaux défilent
  business: true,          // false : pas de choix de business dans l'en-tête
  async rendre(racine, ctx) {
    const d = await ctx.appeler("data", { business: ctx.business });
    ctx.page.sousTitre(F.dateLongue(ctx.maintenant));
    racine.replaceChildren(
      grilleKpi(kpi({ titre: "Essais", valeur: d.essais, cle: "data.essais" })),
      panneau({ titre: "Fil" }, d.fil.length ? d.fil.map((e) => ligneFil(e)) : vide("Rien aujourd'hui.")),
    );
  },
};
```

`rendre` construit tout, puis remplace le contenu d'un coup : pas de clignotement. Il est rappelé à chaque nouvelle version de la base (événement `cockpit`), au changement de business et toutes les 30 s, avec `ctx.rafraichi` vrai : les entrées et les tracés ne se rejouent pas, les chiffres qui portent une `cle` roulent depuis leur ancienne valeur. Un écran qui veut garder un état local (un onglet, une saisie) s'abonne avec `ctx.abonner(fn)` : `fn(ctx)` remplace alors le re-rendu complet.

Le contexte :

| Champ | |
|---|---|
| `appeler(cmd, args)` | une commande Rust ; l'erreur s'affiche en toast puis se relance |
| `lien(url)`, `image(cle)` | ouvrir une adresse ; l'url d'une image du cache (`icone:<appId>`) |
| `aller(id)` | changer d'écran |
| `montrer(genre, id)` | ouvrir l'écran qui montre ce résultat, sur lui (`video`, `app`, `tache`), comme la palette |
| `business`, `businesses` | l'id choisi (null pour « Tous ») ; `[{ id, nom, lettre, teinte, appStoreId }]` |
| `sources`, `enCours`, `pause` | `[{ id, nom, passage }]`, les ids en cours, la collecte en pause |
| `plateforme` | `macos`, `windows`, `linux` |
| `version`, `maintenant`, `rafraichi` | la version de la base, l'heure du rendu, faux au premier rendu |
| `page.sousTitre(t)`, `page.outils(...n)` | l'en-tête |
| `abonner(fn) -> arrêt` | voir plus haut |
| `choix` | `{ genre, id }` quand la palette ou `montrer` ouvre l'écran sur un résultat (`video`, `app`, `tache`), null sinon ; valable pour ce seul rendu |

## Appeler Rust

`appeler("etat")` renvoie `{ version, enCours, pause, reglages, plateforme, businesses, sources, aTraiter, miniatures }`. Les commandes communes sont dans `src/commandes.rs` : `regler(cle, valeur)` pour un réglage de fenêtre, `ecrire_reglage` pour un réglage qui change les écrans, `pause(oui)`, `relancer(source | null)`, `lien(url)`, `session(site)`. Les noms d'arguments sont en camelCase côté JS. Aucun appel réseau depuis l'interface : la CSP n'ouvre que les images `*.mzstatic.com`.

## Composants

La liste exacte et leurs options sont en tête de `js/composants.js`. Les plus courants : `h(tag, attrs, ...enfants)`, `remplacer(el, ...enfants)` (le `replaceChildren` natif écrirait les tableaux et les `false` en texte), `panneau`, `rangee`, `segmente`, `pilule`, `kpi` et `grilleKpi`, `chiffre`, `sparkline`, `courbe`, `barres`, `avatar`, `miniature`, `ligneFil`, `caseRonde`, `pastille`, `point`, `vide`.

## Raccourcis

⌘ sur Mac, Ctrl ailleurs : 1 à 7 pour les écrans, K pour la palette, R pour tout relancer (F5 aussi), `,` pour les réglages. Ctrl+R et F5 ne rechargent jamais la page.
