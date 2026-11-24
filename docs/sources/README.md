# Sources

Une fiche par source : d'où viennent les chiffres, avec quelles requêtes, ce qui est écrit, ce qui peut casser, et ce qui change par rapport à la version Swift. Le contrat commun, les sessions, les secrets et la config des business sont dans [`../collecte.md`](../collecte.md) ; les tables, dans [`../donnees.md`](../donnees.md).

Les 13 sources du registre (`sources::toutes()`, `crates/noyau/src/sources/mod.rs`), dans l'ordre de Santé :

| Fiche | `id` | Fichier | Cadence | Service | Écrit |
|---|---|---|---|---|---|
| [posthog.md](posthog.md) | `posthog` | `posthog.rs` | 15 min | PostHog, HogQL | actifs, installs (total et par store), essais, ronds, actifs par heure, fil des essais, détail des installs, portes du paywall |
| [revenuecat.md](revenuecat.md) | `revenuecat` | `revenuecat.rs` | 30 min | RevenueCat API v2, Firestore de l'app | MRR (total et par store), abonnés, essais actifs, revenu, achats, fil des achats, paiement des installs convertis |
| [app-store-connect.md](app-store-connect.md) | `asc` | `app_store_connect.rs` | 1 h | App Store Connect API | avis, tâches « répondre à l'avis », versions, téléchargements App Store |
| [play-console.md](play-console.md) | `playconsole` | `play_console.rs` | 3 h | Cloud Storage, rapports groupés de la Play Console | téléchargements Play comptés par le store |
| [tiktok.md](tiktok.md) | `tiktok` | `tiktok.rs` | 30 min | yt-dlp, profil public, TikTok Studio (API interne) | vidéos, relevés, analyse, abonnés, vues du jour |
| [instagram.md](instagram.md) | `instagram` | `instagram.rs` | 3 h | API interne d'Instagram, session | Reels, relevés, abonnés |
| [youtube.md](youtube.md) | `youtube` | `youtube.rs` | 1 h | yt-dlp | Shorts, relevés, abonnés |
| [facebook.md](facebook.md) | `facebook` | `facebook.rs` | 3 h | Meta Business Suite (API interne), session | vidéos, relevés, watch time, abonnés |
| [contenu.md](contenu.md) | `contenu` | `contenu.rs` | 15 min | fiches vidéo du dépôt du business, en local | titre et format des vidéos, tâches « verdict » |
| [firebase.md](firebase.md) | `firebase` | `firebase/` | 15 min | Firestore et Identity Toolkit de l'app, par les identifiants Google par défaut (gcloud) ; FCM pour les réponses du SAV, envoyées depuis l'écran SAV | charge et coût IA, messages ouverts, retours nouveaux, tâches, fils IA, messages et retours, tables `sav_*` |
| [classements-itunes.md](classements-itunes.md) | `classements` | `classements.rs` | 3 h | iTunes : `viewTop`, flux RSS en repli, `lookup` | apps, rangs dans le top 200, note et nombre de notes, fil des rangs |
| [notes.md](notes.md) | `notes` | `notes.rs` | 1 min | app Notes de macOS, en local (Mac seulement) | plan de l'Accueil |
| [veille.md](veille.md) | `veille` | `veille/` | à la main | page TikTok et Instagram cachée dans l'app, yt-dlp, Claude | vidéos, relevés, comptes et analyses de la veille, coût des analyses, fil de la veille |

Les fichiers sont dans `crates/noyau/src/sources/`.

## Hors registre

[couts.md](couts.md) : les prix avec lesquels Firebase convertit la charge IA en euros (`firebase/cout.rs`). Ce n'est pas une source.
