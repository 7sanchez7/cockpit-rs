# TikTok

Fichier : `crates/noyau/src/sources/tiktok.rs`, la liste dans `crates/noyau/src/collecte/ytdlp.rs`, la session par l'hôte (`crates/noyau/src/collecte/hote.rs`, `crates/noyau/src/collecte/safari.rs`, `crates/app/src/hote.rs`). Tests : dans `tiktok.rs` (`mod tests`). Id : `tiktok`. Cadence : 30 min. Aucune clé : la session `tiktok.com`.

Les vidéos et les compteurs publics de chaque compte TikTok suivi (table `compte`, [`../donnees.md`](../donnees.md)) et, pour le compte de la session, ses stats privées : watch time, rétention, sources, public. TikTok ne donne pas les stats d'un créateur sans app développeur validée : on appelle l'API interne de TikTok Studio, celle de la page Analytics.

Aucun compte TikTok actif dans les business suivis : `NonConfigure` « aucun compte TikTok suivi, en ajouter dans Vidéos » (`comptes_suivis`).

## La liste : yt-dlp

```
yt-dlp --flat-playlist -J https://www.tiktok.com/@monapp
```

Lancé par `commande::lancer` (`crates/noyau/src/collecte/commande.rs`), qui cherche `yt-dlp` dans le dossier d'outils du cockpit (`<données>/bin`, où `ytdlp::tenir_a_jour` télécharge un nightly au début de chaque collecte : la stable échoue sur les profils TikTok, voir [veille.md](veille.md) § yt-dlp nightly), puis dans le `PATH` et les dossiers d'installation courants : Homebrew et `~/.local/bin` sur Mac, WinGet, Scoop et les scripts Python sous Windows (`yt-dlp.exe`). Absent : `Commande` « yt-dlp introuvable : l'installer et le mettre dans le PATH ». Public, sans cookie, marche pour tous les comptes.

Lu dans `entries[]` (`ytdlp::lire_liste`) : l'id, l'URL, la date de publication, la durée, la légende, les compteurs publics et la miniature d'id `cover`. Pas de liste dans la sortie : `Format` « yt-dlp TikTok @monapp : pas de liste de vidéos ».

## Le profil : la page publique

`GET https://www.tiktok.com/@<compte>` avec un User-Agent de navigateur (`UA_NAVIGATEUR`, `crates/noyau/src/collecte/http.rs`), sans cookie. Le JSON du `<script id="__UNIVERSAL_DATA_FOR_REHYDRATION__">`, sous `__DEFAULT_SCOPE__["webapp.user-detail"].userInfo` (`profil`) : `stats.followerCount`, `videoCount`, `heartCount` (sinon `heart`) font un `compte_releve` ; `user.avatarMedium` (sinon `avatarThumb`) est l'avatar. Sans ce bloc : `Format` « page de profil sans compteurs ».

## La session

`ctx.hote.cookies("tiktok.com", "sessionid")`, voir [`../collecte.md`](../collecte.md) § Sessions :

| | Mac | Windows |
|---|---|---|
| d'abord | les cookies de Safari (accès complet au disque requis) | |
| sinon | la fenêtre Sessions de l'app (commande `session`, `tiktok`) | la fenêtre Sessions de l'app |

Tous les cookies du domaine partent dans l'en-tête `Cookie` (`http::entete_cookie`). Sans `sessionid`, `NonConfigure` : la source finit en gris, après avoir écrit le reste. Aucune valeur de cookie n'est affichée, écrite, ni mise dans une erreur.

## Le compte connecté : `passport`

```
GET https://www.tiktok.com/passport/web/account/info/?aid=1988&app_name=tiktok_creator_center&device_platform=web_pc
```

`data.username` est le compte de la session (`compte_session`), mis en minuscules. Sans `username`, la session a expiré : `Format` « session TikTok expirée, se reconnecter sur tiktok.com dans Safari » (sous Windows : « … à tiktok.com dans Réglages, Sessions »). Si ce compte n'est pas suivi : pas de stats privées, la source finit en gris avec « stats privées : @monapp, le compte de la session, n'est pas suivi ».

## Les stats d'une vidéo : `insight`

Une seule requête par vidéo demande les 19 types de la page Analytics (`TYPES`) : TikTok ne rend les `_realtime` que demandés ensemble. Si la réponse n'a pas `realtime_total_video_views` (`complete`), TikTok n'avait pas fini de calculer : second essai après 3 s.

```
GET https://www.tiktok.com/aweme/v2/data/insight/?aid=1988&app_name=tiktok_creator_center&device_platform=web_pc&tz_name=Europe/Paris&type_requests=[…]
Referer: https://www.tiktok.com/tiktokstudio/analytics/<id vidéo>/overview
User-Agent: <Chrome macOS>
Cookie: <session>
```

`type_requests` est un tableau JSON de `{"insigh_type": "<type>", "aweme_id": "<id>"}` (sic, `insigh_type`), encodé dans l'URL (`requete`). Une réponse dont `status_code` n'est pas 0 : `Format` « insight : <status_msg> (status N) » (`objet`). Une valeur arrive en `{status, value}` ou, pour un `_realtime`, en `{value: {status, value}}` : `champ` déballe les deux et ne garde que `status` 0.

| Type | Lu | Colonne de `video_releve` (`releve`) |
|---|---|---|
| `video_info` | `statistics.play_count`, `digg_count`, `comment_count`, `share_count`, `collect_count` | `vues`, `likes`, `commentaires`, `partages`, `enregistrements` |
| `video_per_duration_realtime` | en secondes | `watch_moyen_s` |
| `video_finish_rate_realtime` | fraction de 0 à 1 | `completion` |
| `realtime_new_followers` (type `video_new_follower_realtime`) | pas encore calculé pour une vidéo du jour | `abonnes` |

Sans `play_count` : `Format` « insight : pas de statistiques ». Le reste va dans `video_analyse` (`analyse`, une ligne par vidéo, remplacée à chaque passage) :

| Type | Lu | Champ de `Analyse` |
|---|---|---|
| `video_total_duration_realtime` | secondes cumulées | `watch_total_s` |
| `video_uv` | spectateurs uniques | `spectateurs` |
| `video_viewer_new_viewer_percent` | part de nouveaux spectateurs | `nouveaux` |
| `video_viewer_follower_percent_realtime` | part de vues d'abonnés | `abonnes_part` |
| `video_retention_rate_realtime` | `value.list[]` de `{timestamp, value}`, trié par temps | `retention` |
| `video_traffic_source_percent_realtime` | `[{key, value}]` | `sources` |
| `item_search_terms` | `[{key, value}]` | `recherches` |
| `video_viewer_age_percent_realtime` | `[{key, value}]` | `ages` |
| `video_viewer_location_percent_realtime` | `value.country_percent_list[]` de `{country_name, country_vv_percent}` | `pays` |
| `video_vv_history_7d` | les vues des 7 premiers jours après la publication | `vues_par_jour` |

## Les vues du compte : `vv_history`

Même endpoint, `type_requests=[{"insigh_type":"vv_history","days":7,"end_days":1}]`, Referer `…/tiktokstudio/analytics/overview` (`historique`). Rend 7 `{status, value}` : les vues de tout le compte, d'il y a 7 jours à hier. Un jour pas encore calculé (`status` différent de 0) n'est pas écrit.

**TikTok coupe ses jours à minuit UTC**, malgré `tz_name` : les jours s'écrivent depuis la date UTC.

## Ce que la source fait

1. Pour chaque compte : yt-dlp, puis la page de profil. Un échec s'ajoute à la liste des échecs (« @monapp : … », « profil @monapp : … ») et le compte suivant continue.
2. La session, puis `passport`.
3. Un `insight` pour chaque vidéo du compte de la session à relever : à chaque passage si elle a moins de 14 jours (`RECENTE_S`), sinon une fois par jour (aucun relevé depuis minuit à Paris). Il remplace le relevé public. 300 ms entre deux appels. Un échec : « stats <id> : … ». Les compteurs publics de yt-dlp suivent la même règle.
4. Un `vv_history`. Une erreur ici, ou au `passport`, coupe les stats privées : `NonConfigure` en gris, toute autre erreur en échec « stats privées : … ».
5. Les miniatures et avatars absents du cache (`crates/noyau/src/collecte/miniatures.rs`), leurs échecs aussi dans la liste.
6. Tout s'écrit dans un seul `lot`, après les appels :
   - chaque vidéo (`publier`), avec sa ligne du fil `tiktok:pub:<id>`, `at` = publication : « Nouvelle vidéo sur @monapp : <titre> ». Le collecteur ne notifie que les lignes de moins de 2 h ;
   - `video_releve` pour chaque relevé, tous au même `at` ;
   - le fil `tiktok:palier:<id>:<palier>` quand les vues passent 1 000, 10 000, 100 000 ou 1 million (`PALIERS`) entre le relevé précédent et celui-ci ; une vidéo de moins de 24 h sans relevé part de 0. Texte : « 10 000 vues pour « <titre> » », ou « 1 million de vues pour … » (`libelle`, espace fine insécable entre les milliers). Titre (`titre`) : celui de la fiche (source Contenu, [`contenu.md`](contenu.md)), sinon la légende sans hashtags coupée à 60 caractères, sinon « une vidéo de @monapp » ;
   - `compte_releve` de chaque profil, `video_analyse` de chaque analyse non vide ;
   - `serie(vues_tiktok)` des jours du `vv_history`, réécrits à chaque passage : le chiffre exact de TikTok remplace l'estimation de la veille ;
   - `serie(vues_tiktok)` du jour, estimée par les relevés (plus bas) ;
   - `serie(vues_tiktok_j7)` sur le jour : le même calcul pour J-7, de minuit à la même heure, quand des relevés existent ce jour-là.
7. Des échecs font finir le passage en erreur après avoir écrit le reste (`sans_echec`) : « N échecs, dont <le premier> ». Sinon, une raison de gris pour les stats privées le fait finir en `NonConfigure`.

### Vues du jour

`vues_gagnees` : pour chaque vidéo TikTok du business, dernier relevé du jour moins son départ. Départ : son dernier relevé d'avant minuit ; sinon 0 si elle est sortie ce jour-là ; sinon son premier relevé du jour. On somme.

- Le jour du tout premier passage compte à partir de ce passage (0 au premier).
- Une vidéo de plus de 14 jours n'a qu'un relevé par jour : son gain est celui d'un relevé à l'autre.
- Le lendemain, le `vv_history` remplace l'estimation. Ses jours sont en UTC, les nôtres à Paris : une à deux heures de décalage, acceptées.

## Comptes sans stats privées

Seul le compte de la session a les stats privées. Les autres ont les compteurs publics de yt-dlp (vues, likes, commentaires, partages, enregistrements) et ceux du profil, sans visionnage, complétion, abonnés gagnés ni analyse. Pour les avoir : se connecter à ce compte, et perdre l'autre.

## Ce qui peut casser

- Session expirée : erreur au `passport`, la source passe en rouge. Se reconnecter sur tiktok.com, dans Safari ou dans la fenêtre Sessions.
- Pas d'accès complet au disque sur Mac, ou pas de `sessionid` : source en gris.
- yt-dlp suit mal les changements de TikTok : le mettre à jour (`brew upgrade yt-dlp` sur Mac, `winget upgrade yt-dlp` ou `pip install -U yt-dlp` sous Windows). Une sortie illisible fait échouer le compte avec le message de yt-dlp.
- TikTok change son API interne (paramètres, `insigh_type`, structure) : `Format` « insight : … ».
- Une limite de débit : les vidéos refusées sortent dans l'erreur de fin de passage, les autres s'écrivent.

## Différences avec la version Swift

- **Session** : sur Mac, Safari d'abord, puis la fenêtre Sessions de l'app si Safari n'a rien ; sous Windows, seulement la fenêtre Sessions. Sur Mac, si les deux échouent, c'est l'erreur de Safari qui s'affiche.
- **Messages** : sous Windows, ils renvoient à Réglages, Sessions au lieu de Safari (`safari::se_connecter`).
- **yt-dlp** : trouvé par `commande::trouver` (PATH, puis une liste de dossiers par système), plus par un zsh de login.
- **Cookies Chromium** : la lecture des cookies chiffrés `v10`/`v11` n'existe pas ici.
- Le reste suit la version Swift : mêmes appels, mêmes types, mêmes ids du fil, même calcul des vues du jour.
