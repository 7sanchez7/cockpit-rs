# La CLI `cockpit`

Fichiers : `crates/cli/src/main.rs`, et `crates/cli/src/analyse.rs` pour `schema` et les vues (package `cockpit-cli`, binaire `cockpit`, ne dépend que de `noyau`). Une sortie fermée en cours de route (`cockpit … | head`) termine la commande en silence, code 0 (crochet de panique dans `main`). Tests dans les mêmes fichiers : `drapeaux_retires`, `ligne_coupee`, `champs_csv`, `cellules_comme_swift`, `reels_arrondis`, `ages_courts`, `vues_valides`.

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
| `cockpit schema` | Ce qu'un agent lit avant d'écrire un `select`, environ 1 000 tokens au lieu des 8 000 du schéma : les conventions (jours à Paris, instants en UTC, JSON par `json_extract`, les business), puis une ligne par table (lignes, premier et dernier jour par `jour` ou `at`, colonnes), puis les vues `v_*` avec leur rôle et leurs colonnes. 0,6 s, surtout le compte d'`evenement`. |
| `cockpit schema <table\|vue>` | Les colonnes et leur type. `evenement` ajoute chaque événement des 14 derniers jours, son nombre et les clés de ses `props`, de la plus à la moins fréquente (1,5 s, par l'index `(business, nom, jour)`) ; `serie` et `v_jour`, chaque métrique avec ses suffixes de store ou de fournisseur, son premier et son dernier jour, à part celles par heure ; `install` et `v_install`, chaque dimension de `valeurs`, le nombre d'installs qui y ont répondu et ses 8 réponses les plus fréquentes. Une cible inconnue sort en erreur. |
| `cockpit sql "<select>" [--tout] [--csv]` | Une requête `select` ou `with`, un seul énoncé (un `;` n'est admis qu'en dernier caractère), sur une connexion en lecture seule (`Database::ouvrir(chemin, true)`, `Database::tableau`) où les vues `v_install`, `v_jour` et `v_cohorte` sont créées (§ Vues). Colonnes dans l'ordre du select, une cellule NULL vide, un réel à 4 décimales au plus (`affichee`, le CSV garde tout), « 0 ligne » si rien. Par défaut, 50 lignes au plus et cellules coupées à 60 caractères, puis « … N de plus, --tout pour tout voir ». `--tout` sort toutes les lignes en entier, retours à la ligne remplacés par des espaces. `--csv` sort tout en CSV, en-tête compris (`champ_csv` : guillemets doublés autour d'une virgule, d'un guillemet ou d'un retour à la ligne), à rediriger vers un fichier. Les tables sont dans [`donnees.md`](donnees.md), avec des exemples sur `evenement`. |

La veille n'est plus dans le Cockpit : un agent la lit par le MCP de Viral Bank ([`sources/veille.md`](sources/veille.md)).

Sans argument, la CLI affiche l'aide. Une commande inconnue affiche l'aide avec le code 1.

**Écritures** : aucune donnée. Mais chaque commande, `sql` compris, ouvre d'abord la base par `Store::ouvrir()`, comme l'app : schéma créé ou complété, graine appliquée si le fichier a changé ([`collecte.md`](collecte.md) § Graine), passages de `collecte` de plus de 30 jours effacés.

**Sorties** : nombres compacts (`25,4 k`, `lecture::format`), âges courts (`40 min`, `5 h`, `3 j`), textes coupés à une ligne avec `…`. Une exception au format français : `sql` écrit les réels comme la version Swift (`3.0`, `0.5`, `16 bytes` pour un blob), coupés à 4 décimales dans le tableau (`0.3333`). Une erreur sort sur stderr, « cockpit : … », avec le code 1.

## Vues

Créées par `analyse::base()` à chaque `cockpit sql` et `cockpit schema`, en `temp` : elles vivent sur la connexion en lecture seule et disparaissent avec elle. La base n'est jamais modifiée, son schéma reste celui de la version Swift. Elles servent les requêtes qui reviennent, sans réécrire la jointure ni le pivot ; leurs colonnes sont listées par `cockpit schema`.

| Vue | Une ligne par | Contenu |
|---|---|---|
| `v_install` | install | les colonnes d'`install` sans `valeurs`, `semaine` (le lundi de `jour`), `paiement` (l'`etat` de `paiement`, nul sans client) et `paye` (0 ou 1), puis une colonne par `Dimension` hors `loin`, ses réponses séparées par des virgules (`music,series`) |
| `v_jour` | business et jour | `serie` à `heure = -1` pivotée, une colonne par métrique de `METRIQUES_JOUR` (installs, actifs, ronds, essais, achats, abonnés, MRR, revenu, coût IA, vues TikTok, note…), total des stores ; nulle un jour sans relevé |
| `v_cohorte` | business et semaine d'install | `installs`, puis en % arrondis à 0,1 : `pct_paywall`, `pct_essai`, `pct_direct`, `pct_paye`, `pct_commence`, `pct_rond`, et `pct_j1` à `pct_j30` sur les seuls installs dont la fenêtre N + 1 jours est passée (un jour de marge), nuls si aucun ne l'est |

```sql
-- les cohortes récentes
select * from v_cohorte where business = 'roleo' order by semaine desc limit 8
-- un découpage, ici par plateforme depuis octobre
select plateforme, count(*), round(100.0 * avg(rond), 1), round(100.0 * avg(paye), 2) from v_install
where jour >= '2026-10-01' group by 1
```

Une vue de plus : sa requête dans `vues()`, son nom et son rôle dans `VUES`, une ligne ici.

## Ajouter une commande

Une fonction dans `main.rs` (ou `analyse.rs` pour une lecture de la base brute) qui lit par `Lectures` (jamais de SQL d'écriture à la main), une branche dans `lancer`, une ligne dans `USAGE`, une ligne ici.
