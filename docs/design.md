# Design

La référence est celle de la version macOS : un **terminal de marché noir et blanc** (14 nov. 2026) : fond noir, cartes noires à peine relevées, chiffres blancs énormes, petits titres en capitales monospace. La couleur ne sert qu'à porter un sens. Dense, lisible d'un coup d'œil. Les jetons sont ceux de `Theme.swift`, portés tels quels en variables CSS ; le fonctionnement de l'interface (fichiers, écrans, contexte, raccourcis) est dans [`crates/app/ui/README.md`](../crates/app/ui/README.md).

## Les règles

- **Fond noir** `--fond`, avec une seule lueur blanche qui tombe du haut de la fenêtre (`.lueur`, posée par `app.js`, dégradé `--lueur`). Jamais de dégradé de couleur ni d'image derrière les cartes.
- **La carte** : la classe `.verre` sur chaque panneau, pilule ou bouton (`css/composants.css`), `.capsule` pour une pilule. Fond `--carte` sous le `--reflet` qui s'éteint vers le bas, filet `--blanc-bord`, arête claire en haut (`--arete`), ombre longue (`--ombre-verre`). Le rayon se règle par `--rayon`.
- **Couleur = sens**, jamais décor, et toujours avec un libellé :
  - blanc (`--accent` = `--texte`) : ce qui est choisi, la série principale, un lien, ce qui passe un seuil ;
  - vert `--succes` et rouge `--danger` : ce qui monte ou baisse (`couleurTon(ton, inverse)`), une source ok ou en panne ;
  - ambre : ce qui demande d'agir (tâche, retard, collecte en pause, erreur d'une source) ;
  - orange, bleu, aqua, violet : des séries, des plateformes ou des business à distinguer.
  Tout le reste est blanc en opacité (`--blanc-*`), `--secondaire` ou `--tertiaire`.
- **Texte** : la police du système, uniquement par les jetons `--f-*`. Les petits titres de carte, les tags et les axes sont en monospace (`--f-surtitre`, `--f-tag`, `--f-axe`), le titre de carte par `surtitre()` (capitales, interlettrage `--i-surtitre`). Chiffres en `tabular-nums` (`.chiffres`), un gros chiffre par `chiffre()`, serré (`--i-kpi`, `--i-heros`). Français, sans tiret cadratin, nombres par `js/format.js`.
- **Couleurs, tailles, rayons, espaces, polices et durées** viennent toujours de `css/theme.css` : aucune valeur en dur ailleurs. Un jeton manque : il s'ajoute dans `theme.css`, avec le nom de `Theme.swift` s'il y existe.
- **Tout bouge, rien ne boucle.** Chaque changement d'état s'anime, mais rien ne tourne en continu, sauf le point d'une source en cours (`.point.en-cours`) et l'icône d'actualisation pendant une collecte (`.tourne`). Voir § Mouvement.
- **Même rendu dans WebKit (Mac) et WebView2 (Windows)** : aucun `-webkit-` sans son équivalent standard.
- Pas de redesign au passage : on applique les composants existants.

## Jetons (`crates/app/ui/css/theme.css`)

| Groupe | Contenu |
|---|---|
| Surfaces | `--fond` 050506, `--carte` 0C0C0E, `--releve` 17171A (surface posée sur une carte : bulle, champ, ligne survolée) |
| Texte | `--texte` F5F5F4, `--secondaire` A1A1A6, `--tertiaire` 66666C ; `--accent` = `--texte` |
| États | `--succes` 4ADE80, `--ambre` FBBF24, `--danger` F87171 |
| Catégories | `--orange` D95926, `--bleu` 3987E5, `--aqua` 199E70, `--violet` 9085E9. `teinte(cle)` (`js/composants.js`) lit la teinte d'un business sous son ancien nom (`menthe` aqua, `ciel` bleu, `lilas` violet, `ambre` orange, aqua par défaut) ; `couleurPlateforme(p)` : TikTok aqua, Instagram violet, YouTube orange, Facebook bleu |
| Blanc | `--blanc-bord`, `--blanc-bord-fort`, `--blanc-bloc`, `--blanc-ligne`, `--blanc-survol`, `--blanc-pilule`, `--blanc-actif`, `--blanc-selection`, `--blanc-fantome`, `--blanc-barre`, `--blanc-repere`, `--blanc-grille`, `--blanc-serie`, `--blanc-trait`, `--blanc-halo` ; `--noir-ombre`, `--voile` (sous la palette et le lecteur), `--voile-stats` (les chiffres au survol d'une case de la Veille) |
| Dégradés, ombres | `--lueur`, `--reflet`, `--arete`, `--lavis` (aire sous une courbe), `--degrade-heros` ; `--ombre-verre`, `--halo-actif`, `--halo-nav`, `--halo-heros`, `--halo-barre` |
| Traits | `--trait` 2px (une courbe), `--trait-case`, `--filet` 1px, `--viseur`, `--trait-icone` |
| Rayons `--r-*` | `panneau` 20, `carte` 18, `bloc` 14, `bouton` 12, `tache` 11, `ligne` 10, `avatar` 8, `barre` 4, `legende` 3, `capsule` |
| Espaces `--e-*` | `xs` 2, `s` 4, `m` 8, `l` 10, `xl` 12, `xxl` 16, `panneau` 20, `page` 28, et ceux d'un composant (pastille, segment, item de navigation…) |
| Tailles `--t-*` | `barre-haut` 52 et `feux` 78 (la place des boutons de la fenêtre sur Mac), `bouton-fenetre` 46 (Windows, Linux), `avatar` 32, `avatar-mini` 26, logos 12, 16, 20, lignes (fil, vidéo, tâche), sparkline, graphiques 128, 170, 210, `marque` 20, `panneau-reseau` 248, `panneau-plan` 280, `rail` 52, `radar` 156, `jauge` 6, `conversation` 460 et `reponse` 88 (le SAV), palette, Réglages, toast ; `--colonnes-videos`, la grille du tableau des vidéos |
| Polices | `--police` (`system-ui`, Segoe UI sous Windows), `--police-mono` (SF Mono, Cascadia Mono) ; `--f-titre`, `--f-h2`, `--f-kpi` 30, `--f-grand`, `--f-heros` 76, `--f-marque` 18, `--f-corps`, `--f-corps-gras`, `--f-ligne`, `--f-label`, `--f-petit`, `--f-mini`, `--f-mini-gras` ; monospace : `--f-surtitre`, `--f-tag`, `--f-axe` ; symboles : `--f-coche`, `--f-lettre`, `--f-fleche`, `--f-symbole`, `--f-icone`, `--f-symbole-grand` ; interlettrages `--i-*` |
| Mouvement | voir § Mouvement |

Le JS lit un jeton par `jeton(nom)` et `nombreJeton(nom)` (`js/composants.js`), jamais une valeur recopiée.

## La fenêtre

Une fenêtre `main` de 1320 × 920, au moins 1080 × 720, fond `#050506`, thème sombre (`tauri.conf.json`).

- **Mac** (`tauri.macos.conf.json`) : barre de titre `Overlay`, titre caché. Les boutons du système restent ; la barre du haut leur laisse `--t-feux` à gauche (`.mac .barre-haut`).
- **Windows et Linux** (`tauri.windows.conf.json`, `tauri.linux.conf.json`) : sans décorations. L'interface dessine réduire, agrandir et fermer à droite de la barre (`.fenetre`, `--t-bouton-fenetre`, fermer en `--danger` au survol).

La barre se déplace par `data-tauri-drag-region`. `app.js` pose la classe `mac` ou `pc` sur la racine selon `plateforme`.

## La barre du haut (`js/app.js`, `css/app.css`)

Il n'y a pas de barre latérale. Une barre de `--t-barre-haut` (52 px) en trois tiers :

- **gauche** : le business affiché (avatar, nom, chevrons) ; un menu pour passer à Tous ou à un business ;
- **centre** : deux pilules d'écrans coupées avant SAV (`NAVIGATION` = 7 écrans, `GAUCHE` = 5 : Accueil, Data, Vidéos, Veille, Santé | SAV, Apps de potes), avec entre elles le logo (`marque()`, l'anneau et l'horizon de l'icône en SVG) et « Cockpit » en `--f-marque`. La pilule blanche glisse d'un écran à l'autre (`glisseur`) ; le compteur ambre du SAV suit le mot ;
- **droite** : la bande des sources (un point par source et l'âge de la collecte, une bulle au survol, un clic ouvre Santé), Rechercher (⌘K ou Ctrl+K), Tout relancer (la flèche tourne pendant la collecte), Réglages, puis les boutons de la fenêtre hors Mac.

## Composants partagés (`js/composants.js`)

La liste exacte et les options de chacun sont en tête du fichier ; les styles dans `css/composants.css`. Chaque composant rend un élément DOM, sans état caché :

| Composant | Usage |
|---|---|
| `h`, `s` | un élément HTML ou SVG |
| `panneau`, `bloc`, `enTetePanneau` | un panneau de verre titré en surtitre, avec ses accessoires à droite |
| `rangee(poids, enfants, { meneur })` | panneaux côte à côte aux largeurs proportionnelles ; `meneur` donne la hauteur, les autres défilent |
| `segmente(options, choisi, quandChoisi)` | la pilule à choix unique (business, période, tri, pays), la pilule blanche glisse d'un choix à l'autre |
| `pilule` | un bouton capsule, avec icône |
| `chiffre`, `kpi`, `grilleKpi` | le gros chiffre qui roule ; la carte KPI (surtitre, pastille de delta, chiffre, sparkline, note) ; leur grille |
| `sparkline`, `courbe`, `barres`, `legende` | graphiques en SVG ; `courbe` a un viseur et une bulle au survol |
| `pastille`, `point`, `vide` | étiquette colorée, pastille d'état, message de liste vide |
| `avatar`, `miniature`, `logoPlateforme` | icône du business (sa lettre sans app), image du cache disque, logo d'une plateforme (`img/logo-*.svg`) |
| `ligneFil`, `caseRonde` | une ligne du fil, une case de tâche |
| `icone(nom)` | les icônes de la barre et des écrans, en traits sur une grille de 24 |

Aucune image n'est chargée par le réseau depuis l'interface : miniatures, avatars et icônes d'apps sont téléchargés par les sources dans le cache disque (`noyau::collecte::miniatures`), et l'interface les lit par le protocole `asset` (`ctx.image(cle)`) ; les vidéos de la Veille aussi (`ctx.fichier(chemin)`). La CSP n'ouvre que les images de `*.mzstatic.com`.

## Mouvement

Les ressorts de la version SwiftUI sont échantillonnés en courbes `linear()` dans `theme.css`, chacun avec sa durée :

| Jeton | Durée | Où |
|---|---|---|
| `--ressort-navigation` | 603 ms | écran, business, pilules, palette |
| `--ressort-chiffre` | 733 ms | les chiffres qui roulent |
| `--ressort-survol` | 393 ms | survol, appui (`.survolable`) |
| `--ressort-entree` | 711 ms | l'entrée d'un bloc |
| `--courbe-trace` | 900 ms (`--duree-trace`) | le tracé d'une courbe |

Autres jetons : `--duree-fondu` 250 ms (toast), `--decalage` 50 ms entre deux blocs (au plus `--rang-max` 8), `--decalage-barre`, `--appui` 0,96, `--appui-carte` 0,98, `--survol-echelle`, `--glisse` 14 px, `--flou` 8 px, `--reduit`.

| Outil | Où |
|---|---|
| `entree(el, rang)` | les blocs d'un écran, l'un après l'autre : fondu, glisse et flou qui se dissipe |
| `bascule` (`app.js`) | le changement d'écran : le nouvel écran glisse depuis la droite s'il est plus à droite dans la barre, depuis la gauche sinon, avec un flou |
| `glisseur(conteneur)` | la pilule qui glisse vers l'élément actif : barre du haut, `segmente` |
| `chiffre` | part de zéros à l'apparition, puis roule depuis l'ancienne valeur gardée sous sa `cle` |
| `.survolable` | une carte se soulève sous la souris et s'enfonce à l'appui |

Un re-rendu (nouvelle version de la base, changement de business, horloge de 30 s) se fait en mode calme (`rendu.calme`) : ni les entrées ni les tracés ne se rejouent, seuls les chiffres roulent.

**Moins d'animations** : `prefers-reduced-motion` coupe toute animation et transition (`theme.css`), et `sobre()` fait sauter les animations lancées en JS.

## Icônes

Dans `crates/app/icons` : `icon.icns` (Mac), `icon.ico` (Windows) et les PNG listés dans `tauri.conf.json`, pour l'app et l'installeur ; `tray.png` (source `tray.svg`) pour le tray, posé en image modèle (`icon_as_template`) : macOS la teinte selon le thème de la barre des menus. Le logo de la barre du haut est dessiné en SVG par `marque()`.
