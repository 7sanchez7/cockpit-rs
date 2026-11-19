# Veille

Fichiers : `crates/noyau/src/sources/veille/mod.rs` (le passage), `veille/parseurs.rs` (le JSON des pages), `veille/instagram.rs` (la recherche Instagram sans session), `veille/analyse.rs` (Claude), `veille/demande.rs` (la demande à Claude), `crates/noyau/src/collecte/hote.rs` (les traits `Hote` et `Navigateur`, le script d'écoute), `crates/app/src/hote.rs` (la page cachée de l'app), `crates/noyau/src/modeles_veille.rs` et `store_veille.rs`. Lectures : `crates/noyau/src/lecture/veille.rs`. id `veille`, cadence 6 h, reprise 3 h après un échec.

Les vidéos qui ont marché sur des mots-clés, sur TikTok et Instagram, sans suivre aucun compte : on donne un mot (« apprendre l'espagnol »), la veille cherche, garde les vidéos au-dessus d'un seuil de vues, découvre leurs comptes, mesure chaque vidéo contre la médiane de son compte, et fait regarder les meilleures par Claude Haiku 5.5.

**Les fournisseurs d'API sont le chemin recommandé.** Une clé `LURKAPI_API_KEY` (principal) ou `TIKHUB_API_KEY` (repli) fait passer la recherche et les médianes de TikTok et d'Instagram par l'API, sans session, sans cookie, sans navigateur caché, partout (app, CLI, `essai`). `collecte/fournisseurs.rs` résout les deux clés une fois par passage, interroge LurkAPI d'abord et bascule sur TikHub si un appel échoue. Sans aucune clé, la veille garde les chemins historiques ci-dessous, dont la recherche TikTok qui ne tourne que dans l'app.

**La recherche TikTok ne tourne que dans l'app sans fournisseur.** Le navigateur caché est fourni par l'hôte (`Hote::navigateur`) ; l'hôte par défaut et `SansInterface` (la CLI, l'exemple `essai`, les tests) rendent `non_configure`, « le navigateur caché n'existe que dans l'app ». TikTok échoue alors, le passage finit en erreur ; Instagram, qui n'a besoin d'aucun navigateur ni d'aucune session (§ Instagram sans session), tourne partout.

## Les fournisseurs

`crates/noyau/src/collecte/lurkapi.rs` (principal) et `tikhub.rs` (repli), réunis par `fournisseurs.rs`. La clé LurkAPI se saisit dans Réglages, onglet Clés, ligne « Clé du scrappeur » : elle est rangée dans la base (réglage `veille.cle_scrappeur`) et passe avant tout le reste. À défaut, chaque clé se lit comme les autres secrets (`ctx.secrets.valeur("LURKAPI_API_KEY", business)`) : variable d'environnement du même nom, trousseau du système, ou Secret Manager du business. LurkAPI se crée sur `lurkapi.com`, TikHub sur `user.tikhub.io/dashboard/api`.

Quand une clé existe, `Fournisseurs::chercher` et `Fournisseurs::dernieres` remplacent, pour la veille et pour la demande à Claude, la page cachée de TikTok, DuckDuckGo et les embeds d'Instagram, et yt-dlp pour la médiane des comptes. Aucun Cookie, aucun accès complet au disque, aucun navigateur : ça marche aussi dans l'app sans session et dans `cargo run -p noyau --example essai -- Veille`.

| | LurkAPI (principal) | TikHub (repli) |
|---|---|---|
| Recherche TikTok | `GET /v1/tiktok/search/keyword` (`query`, `cursor`), 20 vidéos par page | `GET /api/v1/tiktok/app/v3/fetch_general_search_result` (`keyword`, `offset`, `count` 20) |
| Recherche Instagram | `GET /v2/instagram/reels/search` (`query`, `cursor`), 12 reels par page | `GET /api/v1/instagram/v2/search_reels` (`keyword`) |
| Profil TikTok | `GET /v3/tiktok/profile/videos` (`handle`, `max_cursor`), 15 par page | `GET /api/v1/tiktok/app/v3/fetch_user_post_videos_v2` (`unique_id`, `count`) |
| Profil Instagram | `GET /v1/instagram/user/reels` (`username`, `after`), 12 par page | `GET /api/v1/instagram/v2/fetch_user_reels` (`username`) |

LurkAPI répond `{"success":true, …}` et des objets **de forme web** : `parseurs::tiktok` et `parseurs::instagram` les lisent tels quels. Deux retouches dans `lurkapi.rs` avant le parseur : les reels de la recherche reçoivent `media_type` 2, et ceux d'un profil reçoivent `user`, `id` (depuis `pk`, qui porte la date) et les abonnés, parce que l'endpoint ne les met pas sur chaque reel. TikHub, lui, répond `{"code":200,"data":…}` et la forme de l'app TikTok (`aweme_id`, `statistics.play_count`, `author.unique_id`, durée en millisecondes), lue par `parseurs::tiktok_app`.

**Coût** : LurkAPI, 1 crédit par appel, soit ~0,0006 à 0,00093 $ (pack Growth 500 000 crédits à 295 $, ou Pro 42 000 crédits à 39 $ qui n'expirent jamais) ; un passage d'environ 30 appels coûte quelques centimes, de l'ordre de 3 à 4 $ par mois à 4 passages par jour. TikHub, ~0,001 à 0,0025 $ l'appel. Rien n'est appelé quand aucune clé n'est configurée ; la veille ne s'en sert que pour chercher et baser, les vidéos se téléchargent toujours par `yt-dlp`, et l'analyse reste chez Claude (§ L'analyse Claude).

Le chemin sans clé reste décrit ci-dessous pour mémoire et pour le repli à trois niveaux.

## La technique, TikTok

TikTok signe ses appels de recherche (`X-Bogus`) avec un script de la page, et ne répond à `/api/search/item/full/` que pour une session connectée (corps vide sans, essayé en Chromium headless et avec fenêtre, le 19 nov. 2026). Plutôt que de refaire la signature, la veille charge la vraie page dans une WebView hors écran, avec la session de l'utilisateur, et **écoute** les réponses que la page reçoit.

1. `HoteApp::navigateur` (app) crée une `WebviewWindow` libellée `veille-<n>` (`PREFIXE`), 1 280 × 1 800, sans décor, hors de la barre des tâches, sans focus, placée à -30 000 px (`HORS_ECRAN`) : une fenêtre cachée laisserait le système suspendre le JS et le défilement infini ne chargerait rien. Agent : `UA_SAFARI` sur Mac (WebKit), `UA_NAVIGATEUR` ailleurs (WebView2).
2. Le script `SCRIPT_ECOUTE` (noyau, `collecte/hote.rs`) est injecté avant tout script de la page : il enveloppe `fetch` et `XMLHttpRequest`, et chaque réponse dont l'URL contient un des filtres part vers l'app par la commande `plugin:veille|message` (`commandes::message`, plugin `veille` de `main.rs`). La capacité `crates/app/capabilities/veille.json` n'autorise cette commande qu'aux fenêtres `veille-*`, et seulement depuis les pages de TikTok, Instagram et Facebook. `Pages::recevoir` range le texte dans le `Tampon` de la page.
3. Les cookies de session sont posés sur le domaine (`.tiktok.com`, `.instagram.com`) par `set_cookie`. Aucune valeur n'est affichée, écrite ni mise dans une erreur.
4. `Navigateur::lire(url, attente, defilements)` vide le tampon, navigue, attend la fin du chargement (45 s au plus, sinon « page : pas chargée en 45 s »), attend `attente` secondes, défile `defilements` fois (une toutes les 3 s), puis demande à la page ses `<script type="application/json">` qui contiennent `play_count`, `playCount` ou `like_count` (`SCRIPT_BALISES`, Instagram y met ses premiers résultats). Il rend les corps capturés plus ces balises.
5. `Navigateur::ou` rend le titre et le chemin de la page (connexion, captcha) pour le message d'erreur ; `fermer` détruit la fenêtre.

D'où vient la session (`Hote::cookies`, `HoteApp::cookies`) : sur Mac, les cookies de Safari (`collecte/safari.rs`, fichiers `Cookies.binarycookies`, accès complet au disque requis), puis à défaut ceux de la fenêtre de connexion de l'app ; ailleurs, seulement la fenêtre de connexion. Cette fenêtre s'ouvre par la commande `session(site)` (`crates/app/src/commandes.rs`, `SITES` : tiktok, instagram, facebook) : une WebView « Se connecter à … » dont le magasin de cookies est celui de toutes les WebViews de l'app. Le cookie requis est `sessionid`.

| | TikTok | Instagram |
|---|---|---|
| Recherche | page cachée sur `https://www.tiktok.com/search/video?q=<mot>` (`recherche`), `lire(url, 6, N / 5)`, session `sessionid` | DuckDuckGo lite puis l'embed de chaque reel (`instagram::chercher`), sans session ni navigateur |
| Profil, pour la médiane (`dernieres`) | `yt-dlp`, 20 vidéos au plus, sur `https://www.tiktok.com/@<compte>` (`ytdlp::liste`, sans session) ; les abonnés viennent de la page publique du compte (`abonnes_tiktok`, `tiktok::profil`), yt-dlp ne les donne pas | `https://www.instagram.com/<compte>/reels/` dans la page cachée, `lire(url, 5, 1)`, filtré sur le compte car la page charge aussi le fil d'accueil ; **session seulement** : sans session, `Erreur::NonConfigure`, `baser_comptes` saute la plateforme sans erreur |
| Filtres (`ouvrir`) | `/api/search/` | `/api/v1/`, `/graphql` (pour la page Reels) |

Les parseurs (`parseurs::trouvailles`) ne suivent pas un chemin fixe : `objets` découpe chaque corps (une ou plusieurs lignes JSON), `parcourir` visite tout l'arbre, et `parseurs::tiktok` ou `parseurs::instagram` reconnaissent un objet vidéo à sa forme. Une même vidéo vue deux fois est fusionnée (`Trouvaille::fusion`, compteurs au plus haut). `trouvailles_liste` convertit une liste yt-dlp.

## Instagram sans session

La recherche d'Instagram exige une session et ses jetons de page ; on la contourne en deux temps, sans cookie (`veille/instagram.rs`) :

1. **Découverte** : `POST https://lite.duckduckgo.com/lite/` avec `q=site:instagram.com/reel <mot>` et `df=m` (le dernier mois). Une page de résultats, une dizaine de reels triés par pertinence, pas par date (`lire_codes` : les liens `/reel/<code>/`, `/<compte>/reel/<code>/`, `/reels/<code>/` et `/p/<code>/`, une fois chacun, les pubs `duckduckgo.com/y.js` écartées). Aucun lien : « DuckDuckGo n'a donné aucun reel (limite atteinte ou mot sans résultat) ».
2. **Détail** : `GET https://www.instagram.com/reel/<code>/embed/captioned/` pour chaque code, l'un après l'autre. La page porte `"contextJSON":"…"` (un JSON dans une chaîne JSON, `lire_embed`), lu par `parseurs::instagram` : compte, abonnés (`edge_followed_by`), vues (`video_view_count`), likes (`edge_liked_by`), commentaires, légende, miniature. **La date vient de l'id** du média : `((id >> 23) + 1 314 220 021 721) / 1000` (`date_de_l_id`), l'embed n'en donne pas. Un 429 arrête les embeds suivants.

Limites à connaître : une dizaine de reels par mot, quel que soit le curseur `veille.videos` ; ni partages ni enregistrements (Instagram ne les expose pas) ; `video_view_count` parfois nul, et très bas sur les reels de moins de deux jours, d'où le repli de `garde` sur les likes (`vues_min / 50`) ; DuckDuckGo répond 202 quand on insiste (limite), le reel suivant attend alors le passage suivant. Essayé de bout en bout le 19 nov. 2026 sur « apprendre l'espagnol » : 10 reels complets en 7 s.

**Ce qu'on ne fait pas** : résoudre un captcha, se connecter à la place de l'utilisateur, aller plus vite qu'un humain. Au plus 4 mots (`MOTS_PAR_PASSAGE`) et 8 profils (`COMPTES_PAR_PASSAGE`) par plateforme et par passage, un passage toutes les 6 h. Entre deux pages cachées 20 à 40 s (`PAUSE`), entre deux requêtes sans page 3 à 6 s (`PAUSE_SANS_PAGE`). Un captcha ou une page de connexion devant TikTok donne « aucune réponse capturée (page : <titre, chemin>) ».

## Le passage

1. Les mots actifs des business suivis (`Store::mots_veille`, table `veille_mot`), triés par `cherche` (jamais cherché d'abord), les 4 premiers. Aucun mot : `non_configure`, « aucun mot-clé de veille, en ajouter dans l'écran Veille ».
2. `yt-dlp` mis à jour (`ytdlp::tenir_a_jour`, § yt-dlp nightly ; un échec s'ajoute aux échecs du passage sans l'arrêter).
3. TikTok et Instagram **en même temps** (`passer`, réunies par `join_all`). Avec LurkAPI ou TikHub, aucune page n'est ouverte : `chercher` et `dernieres` appellent l'API (§ Les fournisseurs), sans pause. Sans clé, TikTok ouvre sa page cachée (`ouvrir`, sans session : seule sa médiane passe), Instagram n'ouvre rien ; puis, par plateforme :
   - chaque mot : sans fournisseur, TikTok défile la recherche N / 5 fois (`VIDEOS_PAR_DEFILEMENT`), N étant le curseur de l'écran Veille (`veille.videos`, `videos_par_mot`, de 10 à 100 par pas de 10, 40 par défaut, lu au début du passage) ; Instagram, `instagram::chercher`. Aucune des deux ne trie par date : on lit large, puis `retenir` garde les N plus récentes parmi celles que `garde` accepte (vues ≥ `vues_min` du mot, ou, quand la recherche ne donne pas les vues (Instagram), likes ≥ `vues_min / 50`), les vidéos sans date en dernier. Chaque vidéo gardée écrit `veille_video`, son relevé et son compte (`Store::trouvaille`), dans un `store.lot` ;
   - les 8 comptes sans médiane ou dont la médiane a plus de 7 jours, ceux des plus grosses vidéos d'abord (`Store::comptes_a_baser`) : leurs dernières vidéos (`dernieres`, un fournisseur, sinon yt-dlp et page), la médiane (`ScoreVeille::mediane`), `Store::base` et un nouveau relevé des vidéos déjà en base (`Store::relever`). Sans fournisseur, Instagram (session seulement) fait une pause avant chaque profil, TikTok non (yt-dlp ne passe pas par la page).
4. `Store::cherche` met `cherche` à maintenant pour les mots du passage.
5. Les **percées** : une vidéo publiée il y a moins de 48 h que `ScoreVeille::percee` retient écrit une ligne du fil `veille:<id>`, type `VEILLE`, avec le lien de la vidéo. Texte (`percee`) : « @compte · 120 k vues, 6× sa médiane · « mot » ».
6. Les miniatures absentes du cache (`miniatures::telecharger`, clé = id de la vidéo), puis l'effacement des vidéos téléchargées depuis plus de 14 jours (`analyse::nettoyer_videos`).
7. L'analyse Claude (`analyse::analyser`).

Un mot, un profil ou une analyse en échec n'arrête pas le reste ; le passage finit en erreur après avoir tout écrit (« N échecs : a ; b ; … »). Le nombre de lignes rendu compte les vidéos gardées et les comptes basés.

## yt-dlp nightly

TikTok casse yt-dlp en quelques semaines : la stable 2026.10.26 échoue sur les profils (« Failed to parse JSON ») là où le nightly 2026.11.17 les lit, sans session. `ytdlp::tenir_a_jour` garde donc un nightly dans `chemins::outils()` (`<données>/bin/yt-dlp[.exe]`, `~/Library/Application Support/Cockpit/bin` sur Mac), téléchargé depuis `https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/latest/download/<fichier>` (`yt-dlp_macos`, `yt-dlp.exe`, `yt-dlp_linux`, `yt-dlp_linux_aarch64`, environ 37 Mo sur Mac), renouvelé quand il a plus de 3 jours (`FRAICHEUR`). Il est écrit à côté sous un autre nom, doit répondre à `--version`, puis remplace l'ancien : un binaire tronqué ne casse rien. `commande::trouver` cherche ce dossier avant le PATH. Appelé au début de chaque passage de la Veille et de TikTok ; un verrou évite deux téléchargements à la fois. Pas de téléchargement sur un système sans fichier correspondant.

## Le score

Une vidéo compte par rapport à **son compte**, pas dans l'absolu (`ScoreVeille`, `modeles_veille.rs`) :

- **médiane** du compte : ses 20 dernières vidéos (`VIDEOS_BASE`), sans celles de moins de 48 h (`JEUNE_S`) ; nulle sous 5 vidéos (`MIN_BASE`). Une vidéo sans date (reels Instagram) est comptée ;
- **ratio** = vues / médiane ;
- **pull** = (4 × partages + 4 × enregistrements + 2 × commentaires + likes) / vues ;
- **outlier** : ratio ≥ 3 (`RATIO_OUTLIER`), vues ≥ 10 000 (`VUES_OUTLIER`) et pull ≥ 0,5 % (`PULL_OUTLIER`) ;
- **percée** : ratio ≥ 2 (`RATIO_PERCEE`) sur une vidéo jeune ;
- **vitesse** : vues gagnées par heure entre les deux derniers relevés, s'ils sont à au moins une heure d'écart (`VideoVeille::depuis_ligne`).

Tout est calculé à la lecture, rien n'est stocké : changer un seuil change tout l'historique.

## L'analyse Claude

Les vidéos trouvées dans les 14 derniers jours sans analyse, outliers ou à au moins 2× la médiane de leur compte (`VideoVeille::a_analyser`), les plus récemment publiées d'abord puis les plus gros ratios, 10 par passage (`ANALYSES_PAR_PASSAGE`, `analyse::a_analyser`).

- Clé : `ctx.secrets.valeur(&b.config.claude, b)`, le secret `ANTHROPIC_API_KEY` par défaut (champ `claude` de la config, voir [../collecte.md](../collecte.md)). Pas de clé (`non_configure`) : pas d'analyse, sans erreur.
- Modèle : `claude-haiku-5-5` (`HAIKU`), `thinking` désactivé, `effort: low`, 2 000 tokens de sortie au plus, délai de 120 s.
- Claude ne lit pas la vidéo : on lui envoie **des images clés et ce qui est dit** (`extraits`). La vidéo vient de l'URL du fichier quand la page l'a donnée (Instagram, elle expire vite), sinon de `yt-dlp -f 'best[height<=720]/best'`, jusqu'à 3 essais car le CDN de TikTok rend parfois un 403 (`fichier`). Elle est gardée dans `<cache>/veille-videos/`, où le lecteur de l'écran la reprend (`lire_video`, qui la télécharge de la même façon si elle manque) (`analyse::videos()`, cache de [`chemins::cache()`](../architecture.md)), écrite en `.part` puis renommée. `ffmpeg` en tire 6 images en 480 px de haut (`moments` : 0,3, 1,5 et 3 s pour le hook, puis 35, 60 et 85 % de la durée lue par `ffprobe`, 30 s si elle manque), et `transcrire --langue auto --plat` le texte dit, coupé à 4 000 caractères ; sans `transcrire`, l'analyse se fait sur les images seules. Sans aucune image : « aucune image tirée de la vidéo ».
- Réponse : `output_config.format` en `json_schema` (`schema`), lue par `lire` : un `stop_reason` autre que `end_turn` donne « Claude sans réponse », un `adaptable` hors de 0 à 3 est une erreur. Le champ `format` est l'un des formats de `<contenu_dir>/formats/INDEX.md` (lignes `| 3 | [nom](nom/) | …`, `formats_index`) ou « nouveau ».
- **Coût et plafond** : chaque appel écrit ses tokens et son coût dans `veille_cout` (`Store::cout_veille` par `facturer`, 0,10 $ le million en entrée, 0,50 $ en sortie, cinq fois plus au-delà de 100 000 tokens de prompt, `Usage::usd`) avant de lire la réponse. Au-delà de 50 $ cumulés, demandes comprises (`lecture::veille::PLAFOND_VEILLE_USD`, `Store::depense_veille_usd`), l'analyse s'arrête : « analyses arrêtées : N $ dépensés, plafond 50 $ ». `cockpit etat` affiche la dépense ([../cli.md](../cli.md)).

`AnalyseVeille` (mêmes clés JSON que la version macOS, dans `veille_analyse`) : `hook` (mot pour mot), `hookType` (l'un des 16 `AnalyseVeille::HOOKS`), `hookVisuel` (l'un des 12 `VISUELS`), `texteEcran`, `probleme`, `format`, `blocs` (hook, confirmation, valeur, placement, fin), `parts` (part de la durée de chaque bloc en %, total 100, 0 pour un bloc absent ; facultatif, absent des analyses de la version macOS et des plus anciennes, l'anatomie de l'écran se dessine alors à parts égales), `ecart`, `adaptable` (0 à 3), `idee`.

## Les mots-clés

`Store::ajouter_mot(business, mot, vues_min)` (le mot est rogné, un mot vide est ignoré, un mot existant est réactivé avec le nouveau seuil), `retirer_mot` (mot rogné aussi), `activer_mot`. L'écran Veille les appelle (pilules de mots-clés, [../ecrans.md](../ecrans.md) § Veille), la CLI aussi : `cockpit veille mot ajoute` et `cockpit veille mot retire` ([../cli.md](../cli.md)).

## La demande

Une consigne libre tapée dans l'écran Veille (« Trouve des vidéos scriptées qui font la promo d'une app d'apprentissage ») : Claude choisit les recherches, les fait, regarde les vidéos et rapporte celles qui répondent. `demande::creer` l'écrit dans `veille_demande` (consigne rognée, vide ou au-delà de 2 000 caractères refusée, `PROMPT_MAX`), la commande `demander_veille` de l'app la mène en fond par `demande::mener`. Elle réserve l'id `veille` du collecteur (`Collecteur::reserver`) : une demande et un passage ne tournent jamais ensemble, « La veille tourne déjà, réessaie quand elle a fini. » La demande ne tourne que dans l'app, comme la veille, et prend la clé Claude du business comme l'analyse.

1. **Les mots-clés, par Sonnet 5.5** (`SONNET`, `corps_mots`) : un seul appel, `effort: medium`, réponse en `json_schema` (`schema_mots`) : de 4 à 8 recherches (`MOTS_MAX`), chacune avec sa plateforme et une phrase de pourquoi. La consigne (`consigne_mots`) donne la demande, le business et les mots déjà suivis. Sonnet 5.5 refuse `thinking: disabled` : le champ est omis, sa réflexion reste adaptative. `lire_mots` écarte les vides, les doublons (même plateforme, casse ignorée) et les plateformes inconnues ; aucun mot : « Sonnet n'a proposé aucun mot-clé ».
2. **La boucle, par Haiku 5.5** (`HAIKU`, `corps_boucle`, `systeme`) : la demande et la méthode en system, les mots de Sonnet en premier message, puis une boucle d'outils menée à la main, 30 tours au plus (`TOURS_MAX`), 8 000 tokens de sortie et 180 s par appel. `cache_control` est posé à la racine : chaque tour relit le précédent au prix du cache. Les outils (`outils`) :
   - `chercher(mot, plateforme, vues_min)` : la même recherche qu'un passage (`chercher` de `veille/mod.rs` : la page cachée de TikTok, ouverte à sa première recherche ; DuckDuckGo et les embeds pour Instagram ; `pause` entre deux recherches d'un même site), N vidéos les plus récentes au-dessus de `vues_min` (10 000 par défaut, `VUES_MIN`), N étant le curseur `veille.videos`. Les vidéos gardées s'écrivent dans `veille_video` sous ce mot, comme celles d'un passage, et leurs miniatures sont téléchargées. Haiku reçoit une fiche par vidéo (`fiche` : id, compte, date, compteurs, ratio, durée, légende coupée à 160 caractères, `analysee`). 8 recherches au plus (`CHERCHES_MAX`) ;
   - `analyser(id)` : l'analyse d'un passage (`analyse::analyser_une`), écrite dans `veille_analyse` ; une vidéo déjà analysée rend son analyse sans appel. 10 au plus (`ANALYSES_MAX`) ;
   - `autres_mots(raison)` : relance Sonnet avec les recherches déjà faites et ce qui manque, 2 fois au plus (`AUTRES_MOTS_MAX`) ;
   - `retenir(id, raison)` : écrit `veille_retenue` (`Store::retenir_video`) ; seulement une vidéo déjà en base.
   Un outil en échec rend `is_error` à Haiku, qui continue ; la ligne part au journal. Une réponse sans outil est la synthèse (3 à 6 phrases), écrite par `finir_demande`.
3. Chaque étape écrit une ligne au journal (`noter_demande`) et réveille l'écran (`a_change`) : mots-clés, recherches et leur nombre de vidéos, analyses, retenues, échecs.

**Coût** : chaque appel s'écrit dans `veille_cout` sous `demande:<id>` (`facturer`), au prix de son modèle et de chaque sorte de token (`Prix`, `Usage::usd`) : Sonnet 2 $ et 10 $ le million (cache écrit 2,50 $, lu 0,10 $), Haiku 0,10 $ et 0,50 $ (cache écrit 0,125 $, lu 0,01 $). Avant chaque appel, le plafond de 50 $ de la veille est vérifié (`plafond`) : « plafond atteint : N $ dépensés sur 50 $ ». L'écran montre `cout_demande`, la somme de `veille_cout` entre le début et la fin de la demande. Une demande sans analyse coûte environ 0,01 $.

**Interrompue** : l'app quittée en route laisse la demande `en_cours` ; l'écran la montre en échec, « interrompue », et la suivante l'écrit ainsi en base (`Store::nouvelle_demande`).

## Ce qui peut casser

- **Avec un fournisseur** : LurkAPI d'abord, TikHub en repli. Une clé absente, invalide ou sans crédit fait répondre une erreur (`success` faux, `code` TikHub différent de 200, ou HTTP) : l'appel passe au fournisseur suivant, et si les deux échouent la recherche (ou le profil) du mot s'ajoute aux échecs du passage, les autres mots continuent. Aucune clé n'est une panne : la veille retombe sur les chemins historiques. L'API change un chemin ou un nom de champ : mettre à jour `lurkapi.rs`, `tikhub.rs` ou `parseurs::tiktok_app`, avec un exemple figé dans leurs tests. Les vidéos se téléchargent toujours par `yt-dlp`, donc une casse de `yt-dlp` n'empêche que l'analyse, pas la découverte.
- Sur Mac, l'app sans accès complet au disque (Réglages Système > Confidentialité et sécurité) ne lit pas les cookies de Safari : « Safari illisible », et chaque recherche TikTok, d'un passage ou d'une demande, échoue (celle d'Instagram n'a pas besoin de Safari). Sans fournisseur seulement. Un bundle signé ad hoc (`cargo tauri build --debug`) change de signature à chaque build et perd cet accès ; signé par un certificat Apple Development, il le garde ([`runbook.md`](../runbook.md) § Installer).
- Pas de session (Safari expiré sur Mac, fenêtre de connexion jamais ouverte ailleurs) : `Hote::cookies` rend `non_configure`, la recherche TikTok est sautée avec son message, Instagram continue ; la médiane TikTok passe quand même par yt-dlp, celle d'Instagram est sautée en silence. Sans fournisseur seulement.
- La page change ses chemins d'API : plus rien ne passe les filtres, « aucune réponse capturée ». Regarder les requêtes de la page dans un navigateur, mettre à jour les filtres dans `ouvrir`. Sans fournisseur seulement.
- La page change la forme de ses objets : ajouter le nouveau nom de champ dans `parseurs::tiktok`, `parseurs::tiktok_app` ou `parseurs::instagram`, et un exemple figé dans leurs tests.
- yt-dlp ne lit plus les profils TikTok : le nightly est renouvelé tous les 3 jours (§ yt-dlp nightly) ; si GitHub est injoignable, l'échec est dans le message du passage et l'ancien binaire reste utilisé. L'erreur de chaque compte est dans le même message. Sans fournisseur seulement.
- DuckDuckGo ne rend plus de reels (202, page changée) ou l'embed d'Instagram change de forme (`contextJSON`) : la recherche Instagram échoue avec « DuckDuckGo n'a donné aucun reel » ou « aucun des N reels n'est lisible dans son embed ». Les exemples figés sont dans les tests de `instagram.rs` et de `parseurs.rs`. Sans fournisseur seulement.
- Instagram bloque l'adresse : la reprise de 3 h évite d'entretenir le blocage. Sans fournisseur seulement.
- `ffmpeg`, `ffprobe` ou `yt-dlp` absent : l'analyse échoue, retentée au passage suivant ; `transcrire` absent : analyse sur les images.
- Plafond de 50 $ atteint : le relever dans `lecture::veille::PLAFOND_VEILLE_USD` (lu par la source, la CLI et l'écran) seulement sur décision de l'utilisateur.
