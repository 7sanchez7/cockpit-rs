# Écrans

Chaque écran est un module de `crates/app/ui/js/ecrans/<id>.js` qui appelle une commande Rust de `crates/app/src/ecrans/<id>.rs` : la commande lit la base par `Lectures` (`noyau::lecture`) et rend tout formaté, l'écran ne fait qu'afficher. Le business affiché (Tous ou un seul) restreint toutes les lectures ; un passage du collecteur ou une écriture fait relire l'écran (`ctx.abonner`).

## Santé

Les sources répondent-elles, l'IA tient-elle, combien coûte la journée. Commande `sante(business, hier, source)` (`ecrans/sante.rs`), écran `sante.js`.

- **4 KPI** : sources en panne (pastille « n ok »), coût IA du jour et sa part de voix, échecs IA du jour et le total sur 7 jours, messages ouverts et retours nouveaux.
- **Sources** : état (`etat_source`, partagé avec la barre du haut), cadence, dernier passage, durée (« < 1 s » sous la seconde), lignes écrites, dernier succès (`l.derniers_succes()`), l'erreur en clair en ambre sous la ligne. Un bouton relance une source (`relancer`), même en pause ; l'interrupteur « Collecte en pause » (`pause`) est en tête du panneau.
- **IA par fournisseur**, aujourd'hui ou hier (`hier`) : requêtes, part du total, part de 429, part d'erreurs, latence moyenne (`req_ia.*`, `q429_ia.*`, `err_ia.*`, `ms_ia.*`), et les requêtes par heure empilées par fournisseur.
- **Coût IA sur 14 jours** : une barre par jour, texte et voix, coût et part de voix au survol, total du mois. La voix suit le jour UTC.
- **Journal** : les 30 derniers passages, filtrables par source (`source`, menu `.menu-source`).

Lit `collecte` (`l.derniers_passages`, `l.derniers_succes`, `l.journal`), `l.fournisseurs_ia` et les séries `cout_ia`, `cout_voix`, `echecs_ia`, `messages_ouverts`, `retours_nouveaux`. Seule action réseau : relancer une source, au clic.

## SAV

Le dashboard admin de Roleo (pages Messages et Retours) dans le cockpit : lire un fil, répondre, marquer traité, puis le reste de ce qu'il faut faire aujourd'hui. Commande `sav(business, fil, traites)` (`ecrans/sav.rs`), écran `aTraiter.js`. Sous-titre : « 2 fils à répondre, 5 retours à traiter », ou « Rien à répondre » (`l.nb_sav()`). En tête, à droite : le point d'état de Firebase, « lu il y a 4 min » (l'erreur au survol) et le bouton qui relance la source.

En haut, deux panneaux, la conversation donne la hauteur :

- **Fils** (`l.fils_sav()`, 100 au plus) : les ouverts d'abord, puis du plus récent au plus ancien. Par fil : le prénom, sinon l'email, sinon « Invité » ; l'âge du dernier message ; son début, précédé de « Équipe : » s'il vient de nous ; l'état en pastille, « À répondre » (ambre), « Pas encore lu » (accent, notre réponse n'a pas été ouverte), « Lu » (vert), « Traité » (fermé sans réponse). Un clic ouvre le fil (le premier par défaut).
- **Conversation** : le nom, l'email et l'uid (sélectionnables), les messages du fil (`l.messages_sav`, l'user à gauche, l'équipe à droite, le dernier en bas, `--t-conversation` de haut), puis la réponse : un champ de `--t-reponse`, le compteur « n / 2000 » (rouge au-delà), « Marquer traité sans répondre » tant que le fil est ouvert, et « Envoyer » (⌘ Entrée, Ctrl Entrée hors macOS). Sous le champ, ce qui s'est passé : « Envoyé et notifié », « Envoyé, aucun appareil à notifier », « Envoyé, la notif a échoué : … » ou l'erreur. Un compte qui n'a jamais écrit peut recevoir un premier message.

En bas, deux colonnes :

- **Retours** (`l.retours_sav`), segmenté À traiter, Traités, Tous ; dix cartes, puis « Voir les N ». Par carte : la provenance (Profil, Note, Signalement), la catégorie (Idée, Souci, Réponse bizarre, Faute ou mauvaise langue, Hors sujet, Bug de l'app, Autre ; ambre pour Souci et Bug), la note sur 5 (ambre à 2 ou moins), l'âge ; la bulle du tuteur signalée entre guillemets, le message ; en pied, l'email (ou Invité, Anonyme), l'appareil et le build, « traité le … ». « Répondre » ouvre le fil de son auteur et remonte en haut de l'écran ; « Marquer traité » ou « Rouvrir ».
- **Le reste** : les tâches ouvertes qui ne sont ni un message ni un retour (`l.taches`), rangées par genre (Avis App Store, À la main, Verdicts de vidéos, Autres), chacune avec son compteur, ambre si le groupe contient une alerte ou un retard. Les groupes urgents d'abord, l'urgent d'abord dans un groupe ; huit lignes, puis « Voir les N ». Sans tâche : « Rien d'autre à traiter. » et l'heure du dernier passage d'App Store Connect et de Contenu. Puis **Ajouter** à la main (un texte, une échéance facultative, rattaché au business affiché, à aucun si « Tous » en montre plusieurs ; Entrée valide) et **Faites, 7 jours**, repliées (`l.taches_faites`, sans les messages ni les retours).

Les tâches des messages et des retours restent dans `a_traiter` : elles font le compteur ambre de la barre du haut et l'Accueil les montre. Répondre à un fil ou le fermer coche ses tâches ; traiter un retour coche la sienne, le rouvrir la décoche.

Écritures : les trois actions du SAV, seules écritures chez un service, passent par `sav_repondre`, `sav_fermer` et `sav_traiter` vers `firebase::sav::{repondre, fermer, traiter}`, écrivent dans Firestore puis dans la base ; une erreur s'affiche sous le champ ou la carte. Dans la base seulement : cocher ou décocher une tâche (`sav_cocher`), ajouter ou supprimer une tâche manuelle (`sav_ajouter`, `sav_supprimer`, les seules supprimables). Une tâche cochée l'est par sa clé : la source qui la repose ne la rouvre pas.

## Apps de potes

Où sont nos apps et celles des potes dans le top 200 de l'App Store, pays par pays. Commande `apps(business, liste, toutes, app, pays)` (`ecrans/apps.rs`), écran `apps.js`. Sous-titre : « 8 apps suivies dont 1 à nous, 4 pays, lu il y a 1 h ».

- **Top 200** : une ligne par app suivie, les nôtres d'abord (pastille « à nous ») : icône, éditeur, catégorie, note. Une colonne par pays des business affichés : le rang au dernier passage de Classements et les places gagnées depuis la veille (vert si elle monte, rouge si elle descend), « hors top » si l'app a été lue et n'y est pas. Bascules Gratuit, Payant ou Recettes, et sa catégorie (celle du business pour les nôtres) ou toutes (36). Un clic choisit l'app.
- **Détail** de l'app choisie : son rang sur 30 jours, un pays à la fois, axe inversé (1er en haut, 5 places de marge, graduations rondes), un jour hors top coupe la ligne ; la grille pays par liste. « Ne plus suivre » pour une app de pote seulement (`apps_ne_plus_suivre`, ses rangs passés restent en base).
- **Ajouter** : un id ou un lien App Store (`apps_suivre`). La fiche est lue sur iTunes tout de suite (`classements::ajouter_app`), les rangs arrivent au prochain passage de Classements ou en relançant la collecte. Le résultat, ou l'erreur en ambre, reste sous le champ.

Lit `app` et `rang` (`l.apps()`, `l.rang_jours`) et les icônes du cache (`icone:<id>`). L'ajout d'une app est le seul accès réseau de l'écran (sa fiche et son icône).
