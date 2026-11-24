# Collecte

Comment les chiffres arrivent dans la base. Le moteur est dans `crates/noyau/src/collecte/`, une source par service dans `crates/noyau/src/sources/`. Ce que chaque source lit et écrit est dans [`sources/`](sources/README.md), les tables et métriques dans [`donnees.md`](donnees.md).

## Le collecteur

`Collecteur` (`crates/noyau/src/collecte/collecteur.rs`) tourne tant que l'app est ouverte. `demarrer` fait un tour par minute ; chaque tour lance en tâche de fond les sources arrivées à échéance :

- échéance = début du dernier passage + `cadence` de la source (`a_echeance`) ;
- après un passage en **erreur** ou **non configuré** (session expirée, clé absente), le délai est le plus court de `cadence` et `reprise`. `reprise` vaut `REPRISE_APRES_ECHEC` (600 s) par défaut ; Instagram la met à sa cadence de 3 h. Une source repasse ainsi au vert dès que la session revient, sans attendre sa cadence ;
- une source sans aucun passage est due tout de suite ;
- une source déjà en cours n'est pas relancée (`en_cours`) ;
- le réglage `collecte.pause = 1` (`REGLAGE_PAUSE`, menu de la barre des tâches ou commande `pause`) suspend les tours ;
- `relancer(None)` relance tout de suite toutes les sources (Ctrl R ou ⌘R, « Tout relancer » du menu de la barre des tâches), `relancer(Some(id))` une seule (bouton de Santé).

Chaque passage écrit une ligne dans `collecte` (`Store::journal`) : `ok` avec le nombre de lignes, `erreur` avec le message (`Erreur` affichée) ou `non_configure` (gris dans Santé, pas une panne). Une erreur n'est jamais avalée : elle remonte au collecteur, qui l'écrit. Le journal va aussi sur la sortie d'erreur (`cockpit : posthog : 42 lignes en 3,1 s`), visible dans le terminal de `cargo run -p cockpit`.

| Source | `id` | Fichier | Cadence | Fiche |
|---|---|---|---|---|
| PostHog | `posthog` | `posthog.rs` | 15 min | [posthog](sources/posthog.md) |
| RevenueCat | `revenuecat` | `revenuecat.rs` | 30 min | [revenuecat](sources/revenuecat.md) |
| App Store Connect | `asc` | `app_store_connect.rs` | 1 h | [app-store-connect](sources/app-store-connect.md) |
| Play Console | `playconsole` | `play_console.rs` | 3 h | [play-console](sources/play-console.md) |
| TikTok | `tiktok` | `tiktok.rs` | 30 min | [tiktok](sources/tiktok.md) |
| Instagram | `instagram` | `instagram.rs` | 3 h, reprise 3 h | [instagram](sources/instagram.md) |
| YouTube | `youtube` | `youtube.rs` | 1 h | [youtube](sources/youtube.md) |
| Facebook | `facebook` | `facebook.rs` | 3 h | [facebook](sources/facebook.md) |
| Contenu | `contenu` | `contenu.rs` | 15 min | [contenu](sources/contenu.md) |

Le registre, dans l'ordre de Santé, est `sources::toutes()` (`crates/noyau/src/sources/mod.rs`). Une fiche par source dans [`sources/`](sources/README.md).

Aides partagées, dans `crates/noyau/src/collecte/` :

- `http.rs` : `Http` et `Requete` (ci-dessous), `UA_NAVIGATEUR` (Chrome sur Mac), `UA_SAFARI` et `UA_SESSION` (le navigateur de la session : Safari sur Mac, Edge WebView2 sous Windows), `entete_cookie` (l'en-tête `Cookie` d'une session, clés triées), `encoder` (une valeur d'URL), `extrait` (le début d'une réponse pour un message d'erreur).
- `safari.rs` : les cookies d'un domaine dans Safari (§ Sessions), et `se_connecter`, la fin des messages de session expirée : « se reconnecter sur `<site>` dans Safari » sur Mac, « se reconnecter à `<site>` dans Réglages, Sessions » ailleurs (`safari::se_connecter`).
- `ytdlp.rs` : la liste (`liste`, `yt-dlp --flat-playlist -J`) et le détail (`details`, `yt-dlp -j --skip-download`) des vidéos publiques d'un compte TikTok ou YouTube, et `tenir_a_jour`, qui garde un yt-dlp nightly dans `chemins::outils()` (§ Programmes locaux).
- `miniatures.rs` : télécharge les images absentes du dossier `miniatures` du cache (§ Où sont les fichiers), un fichier par clé (`:` et `/` remplacés par `_`) : l'id de la vidéo pour sa miniature, `avatar:<compte>`, `icone:<id App Store>`, `premiere:<id de vidéo>` pour la première image du flux. `extraire_premieres` les tire toutes, 4 à la fois (`EXTRACTIONS_EN_PARALLELE`) : `yt-dlp -g` en 960 px de haut au plus, puis `ffmpeg -frames:v 1`, écrit dans un `.part` puis renommé. Seuls YouTube et Facebook en ont (`cles`), leur miniature n'étant pas la première image. L'interface lit ce dossier sans réseau ; le vider fait tout revenir au passage suivant.
- `jwt.rs` : `es256` (App Store Connect) et `rs256` (Play Console).
- `gcloud.rs` et `secrets.rs` : § Secrets.
- `commande.rs` : § Programmes locaux.

## L'hôte

La collecte ne sait pas dans quelle app elle tourne. Ce qu'elle demande à l'app passe par le trait `Hote` (`crates/noyau/src/collecte/hote.rs`) :

| Méthode | Rôle | Par défaut |
|---|---|---|
| `notifier(titre, corps, id)` | une notification du système | aucune |
| `cookies(domaine, requis)` | la session d'un site, par nom de cookie ; `NonConfigure` sans le cookie `requis` | Safari (`safari::cookies`) |
| `provenance(domaine, requis)` | d'où vient la session, en dates et en nombres de cookies, jamais une valeur : pour les messages d'erreur | `safari::provenance` |

Deux hôtes :

- `SansInterface` : pas de notification, les cookies de Safari sur Mac. C'est l'hôte de l'essai d'une source et des tests.
- `HoteApp` (`crates/app/src/hote.rs`) : les notifications par `tauri-plugin-notification` ; les cookies de Safari d'abord, puis ceux du magasin de cookies des WebViews de l'app (§ Sessions).

## Sessions

TikTok, Instagram et Facebook lisent des stats privées avec la session de l'utilisateur sur le site. Aucune valeur de cookie n'est affichée, écrite ni mise dans une erreur.

- **Mac, Safari** (`crates/noyau/src/collecte/safari.rs`) : les fichiers `Cookies.binarycookies` du profil par défaut (`~/Library/Containers/com.apple.Safari/Data/Library/Cookies/`) et des profils créés dans Safari (`…/WebKit/WebsiteDataStore/<profil>/Cookies/`). On garde la session du profil dont le cookie `requis` a été créé en dernier (`plus_recente`), sans les cookies expirés. Il faut l'accès complet au disque pour Cockpit ; sans lui, `NonConfigure` « Safari illisible : donner l'accès complet au disque à Cockpit ». Pas de session : `NonConfigure` « pas de session dans Safari : se connecter sur `<domaine>` dans Safari ».
- **Fenêtre Sessions de l'app** : la commande `session` (`crates/app/src/commandes.rs`) ouvre la page de connexion du site (`tiktok`, `instagram`, `facebook`) dans une fenêtre `sessions-<site>` de 480 × 760. La session reste dans le magasin de cookies de l'app, partagé par toutes ses WebViews (WebKit sur Mac, WebView2 sous Windows). `HoteApp` le lit pour `https://www.<domaine>/`.
- **Ordre** : sur Mac, Safari, puis la fenêtre Sessions en repli ; si les deux échouent, l'erreur de Safari est rendue. Hors du Mac, `safari::cookies` rend `NonConfigure` « pas de session : se connecter à `<domaine>` dans Réglages, Sessions », et seule la fenêtre Sessions compte.

## Notifications

Après chaque passage, le collecteur prend les lignes du fil que la source vient d'insérer (`Store::prendre_nouveaux`, par le préfixe `<source>:` de l'id) et envoie une notification par ligne (`a_notifier`), titre `<Business> · <Type>`, corps = le texte du fil. Garde-fous :

- rien au **premier passage** d'une source qui n'a jamais réussi (sinon l'historique entier notifierait) ;
- rien pour une ligne de plus de 2 h (`FRAICHEUR_NOTIF`) ;
- un type coupé dans Réglages (`notifs.<type> = 0`, `TypeFil::reglage_notif`) ne notifie pas ;
- au-delà de 3 lignes d'un coup, un résumé « N autres nouveautés ».

Une source dont le dernier passage était `ok` et qui tombe en `erreur` envoie « Cockpit · `<Source>` en panne », corps = le message. Un passage `non_configure` ne notifie pas.

## Contrat d'une source

```rust
#[async_trait]
pub trait Source: Send + Sync {
    fn id(&self) -> &'static str;        // "posthog" : clé du journal, préfixe des ids du fil et des tâches
    fn nom(&self) -> &'static str;       // "PostHog"
    fn cadence(&self) -> i64;            // secondes entre deux passages réussis
    fn reprise(&self) -> i64 { REPRISE_APRES_ECHEC }
    async fn collecte(&self, ctx: &Contexte) -> Res<usize>;   // lignes écrites
}
```

Le trait est dans `crates/noyau/src/collecte/source.rs`. `Contexte` donne `store`, `secrets`, `http`, `gcloud`, `businesses` et `hote`. Les règles :

1. **Boucler sur `ctx.businesses`** et ne traiter que ceux configurés pour la source (un `app_store_id`, des comptes de la table `compte`…). Si aucun ne l'est, rendre `Erreur::NonConfigure`. Une erreur sur un business nomme le business (`Mon App : HTTP 401 …`, `revenuecat::nomme`).
2. **Écrire seulement par `Store`** (`serie`, `somme`, `fil`, `video`, `fiche_video`, `releve`, `releve_compte`, `analyse`, `app`, `rang`, `avis`, `synchroniser_taches`, `installs`, `portes`, `paiement`). Lire par `ctx.store.db.query` est permis. Grouper les écritures dans `ctx.store.lot(|| …)`.
3. **Métriques** : seulement celles de `crates/noyau/src/metrique.rs` et de [`donnees.md`](donnees.md). Une nouvelle s'ajoute aux deux.
4. **Fil** : id `<source>:<id stable chez le service>`, `at` = l'heure de l'événement, pas de la collecte. Texte court en français, sans tiret cadratin.
5. **Tâches** : `synchroniser_taches(source, business, taches)` avec la liste complète des tâches encore ouvertes chez le service ; les absentes passent à faites.
6. **Lecture seule** chez le service. Aucun POST qui modifie quoi que ce soit (les POST de requête, HogQL, `runQuery` de Firestore, graphql de Business Suite, sont permis).
7. **Aucun secret** dans une erreur, un log, la base ni une sortie : ni clé, ni cookie, ni jeton, ni URL ou page qui en contient.
8. **Erreurs** (`crates/noyau/src/erreur.rs`) : `NonConfigure` (il manque un réglage, une clé, une session), `Http(code, extrait)` (rendue par `Http`), `Format` (réponse qui ne ressemble pas à ce qu'on attend), `Commande` (programme local, réseau), `Sql`. Ne jamais rendre 0 ligne en silence quand le service a répondu n'importe quoi.

`Http` (`crates/noyau/src/collecte/http.rs`) : `donnees(&Requete)` rend le corps d'une réponse 2xx, rejoue jusqu'à 3 fois sur 429 et 5xx (`Retry-After` respecté, 30 s au plus, sinon 1 s puis 2 s), délai de 60 s par requête ; un autre code rend `Http(code, 300 premiers octets)`, un échec réseau `Commande("réseau : …")`. `texte`, `json::<T>` et `objet` décodent. `Requete::get(url)` ou `Requete::new(url, methode)`, puis `.jeton()`, `.entete()`, `.json()`, `.formulaire()`, `.brut()`. Aucun cookie automatique : une source qui en a besoin pose l'en-tête elle-même.

## Programmes locaux

`commande::lancer(programme, args)` et `commande::texte` (`crates/noyau/src/collecte/commande.rs`) lancent `gcloud`, `yt-dlp` ou `ffmpeg` sans bloquer l'interface, sans fenêtre de console sous Windows. Un code de sortie non nul rend `Commande("<programme> a échoué (<code>) : <début de la sortie d'erreur>")`.

Une app lancée depuis le Finder ou le menu Démarrer n'a pas le PATH du terminal : `commande::trouver` cherche d'abord dans `chemins::outils()` (`<données>/bin`, où `ytdlp::tenir_a_jour` télécharge un yt-dlp nightly tous les 3 jours : [sources/tiktok.md](sources/tiktok.md) § yt-dlp nightly), puis dans le PATH, puis :

| Mac | Windows |
|---|---|
| `/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin`, `/bin` | `%LOCALAPPDATA%\Microsoft\WinGet\Links` |
| `~/.local/bin` | `%LOCALAPPDATA%\Programs\Python\Scripts` |
| `~/google-cloud-sdk/bin` | `%LOCALAPPDATA%\Google\Cloud SDK\google-cloud-sdk\bin` |
| | `%USERPROFILE%\scoop\shims` |
| | `C:\Program Files (x86)\Google\Cloud SDK\google-cloud-sdk\bin`, `C:\Program Files\Google\Cloud SDK\google-cloud-sdk\bin` |

Sous Windows, il essaie les extensions `.exe`, `.cmd` et `.bat`. Le dossier du programme trouvé passe en tête du PATH de l'enfant, pour que yt-dlp trouve ffmpeg. Introuvable : `Commande("<programme> introuvable : l'installer et le mettre dans le PATH")`.

## Secrets

`ctx.secrets.valeur("NOM", business)` (`crates/noyau/src/collecte/secrets.rs`) cherche, dans l'ordre :

1. la variable d'environnement `NOM` non vide (débogage, `NOM=… cargo run -p noyau --example essai -- <Source>`) ;
2. le cache en mémoire de l'app (une valeur déjà trouvée dans le trousseau ou Secret Manager) ;
3. le trousseau du système, service `cockpit`, compte `<business>.<NOM>` (saisi dans Réglages) ;
4. Secret Manager du `firebaseProject` du business, en REST, s'il en a un (`…/secrets/<NOM>/versions/latest:access`). Un 403 ou 404 rend `NonConfigure`.

Sans aucun des quatre : `NonConfigure` « clé `NOM` absente du trousseau du système pour `<business>` ».

| Trousseau | Mac | Windows |
|---|---|---|
| où | Trousseau de session, service `cockpit`, compte `<business>.<NOM>` | Gestionnaire d'identification, « Informations d'identification Windows », `<business>.<NOM>.cockpit` |

Sur Mac, c'est le trousseau de la version Swift : les deux apps lisent les mêmes clés.

Le jeton Google (`GCloud`, `crates/noyau/src/collecte/gcloud.rs`) ouvre Secret Manager et Firestore, avec l'en-tête `x-goog-user-project`. Il vient des identifiants par défaut de type `authorized_user` (créés par `gcloud auth application-default login`), rafraîchis en HTTP ; sinon de `gcloud auth print-access-token`. Il est gardé jusqu'à 5 minutes de son expiration.

| Identifiants par défaut | Mac | Windows |
|---|---|---|
| fichier | `~/.config/gcloud/application_default_credentials.json` | `%APPDATA%\gcloud\application_default_credentials.json` |

`GOOGLE_APPLICATION_CREDENTIALS` remplace ce chemin (`chemins::adc`).

| Secret | Source | Où |
|---|---|---|
| `POSTHOG_PERSONAL_API_KEY`, `POSTHOG_PROJECT_ID` | PostHog | trousseau ou Secret Manager |
| `REVENUECAT_SECRET_KEY_V2` | RevenueCat | trousseau ou Secret Manager |
| clé `.p8` App Store Connect | App Store Connect | `.appstoreconnect/private_keys/AuthKey_<KEYID>.p8` du dossier personnel, voir sa fiche |
| clé JSON du compte de service Play | Play Console | `.googleplay/<business>-play-publisher.json` du dossier personnel, voir sa fiche |
| cookies `tiktok.com`, `instagram.com`, `facebook.com` | TikTok, Instagram, Facebook | § Sessions |
| aucun | Contenu, YouTube | |

## Business

Un business = une ligne de `business` avec sa `Config` (`crates/noyau/src/business.rs`), en JSON avec les mêmes clés que la version Swift. Il vient de la graine (ci-dessous) ou de Réglages, onglet Business.

| Champ JSON | Champ Rust | Utilisé par |
|---|---|---|
| `firebaseProject` | `firebase_project` | Secret Manager, fil des achats de RevenueCat |
| `posthogHost` | `posthog_host` | PostHog (avec les deux secrets PostHog) |
| `appStoreId` | `app_store_id` | App Store Connect |
| `ascIssuer` | `asc_issuer` | App Store Connect : issuer de la clé API (pas un secret) |
| `ascVendor` | `asc_vendor` | App Store Connect : numéro de fournisseur, pour les téléchargements ; absent, cette étape seule est sautée |
| `playPackage`, `playBucket` | `play_package`, `play_bucket` | Play Console : paquet Android et bucket des rapports groupés (`pubsite_prod_rev_…`, ou l'URI `gs://` copiée telle quelle) |
| `pays`, `genre` | `pays`, `genre` | Classements (storefronts et catégorie, 6017 par défaut) |
| `contenuDir` | `contenu_dir` | Contenu (dossier `bank` des fiches) |
| `objectifMensuelEUR` | `objectif_mensuel_eur` | écran Data, 10 000 par défaut |
| `evenements` | `evenements` | PostHog : le nom des six événements lus, `installation`, `etape`, `paywall`, `essai`, `achat`, `usage` ([`sources/posthog.md`](sources/posthog.md)). Graine seulement |

Un champ absent coupe la source pour ce business, sans erreur. Une clé absente du JSON prend sa valeur par défaut.

`lettre` vaut l'initiale du nom en majuscule et `teinte` vaut `menthe` quand la graine ne les donne pas.

Les comptes vidéo (TikTok, Instagram, YouTube, Facebook) ne sont pas dans la config : ils vivent dans la table `compte` ([`donnees.md`](donnees.md)), ajoutés par la graine, dans l'écran Vidéos ou par un agent ([`runbook.md`](runbook.md) § Comptes vidéo).

## Graine

Ce qui est propre à un utilisateur n'est pas dans le dépôt : ses businesses et ses comptes sont dans `graine.json`, dans le dossier des données (`chemins::graine`), écrit par `/installer` sur le modèle de [`outils/graine.exemple.json`](../outils/graine.exemple.json).

| | Mac | Windows |
|---|---|---|
| `graine.json` | `~/Library/Application Support/Cockpit/` | `%APPDATA%\Cockpit\` |

`COCKPIT_DIR` remplace ce dossier.

- `businesses` : `id` et `nom` obligatoires ; `lettre`, `teinte` et `config`, dont chaque champ est facultatif.
- `comptes` : `business`, `plateforme`, `handle` (normalisé comme une saisie de l'écran Vidéos, `@MonApp` devient `monapp` ; un nom invalide fait échouer la lecture) et `actif` (vrai par défaut).

`Store::ouvrir` l'applique (`semer`) quand sa date de modification diffère de celle retenue dans le réglage `graine` : au premier lancement, puis après chaque modification du fichier. Tout se fait dans une transaction. Un business est créé ou remplacé (nom, lettre, teinte, config). Un compte n'est ajouté que s'il n'est pas déjà suivi : une pause faite dans l'app tient. Un business ou un compte supprimé dans l'app revient à la modification suivante de la graine : le retirer aussi du fichier. Pas de fichier : rien ne se passe, le cockpit démarre vide. Un fichier illisible fait échouer l'ouverture de la base avec `Commande("<chemin> illisible : <erreur>")`.

## Où sont les fichiers

| Quoi | Mac | Windows | Code |
|---|---|---|---|
| base `cockpit.sqlite`, `graine.json` | `~/Library/Application Support/Cockpit/` | `%APPDATA%\Cockpit\` | `chemins::donnees` |
| images, `miniatures/` | `~/Library/Caches/Cockpit/` | `%LOCALAPPDATA%\Cockpit\` | `chemins::cache` |

Avec `COCKPIT_DIR`, les données sont dans ce dossier et le cache dans `<COCKPIT_DIR>/cache`. Sur Mac, ce sont les dossiers de la version Swift : les deux apps se relaient sur la même base.

## Essayer une source

```bash
cargo run -p noyau --example essai -- PostHog
```

L'exemple `crates/noyau/examples/essai.rs` lance la seule source demandée (par son nom ou son `id`, sans tenir compte de la casse) contre les vraies API, avec la graine de la machine, sur `.cache/essai.sqlite` (pas la base de l'app). L'hôte est `SansInterface` : pas de notification. Il affiche ensuite ce qu'elle a écrit : séries du passage, fil, tâches, compteurs des tables.

## Ajouter une source

1. `crates/noyau/src/sources/<nom>.rs` qui respecte le contrat, et ses tests (le décodage des réponses, sur des exemples figés), dans le fichier ou dans `crates/noyau/src/sources/<nom>/tests.rs`.
2. L'ajouter à `sources::toutes()`.
3. Ses tables et métriques dans [`donnees.md`](donnees.md) (et `metrique.rs`), sa fiche dans [`sources/`](sources/README.md), sa ligne dans le tableau plus haut.
4. `cargo run -p noyau --example essai -- <Nom>`, `cargo test --workspace`, `cargo clippy --workspace --all-targets`, puis `cargo check --workspace --target x86_64-pc-windows-gnu` ([`runbook.md`](runbook.md) § Vérifier Windows depuis le Mac).
