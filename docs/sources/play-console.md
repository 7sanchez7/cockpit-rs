# Play Console

Fichier : `crates/noyau/src/sources/play_console.rs`, signature dans `crates/noyau/src/collecte/jwt.rs`. Tests : `crates/noyau/src/sources/play_console/tests.rs`. Id : `playconsole`. Cadence : 3 h.

Les installs Play par jour, comptés par le store (que l'app ait été ouverte ou non), lus dans les rapports groupés que la Play Console dépose chaque jour dans Cloud Storage. En lecture seule.

## Ce qu'il faut

- Dans la config du business : `playPackage` (le paquet Android, `com.monapp.app`) et `playBucket`. Le bucket se copie dans la Play Console, Télécharger des rapports, Statistiques, « Copier l'URI Cloud Storage » ; l'URI entière (`gs://pubsite_prod_rev_…/stats/installs`) se colle telle quelle, la source n'en garde que le nom du bucket (`bucket`).
- La clé JSON du compte de service, hors du dépôt :

  | Mac | Windows |
  |---|---|
  | `~/.googleplay/<business>-play-publisher.json` | `%USERPROFILE%\.googleplay\<business>-play-publisher.json` |

  `<business>` est l'id du business (`monapp`). Le JSON doit avoir `client_email`, `token_uri` et `private_key` (RSA en PEM).
- Dans la Play Console, Utilisateurs et autorisations : ce compte de service a la permission « Afficher les informations de l'application et télécharger les rapports groupés ».

Sans paquet ou sans bucket, le business est sauté ; aucun business suivi : `NonConfigure` « aucun business n'a de bucket Play ». Clé absente : `NonConfigure` au nom du business. Toute autre erreur devient `Commande("<business> : <erreur>")`.

## Le jeton

Un JWT RS256 signé avec la clé du compte de service (`assertion` : `iss` l'e-mail du compte, `scope` `https://www.googleapis.com/auth/devstorage.read_only` (`PORTEE`), `aud` le `token_uri` de la clé, une heure), échangé contre un jeton d'accès (`grant_type=urn:ietf:params:oauth:grant-type:jwt-bearer`, `jeton`).

## Les rapports

```
GET https://storage.googleapis.com/storage/v1/b/{bucket}/o?prefix=stats/installs/installs_{paquet}_
GET https://storage.googleapis.com/storage/v1/b/{bucket}/o/{nom encodé}?alt=media
```

On lit les rapports `installs_{paquet}_AAAAMM_overview.csv` des 4 derniers mois (`MOIS`, de quoi couvrir 91 jours), du plus ancien au plus récent (`a_lire`). Les autres découpes (`_country`, `_device`…) sont ignorées. Le nom d'objet est encodé octet par octet (lettres et chiffres gardés, le reste en `%XX`).

Chaque fichier est en UTF-16 avec marque d'ordre (UTF-8 sans marque accepté aussi), une ligne par jour (`installs`). On lit la colonne « Daily User Installs » : comptes Google qui installent l'app pour la première fois. Sans colonnes `Date` et `Daily User Installs` : erreur `Format`. Une valeur qui n'est pas un nombre aussi.

## Ce qui est écrit

**`serie(telechargements.play_store)`**, `heure = -1`, un jour par ligne de rapport, réécrit à chaque passage dans un `lot`, puis le total `telechargements`, somme des deux stores (`Boutique::telechargements`, `crates/noyau/src/metrique.rs`). Les installs vus par l'app restent `installs.play_store`, de PostHog ([`posthog.md`](posthog.md)), pour rester comparables à l'App Store.

## Ce que la source fait

Pour chaque business suivi : la clé, le jeton, la liste des objets, puis un téléchargement par rapport à lire (4 au plus). 1 + 1 + 4 appels par passage. La source rend le nombre de jours écrits.

## Ce qui peut casser

- **403** : la permission des rapports groupés manque au compte de service, ou elle vient d'être donnée (Google met jusqu'à 24 h à la propager).
- **Aucun fichier** : le bucket est juste mais le premier rapport n'est pas encore déposé (un jour ou deux après la mise en ligne). Le passage réussit avec 0 ligne.
- Les rapports ont un à deux jours de retard : le jour en cours manque toujours.

## Différences avec la version Swift

- **Clé** : `jsonwebtoken` lit la clé PKCS#8 du JSON telle quelle. La conversion en PKCS#1 pour le Trousseau (`PlayConsole.pkcs1`) n'existe plus.
- **Encodage** : l'UTF-16 n'est reconnu qu'avec sa marque d'ordre ; sans marque, le fichier est lu en UTF-8. La version Swift tentait l'UTF-16 d'abord, marque ou pas.
- **Config** : paquet et bucket viennent de la config du business, semée par la graine ([`../collecte.md`](../collecte.md) § Graine) ; il n'y a pas encore d'écran Réglages pour les saisir.
