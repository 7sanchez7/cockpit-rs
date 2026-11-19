---
name: installer
description: Installe le cockpit sur le PC Windows ou le Mac d'un nouvel utilisateur et le branche sur son business à lui (son app, ses comptes, ses clés). À lancer une fois après le clone, par `/installer`.
---

# Installer le cockpit

Tu installes le cockpit pour la personne devant toi, sur sa machine (Windows ou Mac), avec son business. Tout ce qui est propre à un utilisateur (business, comptes, clés) est hors du code. Lis `AGENTS.md`, `docs/runbook.md` et `docs/collecte.md` avant de commencer.

**Règle absolue : aucune clé ne passe dans le chat.** Tu ne demandes jamais une clé, tu ne l'affiches jamais, même tronquée, tu ne l'écris ni dans le dépôt ni dans un fichier que tu lis ensuite. La personne la colle elle-même dans l'app (Réglages, onglet Clés) ou dépose le fichier au bon endroit.

Avance étape par étape, vérifie chacune avant la suivante, et parle français.

## 1. La machine

- Rust : `rustc --version` (1.85 ou plus). Absent : [rustup.rs](https://rustup.rs), que la personne installe elle-même.
- **Windows** : les outils C++ de Visual Studio (`where cl` dans « Developer PowerShell », ou l'installeur « Build Tools for Visual Studio », charge « Développement Desktop en C++ »). Puis `winget install yt-dlp.yt-dlp Gyan.FFmpeg`.
- **Mac** : `xcode-select -p` répond (sinon `xcode-select --install`). Puis `brew install yt-dlp ffmpeg`.
- La CLI Tauri : `cargo install tauri-cli --version "^2" --locked`.
- `cargo build -p cockpit` doit passer.

## 2. Le business

Pose les questions une par une, en disant à quoi sert chaque réponse ; tout champ est facultatif, un champ vide coupe seulement la source qui en a besoin (`docs/collecte.md` § Business) :

- nom de l'app et un id court en minuscules (`monapp`) ;
- App Store : l'id numérique de l'app (dans l'URL de sa page), l'issuer de la clé API App Store Connect et le numéro de fournisseur (Ventes et tendances) ;
- Play Store : le nom du paquet Android, et le bucket des rapports (`pubsite_prod_rev_…`) s'il l'a ;
- PostHog : l'hôte (`https://eu.posthog.com` ou `https://us.posthog.com`) ;
- les pays à suivre dans les classements (codes des storefronts de `crates/noyau/src/sources/classements.rs`) et la catégorie App Store (`genre`, 6017 = Éducation, liste dans `docs/sources/classements-itunes.md`) ;
- l'objectif de revenu mensuel en euros ;
- ses comptes TikTok, Instagram, YouTube et Facebook (le handle sans `@` ; pour Facebook, le nom ou l'id de la Page).

## 3. Les événements PostHog

Le cockpit lit six événements : `installation` (début du tunnel), `etape` (étape du tunnel franchie, avec une propriété `step`), `paywall`, `essai`, `achat`, `usage` (l'action qui prouve qu'on utilise l'app). Leurs noms par défaut sont ceux de Roleo (`Evenements` dans `crates/noyau/src/business.rs`). Demande à la personne les siens. Si elle ne sait pas, une fois la clé PostHog collée (étape 5), elle lance dans PostHog, onglet SQL :

```sql
select event, count() from events where timestamp > now() - interval 30 day group by event order by 2 desc limit 50
```

et te lit la liste ; propose la correspondance et fais-la valider. Un événement qu'elle n'a pas garde le nom par défaut (la série reste vide).

## 4. La graine

Écris `graine.json` dans le dossier des données (`%APPDATA%\Cockpit\` sous Windows, `~/Library/Application Support/Cockpit/` sur Mac) sur le modèle de `outils/graine.exemple.json`, avec les réponses des étapes 2 et 3. L'app l'applique au lancement, puis à chaque fois que le fichier change : le business est créé ou mis à jour, les comptes ajoutés s'ils ne sont pas déjà suivis. Vérifie qu'il se lit : `python -m json.tool <chemin> > NUL` sous Windows, `python3 -m json.tool <chemin> > /dev/null` sur Mac.

## 5. Fabriquer, lancer, brancher les clés

`cd crates/app && cargo tauri build`, puis l'installeur de `target/release/bundle/` (`nsis/*-setup.exe` sous Windows ; `macos/Cockpit.app` à glisser dans Applications sur Mac, premier lancement par clic droit, Ouvrir). Puis guide la personne, sans jamais voir les valeurs :

- **Réglages, onglet Clés**, son business : l'id du projet PostHog, une clé personnelle PostHog scopée au seul projet en lecture, la clé secrète v2 RevenueCat en lecture.
- **App Store Connect** : une clé API (Utilisateurs et accès, Intégrations, rôle Ventes ou Finance), le fichier `AuthKey_<KEYID>.p8` déposé dans `.appstoreconnect/private_keys/` du dossier personnel. Tu peux créer le dossier ; elle y glisse le fichier.
- **Play Console** : la clé JSON d'un compte de service invité dans la Play Console en lecture, déposée sous `.googleplay/<business>-play-publisher.json` du dossier personnel (`docs/sources/play-console.md`).
- **Sessions** : Windows, Réglages, onglet Sessions, « Se connecter » à TikTok, Instagram et Facebook dans l'app. Mac, une session ouverte dans Safari sur tiktok.com, instagram.com et business.facebook.com, et l'accès complet au disque pour `Cockpit.app` (Réglages Système, Confidentialité et sécurité), puis relancer l'app.
- **Plan de l'Accueil** (Mac seulement, facultatif) : une note de l'app Notes dont la première ligne est « Cockpit » (`docs/sources/notes.md`).

## 6. Vérifier

Santé, dans l'app, liste chaque source : verte, grise (non configurée, normal si la personne n'a pas ce service) ou rouge. Une source rouge se rejoue seule, son message s'affiche :

```bash
cargo run -p noyau --example essai -- PostHog
```

Corrige jusqu'à ce que tout ce qui est configuré soit vert.

## 7. Ce qui reste propre à Roleo

Ces parties marchent d'elles-mêmes si la personne n'a rien d'équivalent (la source passe en gris), mais se lisent comme Roleo. Dis-le, et propose de les adapter sur sa copie :

- le tunnel de l'écran Data : les étapes et leurs noms dans la source PostHog, les découpages (`crates/noyau/src/dimension.rs`, `crates/noyau/src/lecture/`) ;
- la date depuis laquelle l'événement `usage` est fiable, dans la source PostHog ;
- les sources Firebase (charge IA, retours, SAV) et Contenu, qui lisent le Firestore et le dossier de fiches de Roleo : sans `firebaseProject` ni `contenuDir`, elles sont coupées.

## 8. Les mises à jour

`git pull` à la racine, puis `cargo tauri build` et réinstaller. Sa graine et ses clés ne sont jamais dans le dépôt, donc jamais en conflit.
