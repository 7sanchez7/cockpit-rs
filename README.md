# Cockpit

Une app de bureau, pour Windows et Mac, qui suit ses business au même endroit. Écrite pour Roleo, branchable sur n'importe quelle app mobile. Elle remplace l'ouverture de PostHog, RevenueCat, App Store Connect, TikTok Studio et du dashboard Firebase : on ouvre le cockpit, on voit tout, on sait quoi faire.

C'est le portage en Rust de [la version SwiftUI pour Mac](https://github.com/7sanchez7/cockpit). Les deux lisent et écrivent la même base au même endroit sur Mac, avec les mêmes clés dans le Trousseau : on passe de l'une à l'autre sans rien perdre.

## Ce qu'elle montre

| Écran | La question à laquelle il répond |
|---|---|
| App | Comment va l'app aujourd'hui, sur 7, 30 ou 90 jours, comparée à la période d'avant ? |
| Data | Combien on gagne, qui convertit en essai puis en payant, qui revient, découpé jusqu'au segment (âge, source, écran du tunnel…) ? |
| Vidéos | Quelle vidéo marche, laquelle passe le seuil de partages et d'enregistrements ? |
| Veille | Quelles vidéos des autres décollent sur nos mots-clés, et pourquoi ? |
| Santé | Les sources répondent-elles, l'IA tient-elle (replis, 429, latence), combien coûte la journée ? |
| SAV | Les fils avec les users, la réponse qui part avec une notif, les retours à traiter ; dessous, les avis sans réponse et les tâches à la main. |
| Apps de potes | Où sont nos apps et celles des potes dans le top 200 de l'App Store, pays par pays ? |

Plus une icône dans la barre des tâches (Windows) ou la barre des menus (Mac) avec le nombre de choses à traiter et les chiffres du jour, et des notifications du système (achat, avis, vidéo qui décolle, rang qui bouge, source en panne).

## Comment elle marche

Une app [Tauri 2](https://tauri.app) : le cœur en Rust (`crates/noyau`), l'interface en HTML et JavaScript sans framework (`crates/app/ui`), sans serveur. Un collecteur interroge chaque source à sa cadence (15 min à 3 h), écrit dans une base SQLite locale, et les écrans ne lisent que cette base : l'app s'ouvre instantanément, garde l'historique et marche hors ligne. Les clés viennent du trousseau du système (Gestionnaire d'identification sous Windows, Trousseau sur Mac), sinon de Secret Manager : aucune valeur sur le disque ni dans le dépôt.

```bash
cargo run -p cockpit        # lance l'app
cargo test --workspace      # tests
cargo run -p cockpit-cli -- etat   # la CLI des agents
```

## L'installer chez soi

Il faut [Rust](https://rustup.rs) et, pour fabriquer l'installeur, la CLI Tauri (`cargo install tauri-cli --version "^2"`). Sous Windows, les outils de compilation C++ de Visual Studio (« Build Tools », charge « Développement Desktop en C++ ») ; WebView2 est déjà dans Windows 10 et 11. Sur Mac, les Command Line Tools (`xcode-select --install`).

```bash
git clone https://github.com/7sanchez7/cockpit-rs cockpit-rs && cd cockpit-rs
cd crates/app && cargo tauri build
```

L'installeur est dans `target/release/bundle/` : `nsis/Cockpit_*_x64-setup.exe` sous Windows, `macos/Cockpit.app` et `dmg/` sur Mac. Avec Claude Code, `claude "/installer"` à la racine fait tout en posant les questions. Le détail, la graine et les clés sont dans [`docs/runbook.md`](docs/runbook.md) § Installer.

## Où lire la suite

Tout est dans [`docs/`](docs/README.md). Un agent commence par [`AGENTS.md`](AGENTS.md).

## Licence

MIT, voir [`LICENSE`](LICENSE).
