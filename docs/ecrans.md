# Écrans

Chaque écran a sa commande Rust dans `crates/app/src/ecrans/<id>.rs`, qui lit la base et rend tout ce qui s'affiche déjà formaté, et son module dans `crates/app/ui/js/ecrans/<id>.js`, qui n'affiche que ce qu'elle rend. Aucun écran ne fait de réseau.

## Vidéos

Toutes les vidéos des comptes suivis, sur TikTok, Instagram, YouTube et Facebook : laquelle marche, laquelle convertit, et son vrai watch time. Commande `videos` (`ecrans/videos.rs`), module `ecrans/videos.js` ; ce que l'écran partage avec la Veille est dans `ecrans/videos/commun.js` (menu du clic droit, popover d'ajout, case 9:16, choix du business).

Sous-titre : « N vidéos sur N comptes, N publiées en 7 j, dernier relevé il y a … ».

- **Comptes** (`rangeeComptes`), une rangée qui défile à l'horizontale, une carte par compte (`l.comptes()`) : avatar, @compte, plateforme (ou « en pause »), abonnés, nombre de vidéos, gain d'abonnés sur 7 jours en pastille, courbe des abonnés sur 14 jours. Un clic filtre la grille sur ce compte, un second l'enlève. Clic droit : Ouvrir le profil, Relever maintenant, Mettre en pause ou Reprendre, Ne plus suivre (`activer_compte`, `retirer_compte`).
- **Suivre un compte** : la dernière carte, « + ». Plateforme, « @compte ou lien du profil » (`handle_compte` le normalise à la frappe), business s'il y en a plusieurs ; Suivre (`suivre_compte`) écrit la ligne de `compte` par le `Store` et relance la source de la plateforme par `etat.collecteur`.
- **Toutes les vidéos** (`panneauVideos`) : tri Récentes, Vues, 24 h (gain de vues), Engagement ((likes + commentaires + partages + enregistrements) / vues), Conversion (abonnés gagnés pour 1 000 vues). Filtre par plateforme quand plusieurs sont suivies. La pastille du seuil rappelle les 1,5 % (partages ou enregistrements rapportés aux vues). Deux vues :
  - Grille (par défaut) : une case par vidéo (`caseVideo`), de 150 à 190 px de large, image 9:16, vues, logo de la plateforme, la mesure du tri en pastille. La vidéo choisie est cernée.
  - Tableau : `.table-videos` (#, titre, âge, vues, gain 24 h, parts de partages et d'enregistrements, seuil).
  Une vidéo sans la mesure du tri passe après les autres. Le titre est celui de la fiche de contenu, sinon la légende sans hashtags, sinon « @compte, date ».
- **Détail** de la vidéo choisie (au clic, sinon la première de la liste). **Disposition fixe** : d'une vidéo à l'autre de la même plateforme, chaque bloc et chaque case restent à la même place ; une valeur absente s'écrit « — », un graphique absent garde sa hauteur avec une phrase, une liste de parts garde ses 6 lignes. Le détail mène la hauteur de la rangée (`rangee([2, 1], …, { meneur: 1 })`) : la page défile, la grille défile dans cette hauteur. En haut à droite, l'état du verdict de la fiche et le seuil. Dans l'ordre : en-tête (titre sur deux lignes, liens vers la vidéo et la fiche par `ouvrir_fiche`, méta, vues et gain sur 24 h) ; **Ce qui l'a fait percer** (le levier de `percee::levier`, vert si la vidéo dépasse la médiane de son compte, rouge sinon, gris sans levier) ; **Visionnage**, 9 cases, et la note sur les stats privées ; **Engagement** ; TikTok : courbe de rétention ; vues relevées ; TikTok : vues des 7 premiers jours, puis les parts (sources de trafic et recherches, âges et pays) ; Facebook : jalons de visionnage. La case du levier est cernée de la couleur du panneau.

Lit `compte`, `compte_releve`, `video`, `video_releve`, `video_analyse`, la fiche de contenu et les miniatures du cache local (`ctx.image`), sans réseau. Les écritures de l'écran passent par le `Store` ; un agent fait la même chose dans la base (voir [runbook.md](runbook.md) § Comptes vidéo).

Écarts avec le Swift : les courbes partent de 0 (le Swift ne force pas le zéro sur les vues relevées), la rétention n'a pas d'aire sous la courbe et sa bulle est celle de `courbe`, les barres des 7 jours n'ont pas d'axe vertical, et un clic sur une vidéo relit l'écran entier.

## Veille

Les vidéos TikTok et Instagram qui ont marché sur des mots-clés, sans suivre personne (`noyau::sources::veille`). Commande `veille` (`ecrans/veille.rs`), module `ecrans/veille.js`.

Sous-titre : « N mots, N vidéos dont N outliers en 30 j, cherché il y a … ». En haut à droite, « Lancer maintenant » lance un passage sans attendre les 6 h ; pendant le passage, « Passage en cours » avec l'icône qui tourne.

- **Mots-clés** (`rangeeMots`) : une pilule par mot avec son nombre d'outliers ; grise en pause. Un clic filtre la grille, un second l'enlève. Survol : seuil et dernière recherche. Clic droit : Mettre en pause ou Reprendre (`activer_mot`), Retirer (`retirer_mot`). La pilule « + Mot-clé » ouvre l'ajout : mot, seuil de vues (20 000), business s'il y en a plusieurs ; Ajouter (`ajouter_mot`) écrit `veille_mot` par le `Store` et relance la Veille par `etat.collecteur`.
- **Ce qui a marché** (`panneauVeille`, deux tiers) : tri × médiane (par défaut), Vues, Vues/h, Récentes ; filtre Toutes, TikTok, Instagram ; Outliers (par défaut) ou Tout. Une case par vidéo (`caseVideo`), 150 à 190 px, image 9:16, vues, logo, la mesure du tri en pastille (verte pour un outlier), @compte et une étincelle si Claude l'a analysée. Clic droit : Ouvrir sur TikTok ou Instagram.
- **Détail** (un tiers, défile seul) de la vidéo choisie, sinon la première : aperçu, compte, mot, âge, durée, légende ; vues, médiane du compte, × médiane, pull, vues/h, abonnés ; puis l'analyse : type de hook, hook visuel, format, adaptable, le hook mot pour mot et le texte à l'écran, problème, les blocs, ce qui change, « À tourner ». « Voir sur TikTok » ou « Voir sur Instagram » ouvre l'originale dans le navigateur.

Lit les tables `veille_*` des 30 derniers jours et les miniatures du cache local, sans réseau. Écart avec le Swift : pas de lecteur vidéo dans le détail, qui demanderait de télécharger la vidéo depuis l'écran.
