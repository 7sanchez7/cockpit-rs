# Firebase (Firestore de Roleo)

Fichiers : `crates/noyau/src/sources/firebase/mod.rs` (le passage, la charge IA, messages et retours), `firebase/firestore.rs` (client REST), `firebase/cout.rs` (coût, voir [couts.md](couts.md)) et `firebase/sav.rs` (le SAV). id `firebase` (`ids_sav::SOURCE`), cadence 15 min. Aucune clé : le jeton du compte Google de gcloud (`ctx.gcloud.jeton()`, ADC, voir [../collecte.md](../collecte.md)).

Ce que le backend de Roleo compte lui-même dans Firestore : la charge IA heure par heure, les fils de messagerie sans réponse, les retours pas encore lus, et tout ce que montrera l'écran SAV (fils, messages, retours, emails). Lecture seule pendant un passage, sur le `firebase_project` de chaque business qui en a un ; aucun projet : `non_configure`, « aucun business n'a de projet Firebase ». Un business en échec arrête le passage, l'erreur est préfixée de son nom.

## Lire Firestore en REST

```
POST https://firestore.googleapis.com/v1/projects/<projet>/databases/(default)/documents:batchGet
POST https://firestore.googleapis.com/v1/projects/<projet>/databases/(default)/documents:runQuery
Authorization: Bearer <jeton gcloud>
x-goog-user-project: <projet>
```

`x-goog-user-project` est obligatoire avec les identifiants d'un user (ADC) : sans lui, Google refuse faute de projet de quota. Tout passe par `Firestore::requete`.

- `Firestore::documents(collection, ids, champs)` : **batchGet**, `{"documents": [noms complets], "mask": {"fieldPaths": champs}}`, 100 noms par appel. Chaque élément de la réponse a `found` ou `missing` ; ni l'un ni l'autre lève `Format`.
- `Firestore::ou(collection, champ, valeur, champs)` : **runQuery** avec un `fieldFilter` `EQUAL` et un `select` facultatif. Une égalité simple n'a pas besoin d'index.
- `Firestore::derniers(collection, parent, ordre, limite, champs)` : **runQuery** trié décroissant avec `limit` ; `parent` vise une sous-collection (`documents/users/<uid>:runQuery`). Un tri sur un seul champ n'a besoin que de l'index simple.

Une réponse runQuery sans résultat n'a qu'un élément avec `readTime`, filtré. Les valeurs typées (`integerValue` en chaîne, `doubleValue`, `stringValue`, `booleanValue`, `timestampValue`, `mapValue`, `arrayValue`, `nullValue`) sont décodées en `Valeur` (`Entier`, `Reel`, `Texte`, `Booleen`, `Instant`, `Carte`, `Nul`…) ; un type inconnu ou illisible fait échouer le décodage.

## Charge IA : `aiLoad`

Un document par heure UTC, `aiLoad/2026-11-12T23` (`id_heure`), écrit par Roleo par lots de 10 minutes :

```
m.<minute>.<fournisseur>.n    requêtes HTTP, reprises comprises
                        .q    réponses 429
                        .e    autres échecs
                        .ms   latence cumulée
m.<minute>.tk                 tokens facturables
m.<minute>.f                  appels qui ont échoué jusqu'à l'user
m.<minute>.v.<moteur>         caractères de voix synthétisés, par moteur
```

`Charge::depuis` somme les minutes de `m` : une clé dont la valeur est une carte est un fournisseur (pas de liste fermée), sauf `v`, la carte des caractères de voix ; `tk` et `f` sont des compteurs ; le reste est ignoré. Un `m` qui n'est pas une carte, une minute qui n'en est pas une ou un compteur non numérique lèvent `Format`. Un document absent est une heure sans appel : `Charge::default()`. Seul le champ `m` est demandé.

**Pourquoi pas `aiDays`.** Roleo tient aussi des agrégats par jour UTC ; le jour serait décalé d'une heure ou deux par rapport à Paris. La source lit donc les heures et les range à l'heure de Paris (`jour::cle`, `jour::heure`).

**Fenêtre.** Quand `cout_texte` d'hier ou de J-29 manque (`Store::historique_incomplet`), 30 jours de Paris (`JOURS_HISTORIQUE`), aujourd'hui compris ; sinon 2 jours, hier et aujourd'hui. Les ids sont calculés (`heures`) : lister `aiLoad` triée par nom demanderait un index composite. `aiLoad` ne garde que 30 jours.

`ecrire_charges` écrit, pour chaque jour de Paris touché :

| Métrique | Heure | Calcul |
|---|---|---|
| `tokens_ia` | -1 | somme des `tk` du jour |
| `echecs_ia` | -1 | somme des `f` |
| `cout_texte` | -1 | `cout::euros_tokens(tokens_ia)` |
| `cout_voix` | -1 | `cout::euros_voix` sur la somme des `v` ; pas écrit un jour sans `v` (la valeur déjà en base reste) |
| `cout_ia` | -1 | `cout_texte + cout_voix` (`Store::somme`) |
| `req_ia.<f>`, `q429_ia.<f>`, `err_ia.<f>` | -1, 0 à 23 | somme de `n`, `q`, `e` ; pour chaque fournisseur vu ce jour-là, 0 aux heures sans appel |
| `ms_ia.<f>` | -1, 0 à 23 | somme de `ms` / somme de `n`, seulement si `n` > 0 |

Les noms viennent de `metrique` (`requetes_ia`, `q429_ia`, `erreurs_ia`, `latence_ia`). Le journalier est toujours la somme des heures ; au passage à l'heure d'hiver, les deux heures UTC qui tombent sur 2 h de Paris s'additionnent.

### Le fil `IA`

Une ligne par heure et par fournisseur (`alerte`) quand :

- au moins 5 requêtes (`MINIMUM_PANNE`) ont toutes échoué : « DeepInfra ne répond plus, 6 requêtes en échec cette heure » ;
- ou les échecs de l'heure, 429 compris, atteignent 20 (`SEUIL_ECHECS`) : « Mistral, 25 erreurs 429 et 4 échecs cette heure ».

Id `firebase:ia:<fournisseur>:<id aiLoad>`, `at` = début de l'heure, lien `<dashboard>/ia/`. Les noms affichés viennent de `nom_fournisseur` (Anthropic, Mistral, DeepInfra, Fireworks, Gemini, sinon le nom en capitales initiales). Le texte garde le compte de la première détection.

## Messages : `users`, `supportOpen == true`

`Firestore::ou("users", "supportOpen", true, CHAMPS_FIL)`, les champs du SAV (`sav::CHAMPS_FIL` : `supportLastAt`, `supportLastFrom`, `supportPreview`, `supportOpen`, `supportUnread`, `displayName`).

- `messages_ouverts` (aujourd'hui, -1) : le nombre de fils ouverts.
- Une tâche par fil, clé `msg:<uid>:<supportLastAt en ISO>` : un nouveau message de l'user ouvre une nouvelle tâche. Texte : `extrait(supportPreview)` (une ligne, 80 caractères au plus, `LONGUEUR_EXTRAIT`) ou « Nouveau message ». Étiquette : `age` (« 12 min », « 2 h », « 3 j »). Ton `Alerte` après 24 h (`DELAI_MESSAGE_S`). Lien `<dashboard>/messages/`. Un fil sans `supportLastAt` lève `Format`.
- Fil `MSG`, id `firebase:msg:<uid>:<supportLastAt>`, `at` = `supportLastAt`, même texte.

## Retours : `feedback`, `status == "new"`

Même requête sur `feedback`, `status` = `"new"`, tous les champs.

- `retours_nouveaux` (aujourd'hui, -1).
- Une tâche par retour, clé `retour:<id>`, texte `libelle` :

  | `source`, `category` | Texte |
  |---|---|
  | `profile`, `idea` | « Idée : … » |
  | `profile`, `problem` | « Problème : … » |
  | `rating` | « Après le bilan : … » |
  | `chat_report` | « Signalement : … » |
  | autre | « Retour : … » |

  Sans message, le mot seul. Étiquette : l'âge du retour. Lien `<dashboard>/feedback/`. Un retour sans `createdAt` lève `Format`.
- Fil `RETOUR`, id `firebase:retour:<id>`, `at` = `createdAt`.

Messages et retours partent dans **un seul** `Store::synchroniser_taches("firebase", business, …)`, avec la liste complète : une tâche absente passe à faite.

Le dashboard est `https://<projet>.web.app` (`dashboard`). Il n'a pas de lien vers un fil précis : les liens mènent à la page.

Tout le passage d'un business (charges, messages, SAV) est écrit dans un seul `store.lot`.

## SAV

### Lire, à chaque passage

`sav::lire_sav`, après les fils ouverts et les retours `new` déjà lus :

- **Fils récents** : `users` par `supportLastAt` décroissant, 50 (`RECENTS`), champs `CHAMPS_FIL`, sans ceux qui n'ont jamais écrit. Avec les ouverts, sans doublon (`uniques`).
- **Retours récents** : `feedback` par `createdAt` décroissant, 50, avec les `new`.
- **Messages** : pour chaque fil dont `supportLastAt` diffère de celui en base (`Store::fils_lus`), les 100 derniers de `users/{uid}/support` (`MESSAGES_PAR_FIL`), remis dans l'ordre chronologique.
- **Emails** : pour les uids sans compte connu (`Store::comptes_sav`), `POST https://identitytoolkit.googleapis.com/v1/projects/<projet>/accounts:lookup` `{"localId": [100 uids au plus]}` (`sav::emails`). Un uid absent ou sans email est gardé vide.

Champs lus par `sav::fil`, `sav::message`, `sav::retour` : sur `users/{uid}`, `supportLastAt`, `supportLastFrom`, `supportPreview`, `supportOpen`, `supportUnread` (non vu si `id`, `text` et `at` sont présents, le `readUnread` de Roleo), `displayName` ; sur `users/{uid}/support/{id}`, `from`, `text`, `createdAt` ; sur `feedback/{id}`, `uid`, `source`, `category`, `stars`, `message`, `quote`, `conversationId`, `status`, `handledAt`, `platform`, `appBuild`, `createdAt`.

Le tout part dans `Store::synchroniser_sav` : un fil ouvert en base mais absent des ouverts lus passe à fermé, un retour à traiter absent des `new` passe à traité. Tables : [../donnees.md](../donnees.md).

### Écrire, sur un clic

Trois fonctions écrivent chez le service, par `Firestore::commit` (`documents:commit`, `{"writes": [...]}`, atomique ; chaque écriture est un `update` avec `updateMask` et `currentDocument.exists`, `Firestore::ecriture`) :

- **`sav::repondre(ctx, texte, cle)`** (le `sendTeamMessage` de Roleo). Le texte sans les blancs autour, 1 à 2 000 caractères (`reponse`, `LONGUEUR_REPONSE`), sinon « réponse vide ou de plus de 2000 caractères ». Un commit de deux écritures :
  - `users/{uid}/support/{id}`, `exists: false`, id de 20 caractères alphanumériques (`nouvel_id`) : `from: "team"`, `text`, `sentBy: "cockpit"`, `feedbackId: null`, `createdAt` ;
  - `users/{uid}`, `exists: true` : `supportLastAt`, `supportLastFrom: "team"`, `supportPreview` (`apercu` : blancs réduits, 140 caractères au plus), `supportOpen: false`, `supportUnread: {id, text, at}`.

  Puis la notif (`notifier`, le `notifyUser` de Roleo) : les documents `pushTokens` dont `uid` vaut l'uid (champ `token`), un `POST https://fcm.googleapis.com/v1/projects/<projet>/messages:send` par appareil, corps `notif` (`data.kind: "support"`, `message_id`, titre = nom du business, `android.priority: HIGH`, alerte APNs avec `sound: default`). Un 400 ou 404 de FCM efface le document `pushTokens`. Le message est écrit même si la notif échoue ; la fonction rend « Envoyé et notifié », « Envoyé, aucun appareil à notifier » ou « Envoyé, la notif a échoué : … ».
- **`sav::fermer(ctx, cle)`** (`closeThread`) : `users/{uid}` `supportOpen: false`.
- **`sav::traiter(ctx, retour, traite)`** : `feedback/{id}` `status: "done"` et `handledAt`, ou `status: "new"` et `handledAt: null`.

Après Firestore, la même chose dans la base (`Store::repondre_sav`, `fermer_fil_sav`, `traiter_retour_sav`).

**Pas encore branché** : aucune commande Tauri n'appelle ces trois fonctions, et l'écran SAV de l'app n'est pas codé. Elles sont testées dans le noyau, rien d'autre.

Droits : le compte Google de gcloud doit pouvoir écrire dans Firestore et envoyer par FCM. Un refus remonte en 403.

## Ce que ça coûte en lectures

Firestore compte une lecture par document rendu, et une par requête sans résultat. Par passage : 25 à 48 `aiLoad`, les fils ouverts, les retours `new`, 100 pour les fils et retours récents, plus les messages des fils qui ont bougé (100 au plus chacun). Le premier passage relit tous les fils récents et 30 jours d'`aiLoad`. Ne jamais ajouter une requête de groupe (`allDescendants`) ni une lecture qui grossit avec le nombre d'users : la voix se lit déjà dans `aiLoad` (`v`), sans lecture de plus.

## Ce qui peut casser

- Pas de compte Google dans gcloud, ou ADC expiré : `non_configure` ou 401. Voir [../runbook.md](../runbook.md).
- Roleo change la forme d'`aiLoad` : une forme inattendue lève `Format`. Un compteur renommé (`tk`, `n`) passerait en zéros sans erreur : comparer à la page `/ia/` du dashboard.
- Un moteur de voix sans prix dans `cout::voix_usd` : « moteur de voix sans prix : … », le passage lève.
- Un filtre de plus (inégalité, tri combiné) demande un index composite : 400 `FAILED_PRECONDITION`.
- Le dashboard quitte le site Hosting par défaut : changer `dashboard`.
- Roleo change la forme du support (`supportUnread`, `from`, la notif, `threadLimit`) : changer `sav.rs` en même temps, sinon le cockpit écrirait des fils que l'app lit mal.
