# La CLI `cockpit`

Fichier : `crates/cli/src/main.rs` (package `cockpit-cli`, binaire `cockpit`, ne dépend que de `noyau`). Tests dans le même fichier : `drapeaux_retires`, `ligne_coupee`, `champs_csv`, `cellules_comme_swift`, `ages_courts`.

Ce que l'app sait, en quelques lignes, pour un agent qui compte ses tokens : au lieu de lire la base à la main ou de capturer l'écran, l'agent lance `cockpit etat` et reçoit une vingtaine de lignes. Elle lit la base de l'app (`noyau::chemins::base()`, [`runbook.md`](runbook.md) § Où sont les fichiers) par les mêmes `Lectures` que les écrans, sur tous les business ; l'app peut tourner en même temps (WAL).

## Installer

```bash
cargo install --path crates/cli      # cockpit dans ~/.cargo/bin (%USERPROFILE%\.cargo\bin sous Windows)
cargo run -p cockpit-cli -- etat     # sans installer
```

À refaire après un changement du cœur : le binaire installé garde l'ancien `noyau`. La sortie est en UTF-8, sans couleur ANSI ; la console Windows l'affiche telle quelle.

## Commandes

| Commande | Sortie |
|---|---|
| `cockpit etat` | Le dernier passage de chaque source (état, âge, erreur coupée à 80 caractères), les 8 derniers événements du fil. |
| `cockpit videos [n]` | Nos vidéos, plus récentes d'abord, 10 par défaut : compte, âge, vues, gain sur 24 h, part de partages et d'enregistrements, ✓ au-dessus du seuil, titre. |
| `cockpit sql "<select>" [--tout] [--csv]` | Une requête `select` ou `with`, un seul énoncé (un `;` n'est admis qu'en dernier caractère), sur une connexion en lecture seule (`Database::ouvrir(chemin, true)`, `Database::tableau`). Colonnes dans l'ordre du select, une cellule NULL vide, « 0 ligne » si rien. Par défaut, 50 lignes au plus et cellules coupées à 60 caractères, puis « … N de plus, --tout pour tout voir ». `--tout` sort toutes les lignes en entier, retours à la ligne remplacés par des espaces. `--csv` sort tout en CSV, en-tête compris (`champ_csv` : guillemets doublés autour d'une virgule, d'un guillemet ou d'un retour à la ligne), à rediriger vers un fichier. Les tables sont dans [`donnees.md`](donnees.md), avec des exemples sur `evenement`. |

La veille n'est plus dans le Cockpit : un agent la lit par le MCP de Viral Bank ([`sources/veille.md`](sources/veille.md)).

Sans argument, la CLI affiche l'aide. Une commande inconnue affiche l'aide avec le code 1.

**Écritures** : aucune donnée. Mais chaque commande, `sql` compris, ouvre d'abord la base par `Store::ouvrir()`, comme l'app : schéma créé ou complété, graine appliquée si le fichier a changé ([`collecte.md`](collecte.md) § Graine), passages de `collecte` de plus de 30 jours effacés.

**Sorties** : nombres compacts (`25,4 k`, `lecture::format`), âges courts (`40 min`, `5 h`, `3 j`), textes coupés à une ligne avec `…`. Une exception au format français : `sql` écrit les réels comme la version Swift (`3.0`, `0.5`, `16 bytes` pour un blob). Une erreur sort sur stderr, « cockpit : … », avec le code 1.

## Ajouter une commande

Une fonction dans `main.rs` qui lit par `Lectures` (jamais de SQL d'écriture à la main), une branche dans `lancer`, une ligne dans `USAGE`, une ligne ici.
