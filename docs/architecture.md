# Architecture

Une app de bureau Tauri 2 (Windows, Mac ; Linux compile), sans serveur. Un workspace Cargo de trois crates (`Cargo.toml` à la racine, édition 2021, licence MIT) et trois couches qui ne se parlent que par la base SQLite :

```
services (PostHog, RevenueCat, ASC, Play, TikTok, Instagram, YouTube, Facebook, Firestore, iTunes, Notes, fiches de contenu, Claude)
   │  noyau::sources      une source par service, lancée par le Collecteur
   ▼
noyau::store         cockpit.sqlite : écritures typées (serie, fil, video, rang, a_traiter…)
   │  noyau::lecture      ce que les écrans affichent, rien que des select
   ▼
app (commandes Tauri) → ui/ (HTML, CSS, JS)      ne touchent jamais le réseau
cli (cockpit)                                    la même base en quelques lignes
```

L'app s'ouvre sur ce qui est déjà en base, le collecteur rafraîchit en fond, et chaque écriture fait monter `Store::version` : l'app la surveille toutes les 400 ms et prévient l'interface par l'événement `cockpit`, qui relit l'écran.

Une seule voie doit remonter vers un service : les trois actions du SAV (`sources::firebase::sav` : `repondre`, `fermer`, `traiter`), qui écrivent dans Firestore et envoient la notif FCM ([`sources/firebase.md`](sources/firebase.md) § SAV). Elles existent dans le cœur, mais aucune commande Tauri ne les appelle encore.

## Les crates

| Crate | Package, binaire | Rôle |
|---|---|---|
| `crates/noyau` | `noyau`, bibliothèque | La base, la collecte, les sources, les lectures. Ne connaît ni Tauri ni l'interface. |
| `crates/cli` | `cockpit-cli`, binaire `cockpit` | La CLI des agents ([`cli.md`](cli.md)). Ne dépend que de `noyau`. |
| `crates/app` | `cockpit`, binaire `cockpit-app` | L'app : fenêtre, tray, commandes, hôte de la collecte, et l'interface dans `ui/`. |

Dépendances du workspace : `rusqlite` (SQLite embarqué, `bundled`), `tokio`, `reqwest` (rustls), `serde`, `serde_json` (`preserve_order`), `async-trait`, `parking_lot`, `chrono`, `chrono-tz`, `keyring` (trousseau du Mac, gestionnaire d'identification de Windows, Secret Service sous Linux), `jsonwebtoken`, `dirs`, `base64`, `uuid`, `regex`, `futures`, `flate2`, `thiserror`, `ring`, `rsa`. L'app y ajoute Tauri et ses plugins `notification` et `opener`.

## Où est quoi dans `noyau`

| Module | Contenu |
|---|---|
| `chemins` | Où vivent la base, la graine et le cache (`donnees()`, `base()`, `graine()`, `cache()`, `adc()`), `COCKPIT_DIR` ([`runbook.md`](runbook.md) § Où sont les fichiers). |
| `db` | `Database` sur rusqlite (WAL, attente de 5 s sur un verrou), lecture seule possible, `Valeur`, la macro `p!` des paramètres. |
| `schema` | Le schéma SQL, identique à la version macOS, sans migrations : `COLONNES` ajoute les colonnes qui manquent. Tables dans [`donnees.md`](donnees.md). |
| `store`, `store_veille` | `Store` : ouverture (schéma, graine, purge de `collecte` à 30 jours), écritures typées, réglages, `lot` (une transaction), `version`. `store_veille` : mots-clés, vidéos, analyses et coût de la veille. |
| `business` | Un business et sa config JSON (`Config`, noms serde de la version Swift), ses événements PostHog (`Evenements`), la graine. |
| `metrique` | Les noms des métriques de `serie`, les deux boutiques (`Boutique`), `PART_NETTE`, `EUR_PAR_USD`. |
| `dimension` | Les réponses de l'onboarding qui découpent les installs. |
| `jour` | Jours de Paris (`yyyy-MM-dd`) et dates ISO, comme la version macOS les écrit. |
| `modeles`, `modeles_veille` | Ce que les sources écrivent et que les lectures rendent (`Plateforme`, `Passage`, `Evenement`, `VideoVeille`…). |
| `erreur` | `Erreur` (dont `NonConfigure`, `Format`, `Commande`, `Sql`) et `Res`. |
| `collecte` | Le moteur : `collecteur` (`Collecteur`, un tour par minute, `REPRISE_APRES_ECHEC` 600 s, `FRAICHEUR_NOTIF` 2 h, réglage `collecte.pause`), `source` (le trait `Source` et `Contexte`), `http` (`Http`, rejoue les 429 et 5xx), `gcloud` (jeton Google par l'ADC), `secrets`, `jwt` (ES256 pour ASC, RS256 pour Google), `commande` (programmes locaux, `chemins::outils()` d'abord), `safari` (cookies de Safari), `hote` (le trait `Hote`, `Navigateur`, `SansInterface`), `ytdlp` (dont le nightly géré par l'app), `miniatures` (cache des images). Règles dans [`collecte.md`](collecte.md). |
| `sources` | Une source par fichier ou dossier, et `toutes()`, le registre dans l'ordre de Santé. Fiches dans [`sources/`](sources/README.md). |
| `lecture` | `Lectures` (fil, vidéos, comptes, tâches, apps, séries, rangs) et un fichier par écran : `accueil`, `argent`, `data`, `videos`, `percee`, `veille`, `sante`, `widget` (le résumé du tray). Plus `fenetre` (la période), `decoupage` (libellés du tunnel), `noms` (langues et pays), `format` (nombres et dates en français). |

`crates/noyau/examples/essai.rs` lance une source contre les vraies API sur une base d'essai ([`runbook.md`](runbook.md) § Déboguer). Les tests sont à côté du code (`#[cfg(test)]`, dossiers `tests/` de `lecture` et des sources) ; `noyau::essai` est leur graine.

## Où est quoi dans `app`

| Fichier | Contenu |
|---|---|
| `src/main.rs` | Ouvre le `Store`, crée le `Collecteur` avec `HoteApp`, le démarre, pose le tray, surveille `Store::version` et les sources en cours (événement `cockpit`, toutes les 400 ms). Fermer la fenêtre la cache : l'app reste dans le tray. |
| `src/commandes.rs` | Les commandes communes : `etat`, `reglage`, `regler`, `ecrire_reglage`, `pause`, `relancer`, `session`, `lien` ; et `message`, le canal du plugin `veille`. |
| `src/ecrans/` | `mod.rs` : `lectures(store, business)` (les `Lectures` des business affichés, sur la boutique du réglage `boutique`), `resume_tray`, et `commandes()`, la liste passée à `invoke_handler`. Un fichier par écran (`accueil`, `apps`, `barre`, `data`, `reglages`, `sante`, `sav`, `veille`, `videos`) : ses commandes de lecture, qui rendent la vue toute formatée, et ses actions ([`ecrans.md`](ecrans.md)). `veille` pose le site de Viral Bank en WebView enfant de `main` (`Window::add_child`, feature `unstable` de Tauri) : dès lors `main` n'est plus une `WebviewWindow`, on la prend par `get_window("main")` et sa page par `webviews()`. |
| `src/hote.rs` | `HoteApp`, l'`Hote` de l'app : notifications du système, sessions (Safari sur Mac, sinon les cookies de la fenêtre Sessions), navigateur caché de la Veille (`Pages`, fenêtres `veille-*`). |
| `src/tray.rs` | L'icône du tray : nombre de choses à traiter, chiffres du jour en info-bulle, menu Ouvrir, SAV, Tout relancer, Collecte en pause, Quitter. Elle remplace le widget du bureau de la version Mac. |
| `tauri.conf.json` | Fenêtre `main` 1320 × 920 (au moins 1080 × 720), CSP, protocole `asset` pour le cache, installeur NSIS en français. `tauri.macos.conf.json` met la barre de titre en `Overlay` ; `tauri.windows.conf.json` et `tauri.linux.conf.json` coupent les décorations (l'interface dessine ses boutons). |
| `capabilities/` | `defaut.json` (fenêtres `main` et `sessions-*`), `veille.json` (les pages cachées `veille-*`, sur TikTok, Instagram et Facebook, n'ont que `veille:default` : la commande `message` du plugin `veille`, déclaré dans `build.rs`). |
| `ui/` | L'interface, sans framework ni bundler ([`../crates/app/ui/README.md`](../crates/app/ui/README.md), [`design.md`](design.md)). |

## Un chiffre, de bout en bout

Les actifs du jour : `PostHog::collecte` lance une requête HogQL toutes les 15 min et écrit `Store::serie(business, metrique::ACTIFS, jour, …)` ; le `Collecteur` journalise le passage dans `collecte`, `Store::version` monte, l'app émet `cockpit`, l'écran rappelle sa commande, qui lit par `Lectures`. Rien d'autre ne connaît PostHog.

## Réglages persistants

Table `reglage`, lue par `Store::reglage` et écrite par `Store::regler` (`None` efface). La commande `regler` écrit sans faire monter la version (réglage de fenêtre) ; `ecrire_reglage` la fait monter (réglage qui change les écrans).

| Clé | Effet |
|---|---|
| `ecran`, `business` | Dernier écran (un des 7 de la navigation) et business affichés, relus au lancement. |
| `boutique` | La boutique choisie (App Store, Play ou les deux), appliquée à toutes les `Lectures` des écrans. |
| `accueil.reseau`, `accueil.plan` | Relus au lancement avec les précédents (`REGLAGES` dans `commandes.rs`), pour les panneaux de l'Accueil. |
| `collecte.pause` | `1` suspend la collecte (`REGLAGE_PAUSE`). |
| `notifs.<type>` | `0` coupe les notifications de ce type du fil (`TypeFil::reglage_notif`). |
| `graine` | Date de modification de la graine déjà appliquée ([`collecte.md`](collecte.md) § Graine). |

Ce sont les clés de la version SwiftUI : sur Mac, les deux apps se relaient sur la même base.
