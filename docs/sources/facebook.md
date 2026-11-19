# Facebook

Fichier : `crates/noyau/src/sources/facebook.rs`, la session par l'hôte (`crates/noyau/src/collecte/hote.rs`). Tests : dans `facebook.rs` (`mod tests`). Id : `facebook`. Cadence : 3 h. Aucune clé : la session `facebook.com`.

Les vidéos des Pages Facebook suivies (table `compte`, [`../donnees.md`](../donnees.md) ; le handle est le nom de la Page ou son id), avec leurs vraies stats : temps de visionnage, spectateurs, abonnés gagnés. Lues dans le tableau Statistiques, Contenu de Meta Business Suite, par son API interne.

Aucune Page active dans les business suivis : `NonConfigure` « aucune Page Facebook suivie, en ajouter dans Vidéos ».

## La session et les jetons

`ctx.hote.cookies("facebook.com", "c_user")`, même lecture que pour TikTok ([`tiktok.md`](tiktok.md) § La session). Sans `c_user` : `NonConfigure`, la source passe en gris. `c_user` est l'id de l'utilisateur, envoyé en `av` et `__user`.

Puis la page `https://business.facebook.com/latest/home?asset_id=<page>`, ouverte comme par le navigateur (`page` : `UA_NAVIGATEUR`, en-têtes `Sec-Fetch-*` d'une navigation), d'où l'on lit trois jetons (`jetons`) : `fb_dtsg` (`DTSGInitialData`, sinon `dtsg.token`), `lsd` (`LSD`) et `__rev` (`server_revision`, sinon `__spin_r`). Sans eux, la session a expiré : `NonConfigure` « session Facebook de Safari expirée, se reconnecter sur business.facebook.com dans Safari ».

Les cookies, les jetons et le HTML de Business Suite ne sont jamais affichés, écrits ni mis dans une erreur : `appel` remplace le corps d'une erreur HTTP par `facebook`.

## La Page

Un handle tout en chiffres est l'id de la Page (`page_id`). Sinon, la page publique `https://www.facebook.com/<handle>` donne `delegate_page_id` (ou un `asset_id=`). Ni l'un ni l'autre : `Format` « Page monapp introuvable, ou session Facebook de Safari expirée ». La page d'accueil de Business Suite donne les abonnés : `page_total_likes_or_followers.count`, un `compte_releve`.

## Le tableau Contenu

```
POST https://business.facebook.com/api/graphql/
fb_api_req_friendly_name=BizWebInsightsContentOrganicTableQueryRendererQuery
doc_id=40040115705602765
variables={"ids": [<page>], "timeRange": {"type": "LAST_90D"}, "count": 100, …}
```

Corps en `application/x-www-form-urlencoded` (`requete`), avec `X-FB-LSD`, `X-FB-Friendly-Name` et `X-ASBD-ID`. La réponse commence par `for (;;);` et met plusieurs objets JSON bout à bout (`objets` les sépare ; un morceau illisible : `Format` « graphql : morceau illisible »). Les lignes sont sous `data.tofu_unified_table.content.edges[].node` (`table`) : les publications des 90 derniers jours. Gardées : `content_type` qui contient VIDEO, ou une ligne qui a un temps de visionnage. Aucune ligne et une erreur graphql : `Format` « graphql : <message> ».

Chaque case est `fields.<clé>.renderer.value_for_sort` ; une case dont le `__typename` contient « Error » est absente, pas à zéro.

| Clé | Colonne ou champ |
|---|---|
| `views` | `vues` (sans elle, pas de relevé ni d'analyse) |
| `net_reactions`, `net_comments`, `shares`, `net_saves` | `likes`, `commentaires`, `partages`, `enregistrements` |
| `new_follows` | `abonnes` |
| `video_average_play_time` (ms) | `watch_moyen_s` |
| `video_play_time` (ms) | `Analyse.watch_total_s` |
| `viewers` | `Analyse.spectateurs` |
| `video_three_second_views`, `video_one_minute_views`, `video_thruplays` | `Analyse.jalons` (3 s, 1 min, ThruPlay), en part des vues |

Sous `header.entity.entity_info` : `title` (la légende), `created_at`, `image_source.uri` (miniature, petite) et `owner.entity_info.image_source.uri` (l'avatar de la Page). L'id de la vidéo est `facebook:<row_id>`, son lien `https://www.facebook.com/<row_id>`.

## Ce que la source fait

Pour chaque Page : id, accueil (abonnés et jetons), tableau. Puis les images absentes du cache et la première image des vidéos qui ne l'ont pas encore (`miniatures::extraire_premieres`, `crates/noyau/src/collecte/miniatures.rs`, 960 px de haut au plus). Puis un seul `lot` : chaque vidéo et sa ligne du fil `facebook:pub:<id>` (« Nouvelle vidéo sur Facebook monapp : <60 premiers caractères de la légende> »), `video_releve`, `video_analyse` des analyses non vides, `compte_releve`.

Une Page en échec n'arrête pas les autres. Le passage finit en erreur après avoir écrit le reste (« N échecs, dont monapp : … »). La source rend le nombre de vidéos, de relevés, d'analyses et de profils écrits.

## Ce qui peut casser

- Session expirée : plus de jetons dans la page. Le `NonConfigure` est rangé dans la liste des échecs comme une autre erreur, et `sans_echec` en fait une erreur `Format` : la source finit en rouge, pas en gris. La version Swift faisait de même.
- Meta change la requête : un nouveau `doc_id` (`DOC_TABLE`, relu dans l'onglet Réseau de la page Statistiques, Contenu de Business Suite), ou des clés renommées. Une erreur graphql sort telle quelle.
- Les miniatures du tableau sont petites : la grille montre la première image extraite, la miniature seulement en attendant.

## Différences avec la version Swift

- **Messages** : « session Facebook de Safari expirée » cite Safari même sous Windows, où la session vient de la fenêtre Sessions.
- **Session** : Safari puis la fenêtre Sessions de l'app sur Mac, la fenêtre Sessions seule sous Windows ([`../collecte.md`](../collecte.md) § Sessions).
- **Programmes** : `yt-dlp` et `ffmpeg`, pour la première image, sont trouvés par `commande::trouver`, plus par un zsh de login.
