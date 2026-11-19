# YouTube

Fichier : `crates/noyau/src/sources/youtube.rs`, la lecture dans `crates/noyau/src/collecte/ytdlp.rs`. Tests : dans `youtube.rs` (`mod tests`). Id : `youtube`. Cadence : 1 h. Aucune clé, aucun cookie.

Les Shorts des chaînes YouTube suivies (table `compte`, [`../donnees.md`](../donnees.md)), leurs compteurs publics et les abonnés de la chaîne.

Aucune chaîne active dans les business suivis : `NonConfigure` « aucune chaîne YouTube suivie, en ajouter dans Vidéos ».

## La liste

```
yt-dlp --flat-playlist -J https://www.youtube.com/@monapp/shorts
```

Lu (`ytdlp::lire_liste`) : `channel_follower_count` (abonnés), `thumbnails[]` d'id `avatar_uncropped` (l'avatar), puis dans `entries[]` : `id` (id `youtube:<id>`), `title`, `view_count`, `url` et la miniature. La liste à plat n'a ni date ni likes.

## Le détail

```
yt-dlp -j --skip-download <url> <url> …
```

Une ligne JSON complète par vidéo (`ytdlp::details`, `ytdlp::lire_details`) : date, durée, likes, commentaires. Ouvert seulement pour les vidéos inconnues de la base ou publiées il y a moins de 7 jours (`RECENTE_S`), au plus 20 par chaîne et par passage (`OUVERTES_MAX`) : une vieille vidéo garde la date de son premier passage. Lancé après la liste, ses champs remplacent ceux de la liste à plat.

yt-dlp est trouvé comme pour TikTok ([`tiktok.md`](tiktok.md) § La liste : yt-dlp).

## Ce que la source fait

Pour chaque chaîne : liste, puis détail. Ensuite les miniatures et avatars absents du cache, et la première image des vidéos qui ne l'ont pas encore (`miniatures::extraire_premieres`, `crates/noyau/src/collecte/miniatures.rs` : le flux par `yt-dlp -g`, 960 px de haut au plus, puis une image par `ffmpeg`). Puis un seul `lot` : chaque vidéo et sa ligne du fil `youtube:pub:<id>` (« Nouvelle vidéo sur YouTube @monapp : <60 premiers caractères du titre> »), `video_releve`, `compte_releve` (abonnés, nombre de Shorts listés).

Une chaîne en échec n'arrête pas les autres. Le passage finit en erreur après avoir écrit le reste (« N échecs, dont @monapp : … », ou « première image <id> : … »). La source rend le nombre de vidéos, de relevés et de profils écrits.

## Ce qui n'y est pas

Le watch time, la rétention et les abonnés gagnés par vidéo ne sont que dans YouTube Studio ou l'API YouTube Analytics, qui demande un projet Google Cloud et un OAuth du propriétaire de la chaîne. Pas branché.

## Ce qui peut casser

- yt-dlp suit mal un changement de YouTube : le mettre à jour (`brew upgrade yt-dlp` sur Mac, `winget upgrade yt-dlp` ou `pip install -U yt-dlp` sous Windows).
- YouTube limite les requêtes anonymes (« Sign in to confirm you're not a bot ») : erreur au détail, la liste reste écrite.
- `ffmpeg` absent : chaque première image manquante fait un échec, le reste s'écrit.

## Différences avec la version Swift

- **Programmes** : `yt-dlp` et `ffmpeg` sont trouvés par `commande::trouver` (`crates/noyau/src/collecte/commande.rs`), dans le `PATH` puis une liste de dossiers par système, et plus par un zsh de login.
- Le reste suit la version Swift : même liste, même règle d'ouverture, même fil.
