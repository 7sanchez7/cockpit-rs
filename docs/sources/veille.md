# Veille

Fichiers : `crates/noyau/src/sources/veille/mod.rs` (le passage), `veille/parseurs.rs` (le JSON des pages), `veille/analyse.rs` (Claude), `crates/noyau/src/collecte/hote.rs` (les traits `Hote` et `Navigateur`, le script d'écoute), `crates/app/src/hote.rs` (la page cachée de l'app), `crates/noyau/src/modeles_veille.rs` et `store_veille.rs`. Lectures : `crates/noyau/src/lecture/veille.rs`. id `veille`, cadence 6 h, reprise 3 h après un échec.

Les vidéos qui ont marché sur des mots-clés, sur TikTok et Instagram, sans suivre aucun compte : on donne un mot (« apprendre l'espagnol »), la veille cherche, garde les vidéos au-dessus d'un seuil de vues, découvre leurs comptes, mesure chaque vidéo contre la médiane de son compte, et fait regarder les meilleures par Claude Haiku 5.5.

**Elle ne tourne que dans l'app.** Le navigateur caché est fourni par l'hôte (`Hote::navigateur`) ; l'hôte par défaut et `SansInterface` (la CLI, l'exemple `essai`, les tests) rendent `non_configure`, « le navigateur caché n'existe que dans l'app ». Les deux plateformes échouent alors, le passage finit en erreur.

## La technique

TikTok signe ses appels de recherche (`X-Bogus`) avec un script de la page ; Instagram exige ses jetons de page (`doc_id`) et bloque les appels directs. Plutôt que de refaire ces signatures, la veille charge la vraie page dans une WebView hors écran, avec la session de l'utilisateur, et **écoute** les réponses que la page reçoit.

1. `HoteApp::navigateur` (app) crée une `WebviewWindow` libellée `veille-<n>` (`PREFIXE`), 1 280 × 1 800, sans décor, hors de la barre des tâches, sans focus, placée à -30 000 px (`HORS_ECRAN`) : une fenêtre cachée laisserait le système suspendre le JS et le défilement infini ne chargerait rien. Agent : `UA_SAFARI` sur Mac (WebKit), `UA_NAVIGATEUR` ailleurs (WebView2).
2. Le script `SCRIPT_ECOUTE` (noyau, `collecte/hote.rs`) est injecté avant tout script de la page : il enveloppe `fetch` et `XMLHttpRequest`, et chaque réponse dont l'URL contient un des filtres part vers l'app par la commande `plugin:veille|message` (`commandes::message`, plugin `veille` de `main.rs`). La capacité `crates/app/capabilities/veille.json` n'autorise cette commande qu'aux fenêtres `veille-*`, et seulement depuis les pages de TikTok, Instagram et Facebook. `Pages::recevoir` range le texte dans le `Tampon` de la page.
3. Les cookies de session sont posés sur le domaine (`.tiktok.com`, `.instagram.com`) par `set_cookie`. Aucune valeur n'est affichée, écrite ni mise dans une erreur.
4. `Navigateur::lire(url, attente, defilements)` vide le tampon, navigue, attend la fin du chargement (45 s au plus, sinon « page : pas chargée en 45 s »), attend `attente` secondes, défile `defilements` fois (une toutes les 3 s), puis demande à la page ses `<script type="application/json">` qui contiennent `play_count`, `playCount` ou `like_count` (`SCRIPT_BALISES`, Instagram y met ses premiers résultats). Il rend les corps capturés plus ces balises.
5. `Navigateur::ou` rend le titre et le chemin de la page (connexion, captcha) pour le message d'erreur ; `fermer` détruit la fenêtre.

D'où vient la session (`Hote::cookies`, `HoteApp::cookies`) : sur Mac, les cookies de Safari (`collecte/safari.rs`, fichiers `Cookies.binarycookies`, accès complet au disque requis), puis à défaut ceux de la fenêtre de connexion de l'app ; ailleurs, seulement la fenêtre de connexion. Cette fenêtre s'ouvre par la commande `session(site)` (`crates/app/src/commandes.rs`, `SITES` : tiktok, instagram, facebook) : une WebView « Se connecter à … » dont le magasin de cookies est celui de toutes les WebViews de l'app. Le cookie requis est `sessionid`.

| | TikTok | Instagram |
|---|---|---|
| Recherche (`recherche`) | `https://www.tiktok.com/search/video?q=<mot>` | `https://www.instagram.com/explore/search/keyword/?q=<mot>` |
| Lecture | `lire(url, 6, 3)` | `lire(url, 6, 3)` |
| Profil, pour la médiane (`dernieres`) | `yt-dlp`, 20 vidéos au plus, sur `https://www.tiktok.com/@<compte>` (`ytdlp::liste`, sans session) | `https://www.instagram.com/<compte>/reels/` dans la page cachée, `lire(url, 5, 1)`, filtré sur le compte car la page charge aussi le fil d'accueil |
| Filtres (`ouvrir`) | `/api/search/` | `/api/v1/`, `/graphql` |

Les parseurs (`parseurs::trouvailles`) ne suivent pas un chemin fixe : `objets` découpe chaque corps (une ou plusieurs lignes JSON), `parcourir` visite tout l'arbre, et `parseurs::tiktok` ou `parseurs::instagram` reconnaissent un objet vidéo à sa forme. Une même vidéo vue deux fois est fusionnée (`Trouvaille::fusion`, compteurs au plus haut). `trouvailles_liste` convertit une liste yt-dlp.

**Ce qu'on ne fait pas** : résoudre un captcha, se connecter à la place de l'utilisateur, aller plus vite qu'un humain. Au plus 4 mots (`MOTS_PAR_PASSAGE`) et 8 profils (`COMPTES_PAR_PASSAGE`) par plateforme et par passage, 20 à 40 s entre deux pages (`PAUSE`), un passage toutes les 6 h. Un captcha ou une page de connexion donne « aucune réponse capturée (page : <titre, chemin>) ».

## Le passage

1. Les mots actifs des business suivis (`Store::mots_veille`, table `veille_mot`), triés par `cherche` (jamais cherché d'abord), les 4 premiers. Aucun mot : `non_configure`, « aucun mot-clé de veille, en ajouter dans l'écran Veille ».
2. TikTok et Instagram **en même temps** (`passer`, réunies par `join_all`) : chaque plateforme ouvre sa page (`ouvrir`), puis `parcourir` :
   - chaque mot : la recherche, puis `garde` : vues ≥ `vues_min` du mot, ou, quand la recherche ne donne pas les vues (Instagram), likes ≥ `vues_min / 50`. Chaque vidéo gardée écrit `veille_video`, son relevé et son compte (`Store::trouvaille`), dans un `store.lot` ;
   - les 8 comptes sans médiane ou dont la médiane a plus de 7 jours, ceux des plus grosses vidéos d'abord (`Store::comptes_a_baser`) : leurs dernières vidéos (`dernieres`), la médiane (`ScoreVeille::mediane`), `Store::base` et un nouveau relevé des vidéos déjà en base (`Store::relever`). Instagram fait une pause avant chaque profil, TikTok non (yt-dlp ne passe pas par la page).
3. `Store::cherche` met `cherche` à maintenant pour les mots du passage.
4. Les **percées** : une vidéo publiée il y a moins de 48 h que `ScoreVeille::percee` retient écrit une ligne du fil `veille:<id>`, type `VEILLE`, avec le lien de la vidéo. Texte (`percee`) : « @compte · 120 k vues, 6× sa médiane · « mot » ».
5. Les miniatures absentes du cache (`miniatures::telecharger`, clé = id de la vidéo), puis l'effacement des vidéos téléchargées depuis plus de 14 jours (`analyse::nettoyer_videos`).
6. L'analyse Claude (`analyse::analyser`).

Un mot, un profil ou une analyse en échec n'arrête pas le reste ; le passage finit en erreur après avoir tout écrit (« N échecs : a ; b ; … »). Le nombre de lignes rendu compte les vidéos gardées et les comptes basés.

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

Seulement les outliers trouvés dans les 14 derniers jours sans analyse, les plus gros ratios d'abord, 3 par passage (`ANALYSES_PAR_PASSAGE`).

- Clé : `ctx.secrets.valeur(&b.config.claude, b)`, le secret `ANTHROPIC_API_KEY` par défaut (champ `claude` de la config, voir [../collecte.md](../collecte.md)). Pas de clé (`non_configure`) : pas d'analyse, sans erreur.
- Modèle : `claude-haiku-5-5` (`MODELE`), `thinking` désactivé, `effort: low`, 2 000 tokens de sortie au plus, délai de 120 s.
- Claude ne lit pas la vidéo : on lui envoie **des images clés et ce qui est dit** (`extraits`). La vidéo vient de l'URL du fichier quand la page l'a donnée (Instagram, elle expire vite), sinon de `yt-dlp -f 'best[height<=720]/best'`, jusqu'à 3 essais car le CDN de TikTok rend parfois un 403 (`fichier`). Elle est gardée dans `<cache>/veille-videos/` (`analyse::videos()`, cache de [`chemins::cache()`](../architecture.md)), écrite en `.part` puis renommée. `ffmpeg` en tire 6 images en 480 px de haut (`moments` : 0,3, 1,5 et 3 s pour le hook, puis 35, 60 et 85 % de la durée lue par `ffprobe`, 30 s si elle manque), et `transcrire --langue auto --plat` le texte dit, coupé à 4 000 caractères ; sans `transcrire`, l'analyse se fait sur les images seules. Sans aucune image : « aucune image tirée de la vidéo ».
- Réponse : `output_config.format` en `json_schema` (`schema`), lue par `lire` : un `stop_reason` autre que `end_turn` donne « Claude sans réponse », un `adaptable` hors de 0 à 3 est une erreur. Le champ `format` est l'un des formats de `<contenu_dir>/formats/INDEX.md` (lignes `| 3 | [nom](nom/) | …`, `formats_index`) ou « nouveau ».
- **Coût et plafond** : chaque appel écrit ses tokens et son coût dans `veille_cout` (`Store::cout_veille`, 0,10 $ le million en entrée, 0,50 $ en sortie, `cout`) avant de lire la réponse. Au-delà de 50 $ cumulés (`lecture::veille::PLAFOND_VEILLE_USD`, `Store::depense_veille_usd`), l'analyse s'arrête : « analyses arrêtées : N $ dépensés, plafond 50 $ ». `cockpit etat` affiche la dépense ([../cli.md](../cli.md)).

`AnalyseVeille` (mêmes clés JSON que la version macOS, dans `veille_analyse`) : `hook` (mot pour mot), `hookType` (l'un des 16 `AnalyseVeille::HOOKS`), `hookVisuel` (l'un des 12 `VISUELS`), `texteEcran`, `probleme`, `format`, `blocs` (hook, confirmation, valeur, placement, fin), `ecart`, `adaptable` (0 à 3), `idee`.

## Les mots-clés

`Store::ajouter_mot(business, mot, vues_min)` (le mot est rogné, un mot vide est ignoré, un mot existant est réactivé avec le nouveau seuil), `retirer_mot` (mot rogné aussi), `activer_mot`. L'écran Veille les appelle (pilules de mots-clés, [../ecrans.md](../ecrans.md) § Veille), la CLI aussi : `cockpit veille mot ajoute` et `cockpit veille mot retire` ([../cli.md](../cli.md)).

## Ce qui peut casser

- Pas de session (Safari expiré sur Mac, fenêtre de connexion jamais ouverte ailleurs) : `Hote::cookies` rend `non_configure`, la plateforme est sautée avec son message, l'autre continue.
- La page change ses chemins d'API : plus rien ne passe les filtres, « aucune réponse capturée ». Regarder les requêtes de la page dans un navigateur, mettre à jour les filtres dans `ouvrir`.
- La page change la forme de ses objets : ajouter le nouveau nom de champ dans `parseurs::tiktok` ou `parseurs::instagram`, et un exemple figé dans leurs tests.
- yt-dlp ne lit plus les profils TikTok : le mettre à jour, l'erreur de chaque compte est dans le message du passage.
- Instagram bloque l'adresse : la reprise de 3 h évite d'entretenir le blocage.
- `ffmpeg`, `ffprobe` ou `yt-dlp` absent : l'analyse échoue, retentée au passage suivant ; `transcrire` absent : analyse sur les images.
- Plafond de 50 $ atteint : le relever dans `lecture::veille::PLAFOND_VEILLE_USD` (lu par la source, la CLI et l'écran) seulement sur décision de l'utilisateur.
