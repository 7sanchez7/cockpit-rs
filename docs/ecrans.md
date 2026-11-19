# Écrans

## Réglages

`ui/js/ecrans/reglages.js` (id `reglages`, ⌘, ou Ctrl+, ; hors de la barre du haut), commandes dans `src/ecrans/reglages.rs`. Dans le Swift, une fenêtre à part ; ici un écran de la fenêtre principale, cinq onglets en segmenté dans l'en-tête, chacun en formulaire groupé dans une colonne de `--t-reglages-l`. Business et Clés gardent la saisie en cours quand la base change ; Notifications, Collecte et Sessions se relisent (`ctx.abonner`).

- **Business** (`reglages_business`, `enregistrer_business`, `supprimer_business`) : le business de la barre du haut, sinon le premier ; « Nouveau business » ouvre un brouillon vide. Identité (nom, lettre, teinte de l'avatar), sources (id App Store, issuer et numéro de fournisseur ASC, paquet Play et bucket des rapports, projet Firebase, hôte PostHog, dossier des fiches de contenu ; un champ vide coupe la source), pays suivis, catégorie App Store, objectif mensuel en €. `Brouillon::business` fait les contrôles en Rust : id tiré du nom (minuscules sans accents), lettre de 2 caractères au plus (l'initiale du nom si vide), objectif et catégorie en nombres ; les noms des événements PostHog et le reste de la config sont gardés tels quels. Entrée ou « Enregistrer » enregistre ; « Supprimer » demande confirmation sur place et reste grisé pour le dernier business.
- **Clés** (`reglages_cles`, `ranger_cle`, `effacer_cle`) : pour chaque secret du business (`noyau::secrets`, service `cockpit`, compte `<business>.<NOM>`, le même que le Swift), présente ou absente, jamais la valeur. Une clé absente a son champ masqué et « Ranger » ; une clé présente, « Effacer ». La saisie ne vit que dans le module JS et s'efface dès qu'elle est rangée. Section Fichiers : la clé `.p8` ASC et les identifiants Google (ADC) trouvés ou non.
- **Notifications** (`reglages_notifs`, puis `ecrire_reglage`) : un interrupteur par type du fil (`TypeFil::reglage_notif`, `notifs.<type>` ; vide = oui, `0` = non).
- **Collecte** (`reglages_collecte`, `pause`, `relancer`, `montrer_base`) : pause, « Tout relancer », la cadence de chaque source, le chemin de la base et « Montrer dans le Finder » (« dans l'Explorateur » sous Windows).
- **Sessions** (`reglages_sessions`, puis `session`) : TikTok, Instagram et Facebook, avec « session de Safari », « session de l'app » ou « pas de session », et « Se connecter » (ou « Reconnecter ») qui ouvre le login du site dans une fenêtre de l'app. Onglet absent du Swift.

## Barre de menus et barre des tâches

Sur Mac, la barre des menus de l'app est en français (`menu` dans `src/main.rs`) : Cockpit (À propos, Masquer, Quitter), Édition (Annuler, Couper, Copier, Coller, Tout sélectionner, que les champs de texte de la WebView utilisent par ⌘X, ⌘C, ⌘V), Fenêtre. Windows n'a pas de menu.

`ui/barre.html` et `ui/js/barre.js`, commandes dans `src/ecrans/barre.rs`. Le clic gauche sur l'icône (`tray.rs`) ouvre ou referme la fenêtre `barre` (sans bord, toujours devant, hors de la barre des tâches), centrée sur l'icône : dessous si l'icône est en haut de l'écran (barre de menus du Mac), dessus si elle est en bas (barre des tâches de Windows), sans déborder de la zone utile. Elle se cache dès qu'elle perd le focus ; un clic sur l'icône dans les 300 ms qui suivent ne la rouvre pas. Le clic droit garde le menu de l'icône.

Le contenu suit `BarreMenus.swift` : l'heure du dernier passage, 4 chiffres du jour (MRR brut en euros, Actifs, Installs, Essais), « SAV » avec le nombre de tâches ouvertes (ambre s'il y en a) qui ouvre l'écran SAV, les 5 derniers événements du fil, puis Ouvrir, Actualiser et Quitter. La lecture (`barre`) suit le business choisi dans la fenêtre principale (réglage `business`, Tous s'il est vide). La page se relit au focus, à chaque événement `cockpit` et toutes les 30 s, se mesure et donne sa hauteur (`barre_taille`). Entrée ouvre le cockpit, Échap cache la fenêtre (`barre_ouvrir`, `barre_cacher`, `quitter`).

## Palette

`ui/js/palette.js`, ouverte par ⌘K (Ctrl+K). Sans saisie : les écrans, « Tout relancer », la relance de chaque source, pause ou reprise, le choix du business. Dès qu'on tape, la palette ajoute après ses résultats ceux de `chercher` (`src/ecrans/palette.rs`, `Lectures::recherche` dans `noyau/src/lecture/recherche.rs`) : les vidéos, les apps suivies et les tâches ouvertes du business affiché dont le titre ou le détail contient le texte, sans accents ni casse, 40 au plus. Une réponse arrivée après une nouvelle frappe est ignorée.

Choisir un résultat ouvre son écran (Vidéos, Apps de potes, SAV) avec `ctx.choix = { genre, id }` (`genre` : `video`, `app` ou `tache`), valable pour ce seul rendu : l'écran peut s'en servir pour ouvrir ou mettre en avant l'élément.
