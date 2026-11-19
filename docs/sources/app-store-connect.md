# App Store Connect

Fichier : `crates/noyau/src/sources/app_store_connect.rs`. Tests : `crates/noyau/src/sources/app_store_connect/tests.rs`. Id : `asc`. Cadence : 1 h.

Les avis App Store, les tâches « répondre à l'avis », les changements d'état des versions et les téléchargements par jour, par l'API officielle (`https://api.appstoreconnect.apple.com/v1`), en lecture. La note et le nombre de notes ne viennent pas d'ici mais de Classements ([`classements-itunes.md`](classements-itunes.md), pas encore portée).

Un business est lu s'il a `appStoreId` et `ascIssuer` dans sa config. Aucun : `NonConfigure` « aucun business avec appStoreId et ascIssuer ». Une erreur porte le nom du business et garde son type (`revenuecat::nomme`).

## La clé

| | Mac | Windows |
|---|---|---|
| dossier | `~/.appstoreconnect/private_keys/` | `%USERPROFILE%\.appstoreconnect\private_keys\` |
| fichier | `AuthKey_<KEYID>.p8` | idem |

La clé est le premier fichier `AuthKey_*.p8` du dossier, par ordre alphabétique (`cle`). `KEYID` vient du nom du fichier. Elle ne sort jamais du dossier : ni dans le dépôt, ni dans la base, ni dans un journal. Pas de clé ou dossier absent : `NonConfigure`. Clé qui n'est pas une clé EC en PEM : `NonConfigure` « clé AuthKey_….p8 illisible », sans rien en citer.

`ascIssuer` est l'issuer de la clé (App Store Connect, Utilisateurs et accès, Intégrations). Ce n'est pas un secret : il vit dans la config du business.

## Le jeton

Un JWT ES256 refait à chaque business et à chaque passage (`jeton`, signé par `crates/noyau/src/collecte/jwt.rs`) :

```
en-tête  {"alg":"ES256","kid":"<KEYID>","typ":"JWT"}
charge   {"iss":"<ascIssuer>","iat":<maintenant>,"exp":<iat + 19 min>,"aud":"appstoreconnect-v1"}
```

Apple refuse un `exp` à plus de 20 min de `iat`.

## Les avis : `customerReviews`

```
GET /v1/apps/{appStoreId}/customerReviews?sort=-createdDate&limit=200&include=response
→ {"data":[{"id":"…","attributes":{"rating":5,"title":"…","body":"…","reviewerNickname":"…",
            "createdDate":"2026-11-11T00:18:14-08:00","territory":"FRA"},
           "relationships":{"response":{"data":null}}}], …}
```

- `createdDate` porte un décalage (`-08:00`), pas un `Z`.
- `territory` est le code pays à 3 lettres, gardé tel quel dans `avis.pays`.
- `include=response` remplit `relationships.response.data` : nul = pas de réponse. Sans `include`, tout semblerait sans réponse.
- 200 avis tiennent sur une page ; au-delà, les plus anciens ne sont plus relus (ils restent dans la base).

Tous les avis s'écrivent dans `avis` (une réponse ajoutée depuis met `repondu` à 1).

### Les tâches

Une par avis de **3 étoiles ou moins sans réponse** (`taches`), via `Store::synchroniser_taches("asc", …)` : un avis répondu sort de la liste, sa tâche passe à fait.

| Champ | Valeur |
|---|---|
| clé | `avis:<id>` |
| texte | `Répondre à <pseudo>, « <titre> »` (titre coupé à 60 caractères ; sans titre, `Répondre à <pseudo>` ; sans pseudo, `Répondre à un avis`) |
| étiquette | la note, `1 ★` |
| ton | `Alerte` pour 1 ou 2 étoiles, `Neutre` pour 3 |
| lien | `https://appstoreconnect.apple.com/apps/{appStoreId}/distribution/activity/ios/ratingsResponses` |

### Le fil `AVIS`

Les avis de moins de 7 jours (`FENETRE_FIL`, `fil_avis`) : id `asc:avis:<id>`, `at` = `createdDate`, texte `★★★★☆ <titre>` (étoiles pleines et vides, puis le titre), même lien que les tâches.

## Les versions : `appStoreVersions`

```
GET /v1/apps/{appStoreId}/appStoreVersions?limit=5
→ {"data":[{"id":"…","attributes":{"platform":"IOS","versionString":"2.0.3","appStoreState":"READY_FOR_SALE",
            "createdDate":"2026-11-01T14:25:38-08:00"}}]}
```

On ne suit que la version iOS la plus récente (`createdDate`, `version`) : c'est elle qui change d'état. Une ligne `VERSION` par état, id `asc:version:<id>:<appStoreState>`, donc une nouvelle ligne à chaque changement. Texte : `2.0.4 en examen`. Lien : `https://appstoreconnect.apple.com/apps/{appStoreId}/distribution`. `at` : `createdDate` au premier passage réussi de la source, l'heure du passage ensuite.

| `appStoreState` | Texte (`etat`) |
|---|---|
| `PREPARE_FOR_SUBMISSION` | en préparation |
| `READY_FOR_REVIEW` | prête pour l'examen |
| `WAITING_FOR_REVIEW` | en attente d'examen |
| `IN_REVIEW` | en examen |
| `ACCEPTED` | acceptée |
| `PENDING_DEVELOPER_RELEASE` | validée, à publier |
| `PENDING_APPLE_RELEASE` | validée, publication par Apple |
| `PROCESSING_FOR_APP_STORE`, `PROCESSING_FOR_DISTRIBUTION` | en traitement |
| `READY_FOR_SALE`, `READY_FOR_DISTRIBUTION` | en vente |
| `REJECTED`, `METADATA_REJECTED`, `INVALID_BINARY` | refusée, métadonnées refusées, binaire invalide |
| `DEVELOPER_REJECTED` | retirée de l'examen |
| `WAITING_FOR_EXPORT_COMPLIANCE` | en attente de conformité export |
| `REMOVED_FROM_SALE`, `DEVELOPER_REMOVED_FROM_SALE` | retirée de la vente |
| `REPLACED_WITH_NEW_VERSION` | remplacée |
| autre | le code en minuscules, `_` changés en espaces |

## Les téléchargements : `salesReports`

```
GET /v1/salesReports?filter[frequency]=DAILY&filter[reportType]=SALES&filter[reportSubType]=SUMMARY
    &filter[vendorNumber]={ascVendor}&filter[reportDate]=AAAA-MM-JJ&filter[version]=1_1
Accept: application/a-gzip
```

- Il faut `ascVendor` dans la config du business, le numéro de fournisseur (App Store Connect, Paiements et rapports financiers). Il ne se lit pas par l'API et n'est pas un secret. Sans lui, l'étape est sautée, avis et versions continuent.
- La réponse est un TSV compressé en gzip (décompressé s'il commence par `1F 8B`). On garde les lignes dont `Apple Identifier` est l'`appStoreId` du business et dont `Product Type Identifier` est un premier téléchargement (`PREMIERS_TELECHARGEMENTS` : `1`, `1F`, `1T`, `F1`, `1E`, `1EP`, `1EU`), et on somme `Units` (`telechargements`). Retéléchargements (`3F`…) et mises à jour (`7F`…) sont exclus. Sans ces trois colonnes : erreur `Format`.
- Un jour sans aucune vente rend 404. Le rapport arrive le lendemain, parfois plus tard : un 404 de moins de 3 jours (`RETARD_VENTES`) est laissé pour le passage suivant, plus vieux il vaut 0.
- Fenêtre : les 90 jours finissant hier (`HISTORIQUE_VENTES`), seulement ceux que la base n'a pas encore (`a_relire`) : un rapport publié ne change plus. Premier passage : jusqu'à 90 appels, ensuite un par jour.

Écrit **`serie(telechargements.app_store)`**, `heure = -1`, puis le total `telechargements`, somme des deux stores (`Boutique::telechargements`, `crates/noyau/src/metrique.rs`).

## Ce que la source fait

La clé est lue une fois par passage. Puis, pour chaque business : le jeton, `customerReviews`, `appStoreVersions`, un `lot` (avis, fil) et la synchro des tâches (`ecrire`) ; enfin, si `ascVendor` est rempli, un `salesReports` par jour manquant. 2 appels, plus les jours de ventes. La source rend le nombre d'avis et de tâches, plus les lignes nouvelles du fil et les jours de ventes écrits.

## Ce qui peut casser

- Clé révoquée ou issuer faux : HTTP 401. Une nouvelle clé : déposer `AuthKey_<KEYID>.p8` dans le dossier ; s'il y en a plusieurs, la première par ordre alphabétique sert.
- Apple marque `appStoreState` comme déprécié au profit de `appVersionState`. S'il disparaît, le décodage échoue (`Format`).
- Plus de 200 avis : les anciens ne sont plus relus, une réponse tardive à l'un d'eux ne ferme pas sa tâche.
- Numéro de fournisseur faux : `salesReports` rend une erreur et le passage échoue. La clé doit avoir un rôle qui voit les ventes (Finances, Ventes ou Admin).

## Différences avec la version Swift

- **Signature** : `jsonwebtoken` (`crates/noyau/src/collecte/jwt.rs`) au lieu de CryptoKit. La clé est validée comme clé EC en PEM à la lecture.
- **Windows** : le dossier des clés est sous `%USERPROFILE%` ; le message `NonConfigure` cite le dossier réel de la machine.
- **Config** : `ascIssuer` et `ascVendor` viennent de la config du business, semée par la graine ([`../collecte.md`](../collecte.md) § Graine) ; il n'y a pas encore d'écran Réglages pour les saisir.
