# Instagram

Fichier : `crates/noyau/src/sources/instagram.rs`, la session par l'hôte (`crates/noyau/src/collecte/hote.rs`). Tests : dans `instagram.rs` (`mod tests`). Id : `instagram`. Cadence : 3 h, reprise après échec : 3 h aussi. Aucune clé : la session `instagram.com`.

Les Reels des comptes Instagram suivis (table `compte`, [`../donnees.md`](../donnees.md)), leurs compteurs et les abonnés du compte. Pas de watch time : l'API interne ne le donne pas sur le fil, et l'API officielle demande un compte pro relié à une app Meta.

Aucun compte Instagram actif dans les business suivis : `NonConfigure` « aucun compte Instagram suivi, en ajouter dans Vidéos ».

## La session

`ctx.hote.cookies("instagram.com", "sessionid")`, même lecture que pour TikTok ([`tiktok.md`](tiktok.md) § La session) : Safari puis la fenêtre Sessions sur Mac, la fenêtre Sessions seule sous Windows. Tous les cookies partent dans `Cookie`, `csrftoken` aussi dans `X-CSRFToken`. Aucune valeur n'est affichée, écrite ni mise dans une erreur.

En-têtes de chaque appel, ceux de la page web d'Instagram : User-Agent du navigateur de la session (`UA_SESSION`, `crates/noyau/src/collecte/http.rs` : Safari sur Mac, Edge WebView2 sous Windows), `X-IG-App-ID: 936619743392459` (`APP_WEB`, l'app web d'Instagram, exigée par l'API interne), `X-Requested-With: XMLHttpRequest`, `X-ASBD-ID: 359341`, `Accept: */*`, `Accept-Language: fr-FR,fr;q=0.9`, `Referer` = le profil.

## Le profil : `web_profile_info`

```
GET https://www.instagram.com/api/v1/users/web_profile_info/?username=<compte>
```

Sous `data.user` (`profil`) : `id` (pour le fil), `edge_followed_by.count` (abonnés), `edge_owner_to_timeline_media.count` (publications), `profile_pic_url_hd` sinon `profile_pic_url` (l'avatar). Un `compte_releve` par passage. Sans `data.user.id` : `Format` « Instagram @monapp : profil introuvable ».

## Le fil : `feed/user`

```
GET https://www.instagram.com/api/v1/feed/user/<id>/?count=33
```

Les 33 dernières publications, dans `items[]` (`fil`). Seules les vidéos (`media_type` 2) sont gardées. Lu : `code` (id `instagram:<code>`, lien `https://www.instagram.com/reel/<code>/`), `caption.text`, `taken_at`, `video_duration`, `like_count`, `comment_count`, `image_versions2.candidates[0].url` (la miniature). Vues : `ig_play_count` d'abord (les lectures sur Instagram seul), sinon `play_count` (qui compte aussi le partage Facebook), sinon `view_count`. Une vidéo sans aucun des trois n'a pas de relevé. Sans `items` : `Format` « Instagram @monapp : pas de publications ».

## Ce que la source fait

Pour chaque compte : profil puis fil. Puis les miniatures et avatars absents du cache (`crates/noyau/src/collecte/miniatures.rs`), puis un seul `lot` : chaque vidéo et sa ligne du fil `instagram:pub:<id>` (« Nouvelle vidéo sur Instagram @monapp : <60 premiers caractères de la légende> »), `video_releve`, `compte_releve`. Un compte en échec (autre qu'une session refusée) n'arrête pas les autres ; le passage finit en erreur après avoir écrit le reste (« N échecs, dont @monapp : … »). La source rend le nombre de vidéos, de relevés et de profils écrits.

**Une session refusée arrête tout le passage**, avant le `lot` : rien n'est écrit, même les comptes déjà lus, et la source passe en gris. La version Swift faisait de même.

## La session refusée

Instagram rend un 401, 403 ou 404, ou une page de connexion à la place du JSON (`require_login`). L'erreur (`refusee`) est un `NonConfigure` :

```
session Instagram refusée (<raison>), se reconnecter sur instagram.com dans Safari
```

La raison dit ce qu'Instagram a renvoyé : `HTTP <code>`, `réponse pas en JSON` ou `require_login`. Sur une page d'erreur marquée `not-logged-in`, elle ajoute la provenance des cookies (`Hote::provenance`) : pour chaque profil Safari lu, son nom court, le nombre de cookies du domaine et les dates du `sessionid` ; dans l'app, le nombre de cookies de la fenêtre Sessions. Jamais une valeur. Le corps de la réponse n'est jamais cité.

Un `sessionid` créé récemment et refusé quand même veut dire qu'Instagram refuse l'adresse, pas la session : se reconnecter n'y change rien, il faut attendre. Des appels refusés toutes les 10 minutes entretiennent ce blocage, d'où la reprise à 3 h (`reprise`).

Toute autre erreur HTTP devient `Http(code, "instagram")`, sans le corps.

## Ce qui peut casser

- Session refusée ou adresse bloquée : voir plus haut.
- Instagram change l'id de son app web ou ses chemins `/api/v1/` : `Format` ou HTTP.

## Différences avec la version Swift

- **Windows** : le message renvoie à Réglages, Sessions (`safari::se_connecter`) et le `User-Agent` est celui d'Edge WebView2, le navigateur de la session.
- **Session** : la fenêtre Sessions de l'app sert de repli sur Mac et de seule source sous Windows ; la provenance cite aussi ses cookies.
