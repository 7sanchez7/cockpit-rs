# Cockpit, consignes pour les agents

**Règle absolue : chaque modification met à jour le bon fichier de `docs/` dans le même changement.** Une table qui gagne une colonne change [`docs/donnees.md`](docs/donnees.md), une source qui récupère un chiffre de plus change son fichier de [`docs/sources/`](docs/sources/), un écran qui bouge change [`docs/ecrans.md`](docs/ecrans.md).

Cockpit est une app de bureau (Windows et Mac) pour suivre ses business, écrite par Tony (seul dev, à l'aise backend et infra) pour Roleo et publiée : chacun clone le dépôt et le branche sur son business par le skill `/installer`. C'est le portage en Rust de la version SwiftUI ; **sur Mac, les deux partagent la base, la graine et le Trousseau**, et doivent pouvoir se relayer. Lire [`README.md`](README.md) pour le quoi, [`docs/README.md`](docs/README.md) pour la carte.

## Quoi lire avant de toucher

| Tâche | Lire d'abord |
|---|---|
| Comprendre le code, savoir où est quoi | [`docs/architecture.md`](docs/architecture.md) |
| Toucher à une table, lire ou écrire un chiffre | [`docs/donnees.md`](docs/donnees.md) |
| Ajouter ou réparer une source | [`docs/collecte.md`](docs/collecte.md), puis [`docs/sources/<source>.md`](docs/sources/) |
| Coder ou changer un écran | [`docs/ecrans.md`](docs/ecrans.md), [`docs/design.md`](docs/design.md) et [`crates/app/ui/README.md`](crates/app/ui/README.md) |
| Lancer, tester, déboguer, fabriquer l'installeur | [`docs/runbook.md`](docs/runbook.md) |
| Lire un chiffre, la veille, nos vidéos depuis un agent | CLI `cockpit` ([`docs/cli.md`](docs/cli.md)) |
| Installer le cockpit sur une autre machine, pour un autre business | skill `/installer` ([`.claude/skills/installer/SKILL.md`](.claude/skills/installer/SKILL.md)), [`docs/runbook.md`](docs/runbook.md) § Installer |
| Ajouter un business | [`docs/collecte.md`](docs/collecte.md) § Business et § Graine, ou Réglages, onglet Business |
| Répondre aux users, traiter les retours | [`docs/ecrans.md`](docs/ecrans.md) § SAV, puis [`docs/sources/firebase.md`](docs/sources/firebase.md) § SAV |

## Règles

- **Aucune valeur de secret dans le dépôt, ni dans la base, ni dans un log.** Les clés passent par `Secrets` (variable d'environnement, trousseau du système, sinon Secret Manager). Ne jamais afficher une clé ni une valeur de cookie, même tronquée, dans une sortie de commande.
- **Rien de propre à un utilisateur dans le dépôt** : le business et les comptes dans la graine (`graine.json`, dans le dossier des données), les clés dans le trousseau. Le code garde des valeurs par défaut, celles de Roleo (`Evenements` dans `crates/noyau/src/business.rs`).
- **Rétrocompatible avec la version SwiftUI.** Même schéma (`crates/noyau/src/schema.rs`, sans migrations), mêmes identifiants, mêmes formats de date et de JSON en base, mêmes clés de réglage, même trousseau (service `cockpit`, compte `<business>.<NOM>`). Une table ou une colonne nouvelle doit rester lisible par l'autre version (colonne facultative, table ignorée).
- **Multiplateforme.** Aucun chemin en dur (`~`, `/`) : `noyau::chemins`. Un programme local passe par `noyau::collecte::commande`, qui trouve aussi `yt-dlp.exe`. Les cookies passent par `Hote::cookies` (Safari sur Mac, sinon la fenêtre Sessions de l'app), le navigateur caché par `Hote::navigateur`. Ce qui n'existe que sur Mac (l'app Notes) rend `Erreur::NonConfigure` ailleurs, jamais un panic. Avant de finir : `cargo check --workspace --target x86_64-pc-windows-gnu` passe ([`docs/runbook.md`](docs/runbook.md) § Vérifier Windows depuis le Mac).
- **Les écrans ne parlent jamais au réseau.** L'interface (`crates/app/ui`) n'appelle que les commandes Tauri (`crates/app/src/commandes.rs`, `crates/app/src/ecrans/`), qui lisent la base par `noyau::lecture`. Seules les sources (`crates/noyau/src/sources/`) appellent des API. Exceptions : dans Apps de potes, les icônes (URL d'artwork d'iTunes) et l'ajout d'une app (un lookup iTunes) ; dans SAV, les trois actions (`repondre`, `fermer`, `traiter`) vers le SAV de la source Firebase ; dans Veille, la WebView du site de Viral Bank (un site distant, sans commande Tauri ; un lien qui ouvre un nouvel onglet part dans le navigateur du système).
- **Une source n'écrit que par `Store`** et seulement dans les tables et métriques documentées dans `docs/donnees.md`.
- **Lecture seule partout, sauf le SAV.** Le cockpit ne modifie rien chez PostHog, RevenueCat, App Store Connect, TikTok ni Firestore. Seule exception, propre à Roleo : l'écran SAV écrit dans le Firestore de Roleo ce qu'écrit son dashboard admin et envoie la notif FCM de la réponse, sur un clic, jamais pendant un passage ([`docs/sources/firebase.md`](docs/sources/firebase.md) § SAV).
- **RevenueCat v1 `GET /subscribers/{id}` crée le client s'il n'existe pas** : ne jamais l'appeler. L'API v2 en lecture seulement.
- **Clé PostHog personnelle scopée au projet**, jamais à l'organisation.
- Design : le terminal de marché noir et blanc de la version Mac est la référence ([`docs/design.md`](docs/design.md)). Fond noir, jamais de dégradé de couleur ; la couleur ne porte qu'un sens. Couleurs, rayons, tailles : uniquement les variables de `crates/app/ui/css/theme.css`.
- Textes de l'UI en français, sans tiret cadratin (une virgule à la place), nombres au format français (`48 200`, `7,10 €`).

## Écrire du code

- Le plus simple qui marche, sans abstraction prématurée, lisible par quelqu'un qui découvre le projet.
- Dépendances : celles du `Cargo.toml` du workspace et de Tauri. L'interface n'a ni framework ni bundler : du HTML, du CSS et des modules JavaScript servis tels quels.
- Pas de code mort, pas de bloc commenté. Commentaires courts, seulement quand la logique n'est pas évidente.
- Une constante définie une fois (`Metrique`, `jour`, les variables CSS), réutilisée partout.
- Une erreur n'est jamais avalée : une erreur de source remonte au `Collecteur`, qui l'écrit dans la table `collecte` et l'affiche dans Santé ; une erreur de commande arrive en texte à l'interface, qui l'affiche.
- **Avant de terminer** : `cargo test --workspace`, `cargo clippy --workspace --all-targets` et le contrôle Windows passent, sans avertissement nouveau.

## Git

Dépôt séparé de Roleo, sous licence MIT. Rien de propre à un utilisateur n'y entre (§ Règles). Commits courts : `feat: …`, `fix: …`, `docs: …`, sans ligne `Co-Authored-By` (elle fait apparaître Claude parmi les contributeurs du dépôt).
