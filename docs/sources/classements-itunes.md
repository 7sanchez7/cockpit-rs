# Classements App Store (iTunes)

Fichier : `crates/noyau/src/sources/classements.rs`. id `classements`, cadence 3 h. Aucune clé. Tests dans `classements.rs` : `listes_de_view_top`, `view_top_sans_listes`, `flux_rss_lu`, `lookup_lu`, `fil_des_rangs`, `id_app_lu`, `places`.

Où sont nos apps et celles des potes dans le top 200 de l'App Store, pays par pays, et la note de nos apps. Apple n'a pas d'API officielle des classements ; on lit l'ancien endpoint de la boutique iTunes, le seul qui va jusqu'à 200.

## Le top 200 : `viewTop`

```
GET https://itunes.apple.com/WebObjects/MZStore.woa/wa/viewTop?genreId=6017&popId=27
X-Apple-Store-Front: 143442,32
```

- `genreId` : la catégorie. `6017` = Éducation, `36` = toutes catégories (`TOUTES_CATEGORIES`). Les noms français sont dans `genre()`, une catégorie inconnue s'écrit « catégorie <n> » (`nom_genre`).
- `popId=27` : classements iPhone.
- `X-Apple-Store-Front` : `<storefront>,32`, sans suffixe de langue. La version macOS a vérifié qu'un suffixe (`143454-1,32`) rend une page vide, et que `?cc=` sans l'en-tête rend une page sans données.

| Pays | Code | Storefront (`storefront`) |
|---|---|---|
| France | `fr` | 143442 |
| Espagne | `es` | 143454 |
| Belgique | `be` | 143446 |
| États-Unis | `us` | 143441 |
| Royaume-Uni | `gb` | 143444 |
| Allemagne | `de` | 143443 |
| Italie | `it` | 143450 |
| Suisse | `ch` | 143459 |
| Canada | `ca` | 143455 |

Un pays de business absent de ce tableau coupe la source (`non_configure`) : l'ajouter à `storefront` et à `nom_pays`.

### Lire les listes

La page HTML porte ses données dans `<script>its.serverData={…}</script>`. `listes` lit `pageData.topChartsPageData.topCharts` : trois listes, chacune `{"adamIds": […], "shortTitle", "id", "title"}`. `adamIds` = ids App Store dans l'ordre du classement ; on garde les 200 premiers (`TAILLE`). `selectedChart`, le doublon d'une des listes, est ignoré.

**Une liste s'identifie par son `id`, pas par son titre** (`liste_pop`) : l'`id` est le `popId`, le même partout, le titre est traduit par pays.

| Liste (`Liste`) | `id` | Flux RSS (`flux_rss`) |
|---|---|---|
| `gratuit` | 27 | `topfreeapplications` |
| `payant` | 30 | `toppaidapplications` |
| `recettes` | 38 | `topgrossingapplications` |

S'il manque `its.serverData`, le chemin JSON ou une des trois listes, `listes` lève une erreur de format : repli RSS.

## Repli : flux RSS, top 100

```
GET https://itunes.apple.com/{pays}/rss/{flux}/limit=100/genre=6017/json
```

Le flux plafonne à 100. Les ids sont dans `feed.entry[].id.attributes["im:id"]` ; une seule entrée arrive en objet, pas en tableau (`liste_rss`). Trois appels par pays et genre, 0,5 s avant chacun.

Les rangs lus en RSS s'écrivent dans `rang`, mais ne font aucune ligne du fil, et le passage finit en erreur après l'écriture : « viewTop sans listes, repli RSS top 100 pour Éducation France, … ». Le collecteur le rejoue au bout de 10 min.

## Détails d'une app : `lookup`

```
GET https://itunes.apple.com/lookup?id=<id>,<id>&country=fr
```

100 ids au plus par appel. `applis` lit `trackName` (nom), `artistName` (éditeur), `artworkUrl512` (icône, `artworkUrl100` à défaut), `primaryGenreId`, `averageUserRating` et `userRatingCount` (pour ce pays), `version`. Un résultat sans `trackId` ou sans `trackName` est ignoré.

Nos apps se lisent dans le premier pays de leur business (le pays principal s'il n'en a pas), celles des potes dans le **pays principal** (`pays_principal` : le premier pays du premier business qui en a).

## Ce que la source fait

Un seul passage global, pas un par business :

1. Pays = l'union des `pays` de tous les business ; aucun : `non_configure`, « aucun business n'a de pays ». Apps = l'`app_store_id` de chaque business, plus les apps suivies sans business (`Store::apps_suivies`, les potes) ; aucune : « aucune app à suivre ».
2. `lookup` de toutes ces apps. Une de nos apps absente du lookup fait échouer le passage, avec le nom du business. Les icônes absentes du cache y sont téléchargées (clé `icone:<id>`, `classements::icones` et `miniatures::icone`) : l'interface les lit sans réseau. Une icône en échec fait finir le passage en erreur, après l'écriture.
3. Genres = les `config.genre` des business (6017 par défaut), les genres rendus par le lookup et ceux des potes, plus 36. Pour chaque pays et genre, un `viewTop`, 0,5 s de pause entre deux appels (`PAUSE`).
4. Tout s'écrit dans un seul `store.lot`, après les appels réseau :
   - `Store::app` pour chaque app lue ; nos apps avec leur `business`, et `suivie = 1` (`Store::suivre`) ;
   - `note` (si l'app a une note) et `nb_notes` du jour, pour chaque business (`metrique::NOTE`, `NB_NOTES`) ;
   - `Store::rang(app, pays, genre, liste, at, position)` pour chaque app suivie et chaque liste où elle est ; toutes les lignes du passage ont le même `at` ;
   - le fil, pour **nos** apps seulement.

Appels par passage : 1 lookup par pays et par lot de 100 apps, plus un `viewTop` par pays et par genre. Chaque app de pote d'une autre catégorie ajoute un `viewTop` par pays.

### Le fil `RANG`

`classements::fil` compare chaque liste au **passage précédent**, le `max(at)` de `rang` avant le passage en cours :

| Cas | Texte (`place`, `nom_genre`, `nom_pays`) | Id |
|---|---|---|
| Entrée dans le top 200 | `188e en recettes, Éducation France (entrée)` | `classements:<app>:<pays>:<genre>:<liste>:<at>` |
| Au moins 20 places gagnées ou perdues (`SEUIL`) | `152e en gratuit, Éducation France (+24)` | idem |
| Sortie du top 200 | `Hors top 200 en gratuit, Éducation Espagne (était 199e)` | idem, avec l'`at` du passage précédent |

Lien : `https://apps.apple.com/<pays>/app/id<app>`. L'`at` de l'id est en ISO UTC. Une sortie porte l'`at` où l'app a été vue pour la dernière fois, pour qu'une même sortie ne revienne pas (le fil est en `insert or ignore`).

Pas de fil au tout premier passage (`rang` vide), ni pour une liste lue en RSS, ni quand le dernier passage de la source a fini en erreur après avoir écrit ses rangs (`passage_precedent` : un repli RSS s'arrête à 100, la comparaison inventerait des entrées). Limites :

- un pays ajouté à un business fait des lignes « entrée » à son premier passage ;
- le passage précédent est le dernier qui a écrit au moins un rang : si aucune app n'était classée nulle part, le suivant se compare à un plus ancien.

## Apps de potes

Deux fonctions publiques pour l'écran Apps :

- `classements::id_app(saisie) -> Option<String>` : l'id d'une saisie, `6770304361` ou un lien `https://apps.apple.com/…/id6770304361` (blancs autour tolérés) ;
- `classements::ajouter_app(id, store, http) -> Res<Appli>` : `lookup` dans le pays principal, puis `Store::app` et `Store::suivre(id, true)` dans un lot, puis l'icône (en échec, l'app reste suivie et l'erreur le dit). L'app n'a pas de `business`. Un id absent de ce pays lève une erreur de format. Ses rangs arrivent au passage suivant.

Retirer une app : `Store::suivre(id, false)`.

**Pas encore branché** : aucune commande Tauri ni de la CLI n'appelle `id_app` ou `ajouter_app`, et l'écran Apps n'est pas codé. Rien dans l'app ni dans la CLI ne permet encore d'ajouter un pote.

## Ce qui peut casser

- Apple change la page : plus de `its.serverData`, un autre chemin JSON, d'autres `id` de liste. La source passe au RSS et le passage finit en erreur, Santé le montre.
- Une limite de débit sur `itunes.apple.com` : sur un 429, le client HTTP rejoue jusqu'à 3 fois (`Http`, `Retry-After` respecté), puis erreur ; un 403 lève tout de suite.
- Une app retirée de la vente : absente du lookup. Pour une des nôtres, le passage échoue ; pour un pote, elle garde ses anciennes infos et n'a plus de rang.
