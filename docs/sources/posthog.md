# PostHog

Fichiers : `crates/noyau/src/sources/posthog.rs` (requêtes, passage) et `crates/noyau/src/sources/posthog/decodage.rs` (décodage, écriture). Tests : `crates/noyau/src/sources/posthog/tests.rs`. Cadence : 15 min. Clés : `POSTHOG_PERSONAL_API_KEY` et `POSTHOG_PROJECT_ID` (clé personnelle limitée au projet, lecture seule), dans le trousseau ou Secret Manager ([`../collecte.md`](../collecte.md) § Secrets). Hôte : `config.posthogHost` du business.

Actifs, installs, essais, ronds commencés et finis, actifs par heure, fil des essais, détail de chaque install (tunnel, réponses, conversion, ronds, retour), portes du paywall, et les événements de l'app un par un (table `evenement`), lus en HogQL.

Les noms d'événements sont ceux de `config.evenements` du business, avec les valeurs par défaut de `Evenements` (`crates/noyau/src/business.rs`), ceux de Roleo :

| Champ | Par défaut | Rôle |
|---|---|---|
| `installation` | `onboarding_started` | le premier fait l'install |
| `etape` | `onboarding_step_completed` | étape du tunnel franchie, propriété `step` |
| `paywall` | `paywall_viewed` | paywall vu, propriété `source` |
| `essai` | `trial_started` | essai pris, propriété `product` |
| `achat` | `purchase_completed` | achat tenté, propriété `product` |
| `commence` | `path_round_started` | rond commencé, rapporté au rond fini |
| `usage` | `path_round_done` | l'usage qui compte (le rond fini), base des retours JN |
| `detail` | 23 noms (`Evenements::default`) | les événements gardés un par un dans `evenement`, avec leurs propriétés ; une liste vide coupe cette lecture |

Restent propres à Roleo, en dur dans le code : la liste des écrans du tunnel (`ECRANS`, où `paywall_viewed` sert de clé quel que soit le nom de l'événement `paywall`), les propriétés de réponse des dimensions, `RONDS_DEPUIS` et les noms des portes (`NOMS_PORTES`).

## L'appel

```
POST {posthogHost}/api/projects/{POSTHOG_PROJECT_ID}/query/
Authorization: Bearer {POSTHOG_PERSONAL_API_KEY}
{"query": {"kind": "HogQLQuery", "query": "select …"}}
```

Réponse : `results` (lignes, colonnes dans l'ordre du select), `hasMore`, `columns`. **HogQL coupe à 100 lignes par défaut** : chaque requête porte sa `limit`, et une réponse avec `hasMore = true` fait échouer le passage (« HogQL coupé par sa limit ») plutôt que d'écrire un chiffre tronqué. Une réponse sans `results` aussi (`decodage::lignes`).

Le projet PostHog est en UTC : tout découpage en jour ou en heure passe par `toTimeZone(timestamp, 'Europe/Paris')`. Les bornes sont envoyées en secondes Unix (`fromUnixTimestamp`), calculées avec `minuit` (minuit à Paris).

## Définitions

- **Bruit** (`BRUIT`) : `$identify`, `$set`, `$feature_flag_called`, `$opt_in`, `$screen_view_exit`, envoyés sans que l'user ouvre l'app. Exclus des actifs.
- **Actif** : personne distincte avec au moins un événement hors bruit dans le jour de Paris.
- **Install** : premier événement `installation` d'une personne, dont l'`$os` est `iOS`, `iPadOS` ou `Android`, dans les séries comme dans le détail par install (`installes`). La dimension Plateforme (`$os` du premier événement `installation`) les sépare dans l'arbre.
- **Store** : l'`$os` de l'événement (`filtre`). `iOS` et `iPadOS` : App Store ; `Android` : Play Store. Un actif qui passe d'un OS à l'autre le même jour compte dans les deux stores, une fois au total.
- **Essai** : un événement `essai`.
- **Achat direct** : un événement `achat` sans `essai` chez la même personne. Chez Roleo, `purchase_completed` part aussi au début d'un essai gratuit, et c'est une tentative : le payé vient de RevenueCat (`paiement`).
- **Rond fini** : un événement `usage`. Fiable seulement pour les installs depuis `RONDS_DEPUIS` (15 oct. 2026). Rond fini, retours JN et cohortes ne comptent que ces installs.
- **Rond commencé** : un événement `commence`, même porte `RONDS_DEPUIS`. Par install, un rond fini vaut aussi commencé : un début perdu ne fait jamais finir plus d'installs qu'il n'en a commencé. Par jour, ce sont des événements, pas des personnes.
- **`_j7`** : le même compte 7 jours plus tôt, coupé à la même heure de Paris (`semaine_avant`, heure d'été comprise). Un jour fini se compare à un jour fini.
- **Retour JN** (N = 1, 7, 14, 30) : un événement `usage` entre `install + N jours` et `install + N + 1 jours`, à la seconde près.

## Les requêtes

| Requête | Ce qu'elle rend | Fenêtre | `limit` |
|---|---|---|---|
| `requete_jours` | jour, puis pour chaque segment (total, App Store, Play Store, `SEGMENTS`) : actifs, essais, ronds finis, ronds commencés, chacun en entier puis avant la coupe | les jours écrits, plus les 7 jours d'avant (pour `_j7`) | 100 |
| `requete_installs` | jour, puis pour chaque segment : installs, installs avant la coupe | idem | 100 |
| `requete_heures` | jour, heure, actifs de chaque segment | les jours d'`actifs_h` | 1 000 |
| `requete_essais` | uuid, instant Unix, `product`, `source`, `$geoip_country_code` | 48 h glissantes | 200 |
| `requete_par_install` | une ligne par install : jour, écran le plus loin, paywall vu, essai, achat direct, rond commencé, rond fini, retours J1 à J30, id client, `person_id`, ronds commencés, ronds finis, jours avec un rond fini, secondes jusqu'au premier rond (-1 sans), jour du dernier rond, une colonne par dimension | installs des `JOURS_INSTALLS` (97) derniers jours de Paris | 50 000 |
| `requete_portes` | jour, porte (`source`), paywalls vus, essais, achats directs | idem | 10 000 |
| `requete_evenements` | une ligne par événement `detail` : uuid, `person_id`, instant, arrivée chez PostHog (ms), nom, `$os`, `$app_build`, jour de Paris, propriétés hors `$…` | arrivés depuis le curseur, moins une heure | 10 000 par page |

Les colonnes « avant la coupe » ne servent qu'aux jours d'il y a 7 jours et plus : le `_j7` du jour J est la colonne coupée du jour J−7.

L'écran le plus loin est le rang maximal, dans `ECRANS`, parmi les événements `etape` de l'install (propriété `step`) et `paywall`, 0 s'il n'en a aucun. Une étape compte ceux qui l'ont atteinte **ou dépassée** : un écran sauté ne casse pas la courbe. L'id client est le `distinct_id` du dernier `essai` ou `achat`, rempli seulement s'il a converti : c'est l'`app_user_id` RevenueCat, lu ensuite par la source RevenueCat.

Les dimensions (`Dimension`, `crates/noyau/src/dimension.rs`) valent un texte vide quand l'install n'a pas répondu. Une question posée après le début des 97 jours a sa date (`mesuree_depuis`) : les installs d'avant sont « Pas mesuré » (`?`), pas « Sans réponse ».

| Dimension | D'où elle vient (`valeur`) |
|---|---|
| Écran atteint (`loin`) | le rang calculé ci-dessus |
| Langue apprise (`langue`) | `language` de l'étape `language` |
| Langue maternelle (`maternelle`) | `native_language` de l'étape `native_language`, depuis le 13 oct. 2026 |
| Connu par (`source`) | `source` de l'étape `acquisition_source` |
| Âge (`age`) | `age_range` de l'étape `age`, depuis le 24 oct. 2026 |
| Motivation (`motivation`) | `motivation` de l'étape `motivation`, depuis le 16 sept. 2026 |
| Ce qui bloque (`blocage`) | `pains` de l'étape `pain` (plusieurs, séparés par des virgules), sinon l'ancien `pain` |
| Centres d'intérêt (`interet`) | `interests` de l'étape `interests` (plusieurs), depuis le 13 oct. 2026 |
| Niveau de départ (`niveau`) | `start_level` des étapes `level` ou `projection`, depuis le 13 oct. 2026 |
| Tuteur choisi (`tuteur`) | `character` de l'étape `persona`, depuis le 2 sept. 2026 |
| Notifications (`notifs`) | `notifs` de l'étape `reminder`, depuis le 9 nov. 2026 |
| Compte lié (`compte`) | `method` de l'étape `account_link` |
| Pays (`pays`) | `$geoip_country_code` du premier événement `installation` |
| Version de l'app (`version`) | `$app_version` du premier événement `installation` |
| Build (`build`) | `$app_build` du premier événement `installation` |
| Plateforme (`plateforme`) | `$os` du premier événement `installation` |
| Offre prise (`produit`) | `product` du premier `essai` ou `achat` |
| Zone (`zone`) | déduite du pays au décodage (`Dimension::zone`) : régions de l'ONU en dur, Amérique du Nord et latine séparées, l'outre-mer français à part |

Une réponse d'étape est la dernière donnée (`argMaxIf`) : un install revenu en arrière garde sa réponse finale.

`requete_portes` compte par jour et par `source` (la porte du paywall dans l'app, `onboarding` pour la fin du tunnel, vide pour les versions qui ne l'envoyaient pas) les paywalls vus, les essais et les achats directs (`achat` moins `essai`, jamais négatif).

Les 17 écrans de `ECRANS`, dans l'ordre : `welcome`, `language`, `native_language`, `acquisition_source`, `first_name`, `age`, `motivation`, `pain`, `interests`, `level`, `social_proof`, `app_tour`, `projection`, `account_setup`, `persona`, `paywall_viewed` (Paywall vu), `paywall` (Essai touché : l'étape validée, pas l'essai confirmé par Apple). Un écran ajouté au tunnel de l'app s'ajoute à `ECRANS`.

## Ce que la source fait

Un passage par business qui a un `posthogHost`, l'un après l'autre. Aucun : `NonConfigure` « aucun business n'a d'hôte PostHog ». Une erreur porte le nom du business ; hors `NonConfigure`, elle devient `Commande("<business> : <erreur>")` et perd son type, comme dans la version Swift.

1. **Historique ou pas** : si la série `actifs` d'hier ou de J-90 manque pour l'un des trois segments (`historique_incomplet`), le passage lit `JOURS_HISTORIQUE` (91) jours de séries, aujourd'hui compris, et 9 jours d'`actifs_h`. Sinon, aujourd'hui et hier seulement.
2. Les quatre requêtes de chaque passage, à la suite : jours, installs, heures, essais.
3. **Installs et portes au plus une fois par heure** : le réglage `posthog.horaire.<business>` garde l'instant ISO du dernier calcul. S'il manque ou a une heure ou plus, `requete_par_install` et `requete_portes` partent.
4. Tout s'écrit dans un seul `lot`, après les appels réseau (`decodage::ecrire`) : un passage qui échoue n'écrit rien. Le réglage est posé dans le même `lot`.
5. **Événements**, si `detail` n'est pas vide : voir § Événements.

## Événements

La table `evenement` se remplit par pages, dans l'ordre d'arrivée chez PostHog (`created_at`, puis `uuid`), pas dans l'ordre des instants : un téléphone hors ligne envoie ses événements en retard, et l'arrivée ne recule jamais.

- **Curseur** : le réglage `posthog.evenements.<business>`, millisecondes Unix de la dernière arrivée lue. Absent (premier passage), il part de minuit à Paris il y a `JOURS_INSTALLS` (97) jours, et `posthog.evenements.depuis.<business>` garde ce jour : l'écran Data ne lit rien avant.
- **Pages** : au plus `PAGES_EVENEMENTS` (30) pages de `PAGE_EVENEMENTS` (10 000) événements par passage. Chaque page s'écrit avec le curseur dans son propre `lot`, si bien qu'un passage coupé garde les pages déjà lues. Une page incomplète finit le passage. Le premier rattrapage des 97 jours peut donc prendre plusieurs passages de 15 min.
- **Marge** : chaque passage relit la dernière heure d'arrivées (`MARGE_EVENEMENTS_MS`). Un uuid déjà écrit est remplacé, pas doublé.
- **Borne** : `timestamp` doit être dans les 30 jours avant le curseur (`RETARD_MAX_JOURS`), pour que PostHog lise sur sa clé de tri. Un événement arrivé plus de 30 jours après son instant est perdu.
- **Une page pleine à la même milliseconde** ne bloque pas le curseur : la page suivante part une milliseconde plus loin.

La source rend le nombre de lignes écrites : séries et heures (toujours comptées), lignes du fil nouvelles seulement, installs et portes.

## Ce qui est écrit

**`serie`**, `heure = -1`, pour chaque jour lu : `actifs`, `actifs_j7`, `installs`, `installs_j7`, `essais`, `essais_j7`, `ronds`, `ronds_j7`, `ronds_commences`, `ronds_commences_j7`, chacune au total puis suffixée par store. Un jour sans ligne HogQL vaut 0, jamais un trou, sauf `ronds` et `ronds_commences` avant `RONDS_DEPUIS`, et leur `_j7` dont le jour d'il y a 7 jours est avant : pas écrits. Le jour en cours se réécrit à chaque passage.

**`serie(actifs_h)`**, `heure` 0 à 23, au total et par store : les heures passées des jours lus, jusqu'à l'heure en cours pour aujourd'hui. Les heures sans ligne valent 0.

**`install`** : une ligne par install des 97 derniers jours de Paris, réponses en JSON dans `valeurs`. Les jours à partir du début de la fenêtre sont effacés puis réécrits à chaque calcul (`Store::installs`) : un install qui convertit ou revient change sa ligne. Un retour JN est écrit tel que PostHog le voit ; c'est la lecture qui ne le compte que pour les jours d'install qui ont eu tout leur temps.

**`porte`** : une ligne par jour et par porte, même fenêtre, même remplacement (`Store::portes`).

**Le fil `ESSAI`**, un par événement `essai` des 48 dernières heures (`decodage::fil`) :

| Champ | Valeur |
|---|---|
| `id` | `posthog:essai:<uuid de l'événement>` |
| `at` | l'instant de l'événement |
| `texte` | `Essai annuel, Espagne, rond verrouillé` (`texte_essai`) |

L'offre vient de `product` (contient `year` ou `annual` : annuel, `quarter` : trimestriel, `month` : mensuel, `week` : hebdo, sinon « Essai » seul). Le pays vient de `$geoip_country_code`, nommé en français (`crates/noyau/src/sources/pays_fr.rs`). La porte n'est ajoutée que si l'essai ne vient pas du tunnel (`onboarding`), traduite par `NOMS_PORTES`, brute si inconnue. Une ligne déjà dans le fil n'est pas réécrite.

Pas de fil pour les installs ni pour les achats : les achats viennent de RevenueCat.

## Coût

Par passage de 15 min : 4 requêtes HogQL à la suite, 6 quand les installs et les portes sont dus (une fois par heure), plus une page d'événements au moins (30 au plus, pendant le rattrapage), soit environ 22 requêtes par heure et par business en régime normal. PostHog limite le débit et la concurrence des requêtes `/query/` par projet : un autre outil qui interroge le même projet partage ce budget.

## Ce qui peut casser

- **Un événement ou une propriété renommés dans l'app** : les événements de `config.evenements`, la propriété `step`, `product`, `source`, `$geoip_country_code`, `$os`, `$app_version`, `$app_build`, et chaque propriété de réponse du tableau des dimensions. Rien ne casse : les chiffres tombent à 0, ou une dimension passe à « Sans réponse », sans erreur.
- **Une requête qui dépasse sa `limit`** : le passage échoue avec « HogQL coupé par sa limit ». Plus de 100 jours lus, plus de 200 essais en 48 h, plus de 50 000 installs en 97 jours ou plus de 10 000 lignes de portes. Les événements n'ont pas ce risque : ils se lisent par pages.
- **La clé** : révoquée ou sans le scope du projet, HTTP 401 ou 403, le passage échoue avec le nom du business.
- **Limite de débit ou panne PostHog** (429, 5xx) : `Http` rejoue, puis erreur.
- Les vieilles lignes d'`actifs_h` ne sont jamais purgées : une ligne par heure et par jour, négligeable.
- `evenement` non plus : environ 5 Mo par jour en novembre 2026 (150 Mo par mois), à surveiller si l'audience décolle.
- **Une propriété renommée dans l'app** (`elapsed_s`, `tutor_ms`, `replies`, `rights`, `helps`, `voice_replies`, `said`, `forms`, `how`, `answered`, `missed`, `answer`) : la carte de l'écran Data tombe à vide, sans erreur.

## Différences avec la version Swift

- **Historique** : la version Swift regardait la série `actifs` du total ; ici, un trou dans l'un des trois segments (total, App Store, Play Store) relit les 91 jours.
- **Zone** : la version Swift la tirait de `Locale.Region` ; ici, une table de régions de l'ONU en dur dans `dimension.rs`.
- **Android, ronds par install et table `evenement`** : propres à cette version. La version Swift ignore les colonnes et la table ajoutées.
- **Noms** : `PostHog.ecrans`, `PostHog.rondsDepuis`, `PostHog.portes`, `PostHog.joursInstalls` deviennent `ECRANS`, `RONDS_DEPUIS`, `NOMS_PORTES`, `JOURS_INSTALLS` ; `requeteJours` devient `requete_jours`, et ainsi de suite.
