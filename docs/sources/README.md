# Sources

Une fiche par source : d'où viennent les chiffres, avec quelles requêtes, ce qui est écrit, ce qui peut casser, et ce qui change par rapport à la version Swift. Le contrat commun, les sessions, les secrets et la config des business sont dans [`../collecte.md`](../collecte.md) ; les tables, dans [`../donnees.md`](../donnees.md).

Les 9 sources du registre (`sources::toutes()`, `crates/noyau/src/sources/mod.rs`), dans l'ordre de Santé :

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

Les fichiers sont dans `crates/noyau/src/sources/`.

## Pas encore portées

Ces sources existent dans la version Swift mais pas encore sur `master`. Leurs fiches s'écriront avec leur portage :

| Fiche | Ce qu'elle fera |
|---|---|
| [firebase.md](firebase.md) | charge et erreurs IA, messages et retours, lus dans le Firestore de l'app |
| [couts.md](couts.md) | coût IA du jour en euros, texte et voix, calculé |
| [classements-itunes.md](classements-itunes.md) | rangs dans le top 200, note et nombre de notes, apps suivies |
| [veille.md](veille.md) | vidéos qui ont marché sur des mots-clés TikTok et Instagram, lues dans une page web cachée |
| [notes.md](notes.md) | plan de l'Accueil, lu dans l'app Notes de macOS |
