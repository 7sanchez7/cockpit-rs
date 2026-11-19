# RevenueCat

Fichier : `crates/noyau/src/sources/revenuecat.rs`. Tests : `crates/noyau/src/sources/revenuecat/tests.rs`. Cadence : 30 min. Clé : `REVENUECAT_SECRET_KEY_V2` (`NOM_CLE`), cherchée pour chaque business ([`../collecte.md`](../collecte.md) § Secrets).

L'argent : MRR, abonnés, essais en cours, revenu, achats du jour, fil des achats, et le paiement des installs que PostHog a vus convertir. Uniquement l'API v2 (`https://api.revenuecat.com/v2`, `Authorization: Bearer <clé>`), en lecture.

**Jamais l'API v1 `GET /subscribers/{id}`** : elle crée le client qu'elle ne trouve pas. En v2, `GET …/customers/{id}/subscriptions` rend 404 sans rien créer.

Un business est lu si sa clé se trouve ; sans clé (`NonConfigure`), il est sauté. Le fil `ACHAT` demande en plus un `firebaseProject` : il part du Firestore de l'app. Sans lui, séries et paiements sont écrits, pas le fil. Aucun business lu : `NonConfigure` « aucun business avec REVENUECAT_SECRET_KEY_V2 ». Une erreur porte le nom du business et garde son type (`nomme`).

## Le projet : `GET /projects`

```
GET /v2/projects
→ {"items":[{"id":"proj…","name":"monapp",…}]}
```

On prend le projet qui porte le nom du business (`projet`), sinon le seul de la liste. Plusieurs projets sans nom correspondant : erreur `Format`.

## Aujourd'hui : `GET /projects/{id}/metrics/overview`

| `id` | Métrique | Remarque |
|---|---|---|
| `mrr` | `mrr_brut`, et `mrr_net` = brut × 0,72 (`metrique::PART_NETTE`) | en dollars, TVA et commission du store comprises |
| `active_subscriptions` | `abonnes` | |
| `active_trials` | `essais_actifs` | |
| `revenue` (28 jours) | `revenu_28j` du jour | dollars bruts sur 28 jours glissants |

Les autres `id` sont ignorés. Un des quatre absent : erreur `Format` (`apercu`).

## L'historique : `GET /projects/{id}/charts/{nom}`

```
GET /v2/projects/{projet}/charts/mrr?resolution=0&start_date=…&end_date=…&segment=store
→ {"measures":[{"display_name":"MRR",…}], "segments":[…],
   "values":[{"cohort":…,"measure":0,"segment":0,"value":…},…]}
```

- `resolution=0` : au jour.
- `cohort` : minuit UTC du jour, en secondes. Les jours de RevenueCat sont des jours UTC : un achat à 0 h 30 à Paris compte la veille.
- `measure` : index dans `measures`. La mesure se lit **par son `display_name`** (`mesure`) ; un nom disparu fait une erreur `Format`.
- Limite de RevenueCat : 25 requêtes par minute sur les graphiques. Un passage en fait 6.

| Graphique | Mesure lue | Métrique |
|---|---|---|
| `mrr` | `MRR` | `mrr_brut`, `mrr_net` |
| `actives` | `Actives` | `abonnes` |
| `trials` | `Active Trials` | `essais_actifs` |
| `actives_new` | `Total Paid Subscriptions` moins `Product Changes` (`achats`) | `achats` |
| `customers_new` | `New Customers` | `nouveaux_clients`, aujourd'hui compris (encore partiel) |
| `revenue` | `Revenue` | `revenu`, le brut de chaque jour lu, aujourd'hui compris ; et `revenu_28j` des jours passés : somme des 28 jours finissant à chacun (`revenus28j`), lue 27 jours avant le début de la fenêtre. Un jour dont la fenêtre n'est pas complète n'est pas écrit |

Un changement de formule n'est pas un achat, d'où la soustraction dans `actives_new`.

**Par store.** Chaque graphique est demandé avec `segment=store`. Le total se lit sur le segment `is_total` ; un store, sur le segment dont le nom est `App Store` ou `iOS`, ou contient `Play` ou vaut `Android` (`boutique`), à 0 les jours où il n'apparaît pas. `customers_new` refuse `segment=store` : il est demandé avec `segment=first_platform` (segments iOS, Android, Unknown) ; Unknown ne va qu'au total, si bien que les deux stores peuvent faire un de moins que le total. L'overview ne se découpe pas : aujourd'hui, chaque store en prend la part qu'il a dans les graphiques du jour (MRR, abonnés, essais) ou des 28 derniers jours (revenu). Les métriques par store s'écrivent suffixées (`mrr_brut.play_store`) et somment au total.

**Fenêtre.** 90 jours (`HISTORIQUE`) au premier passage réussi, ou quand `revenu_28j`, `revenu` ou `nouveaux_clients`, au total ou par store, manque hier ou il y a 89 jours ; sinon les 7 derniers (`RELECTURE`), que RevenueCat consolide. Les jours passés viennent des graphiques ; aujourd'hui vient de l'overview, sauf `achats`, `revenu` et `nouveaux_clients`, qui n'existent que dans le graphique.

## Le fil `ACHAT`

La v2 n'a aucune liste des transactions d'un projet : elles se lisent abonnement par abonnement. On part des comptes que le webhook RevenueCat du backend vient de relire dans Firestore (`users/{uid}` : `premiumUntil`, `premiumTrial`, `entitlementCheckedAt`).

1. **Firestore** (jeton Google de `gcloud`, en-tête `x-goog-user-project`) : `POST …/projects/{firebaseProject}/databases/(default)/documents:runQuery` avec `requete_comptes`, les comptes dont `premiumUntil` est dans le futur. On garde ceux hors essai (`premiumTrial` différent de `true`) relus depuis le dernier passage réussi moins 2 h, ou depuis 72 h au premier passage (`PREMIERS_ACHATS`, `comptes_payes`).
2. **Abonnements du compte** : `GET /v2/projects/{p}/customers/{uid}/subscriptions?limit=50`. Un 404 (compte inconnu de RevenueCat) est sauté. On garde les abonnements `environment: "production"` commencés au plus 8 jours (`ESSAI_MAX`) avant la fenêtre, le temps d'un essai (`abonnements`).
3. **Transactions** de chaque abonnement pas encore dans le fil : `GET /v2/projects/{p}/subscriptions/{sid}/transactions?sort=purchased_at&direction=asc&limit=20`. La première avec `revenue_in_local_currency.gross > 0` est l'achat ; une transaction à 0 avant elle est l'essai. Pas encore de paiement : rien, l'abonnement est relu au passage suivant.

| Champ | Valeur |
|---|---|
| `id` | `revenuecat:achat:<id de l'abonnement>` |
| `at` | `purchased_at` de la première transaction payée |
| `texte` | `Annuel après essai, 69,99 €, France` : formule, prix dans la devise du client, pays |
| `lien` | aucun |

La formule se lit dans `product_store_identifier` (`formule`) : `week` ou `hebdo` Hebdo, `month` ou `mensuel` Mensuel, `quarter` ou `trimestr` Trimestriel, `year`, `annual` ou `annuel` Annuel, vide Abonnement, sinon l'identifiant tel quel. Le prix est formaté à la française par `prix` : espace fine entre les milliers, virgule décimale, 0 décimale pour `JPY` et `KRW`, symbole pour `EUR`, `USD` ($US), `CAD`, `GBP`, `AUD`, `MXN`, `BRL`, le code ISO sinon. Le pays est nommé par `crates/noyau/src/sources/pays_fr.rs`. Un achat trouvé en retard s'écrit quand même, à son heure ; le collecteur ne notifie que ce qui a moins de 2 h.

## Le paiement des installs convertis

PostHog dit qui a pris un essai ou acheté (`install.client`, l'`app_user_id` RevenueCat), pas qui a payé : un essai annulé ou un achat remboursé y restent. La source relit ces clients et écrit leur état dans la table `paiement` ([`../donnees.md`](../donnees.md)).

1. **Les clients à lire** (`Store::clients_a_verifier`) : ceux de `install` jamais lus, puis ceux en `essai` ou `non` lus il y a plus de 6 h (`RELECTURE_PAIEMENT`) et installés dans les 40 derniers jours (`SUIVI_PAIEMENT`). Les plus récents d'abord, 500 au plus par passage (`PAIEMENTS_PAR_PASSAGE`).
2. **`GET /v2/projects/{p}/customers/{id}/subscriptions?limit=50`**, l'id encodé (`encoder_client` : lettres, chiffres, `-` et `_` gardés, le reste en `%XX`).
3. **L'état** (`paiement`), sur les seuls abonnements de production : `paye` dès que l'un a `total_revenue_in_usd.gross > 0`, sinon `essai` si l'un est `trialing`, sinon `non`. `inconnu` sans abonnement de production (testeur en sandbox) ou sur un 404 (compte anonyme non lié).

Un `paye` ou un `inconnu` n'est plus relu ; un `essai` ou un `non` l'est toutes les 6 h tant que l'install a moins de 40 jours.

## Ce que la source fait

Pour chaque business, à la suite : `projects`, `overview`, les 6 graphiques, puis les séries dans un `lot` (`ecrire`) ; ensuite Firestore et le fil ; enfin les paiements. Appels : 8 vers RevenueCat, 1 vers Firestore, puis 1 par compte relu, 1 par abonnement neuf et 1 par client converti à lire. La source rend le nombre de points écrits, plus les achats nouveaux et les paiements lus.

## Ce qui peut casser

- RevenueCat renomme une mesure (`display_name`) ou un `id` de l'overview : erreur `Format`.
- Le webhook de l'app arrête de relire les comptes, ou les champs `premiumUntil`, `premiumTrial`, `entitlementCheckedAt` changent de nom : le fil ACHAT se tait sans erreur, `achats` (graphique) reste juste.
- Un achat sur un compte dont l'uid n'est pas l'`app_user_id` RevenueCat : 404, absent du fil.
- Un client converti dont le `distinct_id` PostHog n'est pas l'`app_user_id` : 404, `inconnu`, jamais payant.
- Beaucoup de comptes relus par passage : la limite de débit rend des 429, `Http` rejoue 3 fois.
- `mrr` est en dollars et brut ; `mrr_net` applique 72 %, une moyenne (TVA et commission varient par pays).

## Différences avec la version Swift

- **Prix** : la version Swift passait par `formatted(.currency(code:).locale(fr_FR))`. Ici, une table de symboles en dur : une devise hors de la table s'affiche avec son code ISO (`CHF`, `INR`), et les décimales ne sont à 0 que pour `JPY` et `KRW`.
- **Secrets** : une erreur de lecture de la clé autre que `NonConfigure` porte ici le nom du business ; la version Swift la remontait telle quelle.
- Le reste suit la version Swift : mêmes appels, mêmes fenêtres, mêmes mesures, mêmes ids du fil.
