# La CLI `cockpit`

Fichier : `crates/cli/src/main.rs` (package `cockpit-cli`, binaire `cockpit`, ne dépend que de `noyau`). Tests dans le même fichier : `options_retirees`, `ligne_coupee`, `cellules_comme_swift`, `ages_courts`.

Ce que l'app sait, en quelques lignes, pour un agent qui compte ses tokens : au lieu de lire la base à la main ou de capturer l'écran, l'agent lance `cockpit veille` et reçoit 15 lignes. Elle lit la base de l'app (`noyau::chemins::base()`, [`runbook.md`](runbook.md) § Où sont les fichiers) par les mêmes `Lectures` que les écrans, sur tous les business ; l'app peut tourner en même temps (WAL).

## Installer

```bash
cargo install --path crates/cli      # cockpit dans ~/.cargo/bin (%USERPROFILE%\.cargo\bin sous Windows)
cargo run -p cockpit-cli -- etat     # sans installer
```

À refaire après un changement du cœur : le binaire installé garde l'ancien `noyau`. La sortie est en UTF-8, sans couleur ANSI ; la console Windows l'affiche telle quelle.

## Commandes

| Commande | Sortie |
|---|---|
| `cockpit etat` | Le dernier passage de chaque source (état, âge, erreur coupée à 80 caractères), « veille : N mots actifs, N vidéos sur 30 j, N outliers », le nombre d'analyses Claude et leur dépense sur le plafond de 50 $ (`PLAFOND_VEILLE_USD`), les 8 derniers événements du fil. |
| `cockpit veille [n] [--tout] [--jours j]` | Les outliers des 30 derniers jours (`--jours` pour une autre fenêtre), plus gros ratio d'abord, 15 par défaut : id, compte, plateforme, vues, ratio à la médiane, vues par heure, âge, hook s'il est analysé. `--tout` montre aussi les non-outliers. « … N de plus » s'il en reste. |
| `cockpit veille <id>` | Une vidéo : lien, médiane du compte, abonnés, mot, compteurs, ratio, pull, vitesse, légende (200 caractères), toute l'analyse Claude. Cherchée dans la fenêtre, puis sur 3 650 jours. |
| `cockpit veille hooks` | Les types de hook des vidéos analysées des 30 derniers jours, du plus fréquent au moins fréquent (`Lectures::hooks_veille`). |
| `cockpit veille cherche <texte>` | Les vidéos de la fenêtre dont la légende, le compte, le mot, le hook ou l'idée contient le texte (casse indifférente), 20 au plus ; « rien » sinon. |
| `cockpit veille mots` | Les mots-clés : business, mot, seuil de vues, en pause ou non, dernière recherche. |
| `cockpit veille mot ajoute <mot> [seuil] [--business id]` | Ajoute un mot (`Store::ajouter_mot`), ou le réactive avec ce seuil s'il existe. Le seuil est le dernier mot s'il est un entier, 20 000 vues par défaut ; le business, le premier par défaut. Cherché au prochain passage de la Veille. |
| `cockpit veille mot retire <mot> [--business id]` | Retire un mot (`Store::retirer_mot`) ; ses vidéos restent. |
| `cockpit videos [n]` | Nos vidéos, plus récentes d'abord, 10 par défaut : compte, âge, vues, gain sur 24 h, part de partages et d'enregistrements, ✓ au-dessus du seuil, titre. |
| `cockpit sql "<select>"` | Une requête `select` ou `with`, un seul énoncé (un `;` n'est admis qu'en dernier caractère), sur une connexion en lecture seule (`Database::ouvrir(chemin, true)`). Colonnes par ordre alphabétique, cellules coupées à 60 caractères, 50 lignes au plus, « 0 ligne » si rien. Les tables sont dans [`donnees.md`](donnees.md). |

Sans argument, la CLI affiche l'aide. Une commande inconnue affiche l'aide avec le code 1.

**Écritures** : les mots-clés de la veille, par `Store` comme le fera l'écran Veille. Mais chaque commande, `sql` compris, ouvre d'abord la base par `Store::ouvrir()`, comme l'app : schéma créé ou complété, graine appliquée si le fichier a changé ([`collecte.md`](collecte.md) § Graine), passages de `collecte` de plus de 30 jours effacés.

**Sorties** : nombres compacts (`25,4 k`, `lecture::format`), âges courts (`40 min`, `5 h`, `3 j`), textes coupés à une ligne avec `…`. Deux exceptions au format français : la dépense de `etat` s'écrit avec un point (`1.234 $`), et `sql` écrit les réels comme la version Swift (`3.0`, `0.5`, `16 bytes` pour un blob). Une erreur sort sur stderr, « cockpit : … », avec le code 1.

## Limite connue

`cockpit veille <id>` ne marche que pour un id non numérique (`instagram:<code>`). Un id TikTok est un nombre (`Plateforme::id_video`) : la CLI le prend pour le `n` de `cockpit veille [n]`. Hérité de la version Swift (`Int(id) == nil`). En attendant : `cockpit sql "select * from veille_video where id = '<id>'"`.

## Ajouter une commande

Une fonction dans `main.rs` qui lit par `Lectures` (jamais de SQL d'écriture à la main), une branche dans `lancer`, une ligne dans `USAGE`, une ligne ici.
