# Runbook

Tout se lance depuis la racine du dépôt.

## Prérequis

- [Rust](https://rustup.rs) stable. Le dépôt ne fixe pas de version minimale (`rust-version` absent des `Cargo.toml`, édition 2021).
- **Windows** : les outils de compilation C++ de Visual Studio (« Build Tools for Visual Studio », charge « Développement Desktop en C++ »). WebView2 est déjà dans Windows 10 et 11.
- **Mac** : les Command Line Tools (`xcode-select --install`).
- Pour fabriquer l'installeur : `cargo install tauri-cli --version "^2" --locked`.
- Pour un business avec `firebaseProject` seulement, identifiants Google par défaut pour Secret Manager et Firestore : `gcloud auth application-default login` (une fois ; crée `application_default_credentials.json` dans `~/.config/gcloud/` sur Mac, `%APPDATA%\gcloud\` sous Windows). Le cockpit lit ce fichier et rafraîchit le jeton en HTTP ; la CLI gcloud ne sert qu'en repli (`gcloud auth print-access-token`). `GOOGLE_APPLICATION_CREDENTIALS` remplace le fichier.
- La clé App Store Connect `AuthKey_<KEYID>.p8` dans `.appstoreconnect/private_keys/` du dossier personnel (`~` sur Mac, `C:\Users\<nom>` sous Windows), et l'issuer dans le champ `ascIssuer` du business (Réglages, onglet Business, ou la graine).
- `yt-dlp` pour TikTok et YouTube, `ffmpeg` pour la première image des vidéos YouTube et Facebook. Mac : `brew install yt-dlp ffmpeg`. Windows : `winget install yt-dlp.yt-dlp Gyan.FFmpeg` (ou scoop). L'app les cherche dans son dossier d'outils (`<données>/bin`), dans le PATH, puis dans les dossiers de Homebrew, winget, scoop et pip, même lancée depuis le Finder ou le menu Démarrer (`noyau::collecte::commande`). Pour TikTok, elle télécharge elle-même un yt-dlp nightly dans ce dossier (renouvelé tous les 3 jours, `ytdlp::tenir_a_jour`) : celui du PATH ne sert que de repli.
- **Les sessions** de tiktok.com, instagram.com et facebook.com, pour les stats privées TikTok, Instagram et Facebook :
  - Mac : une session ouverte dans Safari, et l'accès complet au disque pour Cockpit (Réglages Système, Confidentialité et sécurité, Accès complet au disque, ajouter `Cockpit.app`, puis la relancer). Sans Safari, la fenêtre Sessions de l'app sert de repli.
  - Windows : la commande Tauri `session` (`crates/app/src/commandes.rs`, sites `tiktok`, `instagram`, `facebook`) ouvre la page de connexion du site dans une fenêtre de l'app : Réglages, onglet Sessions, « Se connecter ». La session reste dans le magasin de cookies de l'app (WebView2).
  - Session expirée : la source finit en `non_configure` avec le message qui dit où se reconnecter (`cockpit etat`), se reconnecter.
- **Le plan de l'Accueil** vient d'une note de l'app Notes (Mac seulement) dont la première ligne est « Cockpit » ([`sources/notes.md`](sources/notes.md)). Sous Windows, la source Notes reste en gris.

## Où sont les fichiers

| Quoi | Mac | Windows |
|---|---|---|
| Base `cockpit.sqlite`, graine `graine.json` | `~/Library/Application Support/Cockpit/` | `%APPDATA%\Cockpit\` |
| Images (miniatures, avatars, icônes) | `~/Library/Caches/Cockpit/` | `%LOCALAPPDATA%\Cockpit\` |
| Clés | Trousseau, service `cockpit` | Gestionnaire d'identification, « Informations d'identification Windows », `<business>.<NOM>.cockpit` |

Sur Mac, ce sont les dossiers et le trousseau de la version SwiftUI : les deux apps se relaient sur la même base. Le bundle porte aussi son identifiant (`com.tonysanchez.cockpit`, `crates/app/tauri.conf.json`) : `Cockpit.app` remplace l'autre dans Applications, une seule des deux tourne à la fois. `COCKPIT_DIR=<dossier>` remplace le dossier des données (et met le cache dans `<dossier>/cache`), pour essayer l'app sur une copie de la base.

## Installer

Sur une autre machine, pour un autre business : cloner, puis `claude "/installer"` à la racine. Le skill ([`.claude/skills/installer/SKILL.md`](../.claude/skills/installer/SKILL.md)) fait les étapes ci-dessous en posant les questions, sans jamais voir une clé. À la main :

1. La graine `graine.json` dans le dossier des données (§ Où sont les fichiers), sur le modèle de [`outils/graine.exemple.json`](../outils/graine.exemple.json) ([`collecte.md`](collecte.md) § Graine).
2. `cd crates/app && cargo tauri build`, puis lancer l'installeur de `target/release/bundle/` (`nsis/*-setup.exe` sous Windows, `macos/Cockpit.app` à glisser dans Applications sur Mac). Pour essayer sans installer : `cargo run -p cockpit`.
3. Les clés : Réglages, onglet Clés (rangées dans le trousseau du système). Sinon une variable d'environnement du même nom, Secret Manager pour un business avec `firebaseProject`, ou, sur Mac, les clés déjà rangées par la version SwiftUI (même trousseau). Les fichiers de clés App Store Connect et Play à leur place (§ Prérequis, [`collecte.md`](collecte.md) § Secrets) ; les sessions (§ Prérequis).

L'app Mac fabriquée ainsi n'est pas signée : au premier lancement, clic droit, Ouvrir. Pour qu'elle garde l'accès complet au disque d'un build à l'autre, la signer avec un certificat Apple Development du trousseau : `APPLE_SIGNING_IDENTITY="Apple Development: …" cargo tauri build --bundles app` (les noms : `security find-identity -v -p codesigning`), puis remplacer `Cockpit.app` dans Applications ; macOS redemande l'accès une seule fois. Sous Windows, SmartScreen demande « Informations complémentaires », « Exécuter quand même ».

## Commandes

```bash
cargo run -p cockpit                      # l'app, en debug
cargo test --workspace                    # tests du cœur et de la CLI
cargo clippy --workspace --all-targets    # sans avertissement
cargo run -p cockpit-cli -- etat          # la CLI (docs/cli.md)
cargo install --path crates/cli           # la CLI dans ~/.cargo/bin, à refaire après un changement du cœur
cargo run -p noyau --example essai -- RevenueCat   # une source contre les vraies API, sur une base d'essai
cd crates/app && cargo tauri build        # l'installeur, dans target/release/bundle/
```

## Vérifier Windows depuis le Mac

Le cœur, la CLI et l'app se vérifient pour Windows sans machine Windows :

```bash
rustup target add x86_64-pc-windows-gnu
brew install mingw-w64
cargo check --workspace --target x86_64-pc-windows-gnu
cargo clippy --workspace --target x86_64-pc-windows-gnu
```

C'est une vérification de compilation : l'installeur Windows se fabrique sur Windows (`cargo tauri build`), ou par une CI Windows.

## Déboguer

- **Une source en panne** : son message est dans la table `collecte` (`cockpit etat`). La rejouer seule : `cargo run -p noyau --example essai -- <Nom>` (nom ou id de la source, `crates/noyau/examples/essai.rs`). L'essai écrit dans `.cache/essai.sqlite` sous le dossier courant, jamais dans la vraie base, avec la graine de la machine ; il n'a ni notification ni navigateur caché, la Veille y finit donc en `non_configure`.
- **Journal** : l'app écrit ses erreurs sur la sortie d'erreur ; `cargo run -p cockpit` les montre dans le terminal.
- **La base** : `sqlite3 <dossier des données>/cockpit.sqlite`, ou `cockpit sql "select …"`. L'effacer est sans risque, seul l'historique accumulé est perdu.
- **Les images** : le dossier du cache, `miniatures/`. Le vider fait tout retélécharger au prochain passage de chaque source.
- **Une clé** : variable d'environnement du même nom pour tester (`REVENUECAT_SECRET_KEY_V2=… cargo run -p noyau --example essai -- RevenueCat`) : elle passe avant le trousseau (`Secrets::valeur`). Ne jamais l'afficher.
- **L'interface** : en debug, clic droit, Inspecter, ouvre les outils de développement de la WebView.

## Comptes vidéo

La graine les ajoute ([`collecte.md`](collecte.md) § Graine) ; l'écran Vidéos les ajoute, les met en pause et les retire ([`ecrans.md`](ecrans.md) § Vidéos) ; un agent fait la même chose dans la base, puis relance les sources (Ctrl+R ou ⌘R, ou F5). L'id est `<plateforme>:<compte>`, le compte sans `@`, en minuscules ; pour Facebook, le nom de la Page ou son id.

```bash
B=~/Library/Application\ Support/Cockpit/cockpit.sqlite     # Windows : "$APPDATA/Cockpit/cockpit.sqlite"
sqlite3 "$B" "select id, business, actif from compte"
sqlite3 "$B" "insert into compte (id, business, plateforme, handle, ajoute_le) values ('instagram:x', 'roleo', 'instagram', 'x', strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))"
sqlite3 "$B" "update compte set actif = 0 where id = 'instagram:x'"   # pause, 1 pour reprendre
sqlite3 "$B" "delete from compte where id = 'instagram:x'"           # ne plus suivre : vidéos et relevés restent
```

Plateformes : `tiktok`, `instagram`, `youtube`, `facebook`. Le business doit exister (`select id from business`).

## Voir l'app depuis un agent

Pour les chiffres, la CLI d'abord : `cockpit etat`, `cockpit videos` ([`cli.md`](cli.md)). La capture ne sert qu'à juger l'interface.

Changer d'écran sans clavier : le réglage `ecran` puis une relance, `sqlite3 <base> "insert or replace into reglage values ('ecran', 'data')"` (valeurs : `app`, `data`, `videos`, `veille`, `sante`, `aTraiter` (SAV), `apps`). Sur une copie : `COCKPIT_DIR=<copie> cargo run -p cockpit`. Sur Mac, capture de la fenêtre seule : `screencapture -x -o -l <id de la fenêtre> ecran.png`.

## Réparer l'environnement

**ADC expirés** (Santé : « invalid_grant ») : `gcloud auth application-default login`.

**yt-dlp introuvable** : Santé le dit pour TikTok et YouTube. L'app télécharge un nightly dans `<données>/bin` à chaque passage de TikTok ; si le téléchargement échoue (GitHub injoignable), l'installer (§ Prérequis) ; s'il est ailleurs, l'ajouter au PATH du système, pas seulement à celui du terminal.

**Notifications muettes** : Mac, Réglages Système, Notifications, Cockpit ; Windows, Paramètres, Système, Notifications, Cockpit. En debug sous Windows, les notifications portent le nom de PowerShell : c'est l'app installée qui a son nom.
