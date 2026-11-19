# Données

Une base SQLite locale, `cockpit.sqlite`, dans le dossier des données ([`collecte.md`](collecte.md) § Où sont les fichiers) :

| | Mac | Windows |
|---|---|---|
| base | `~/Library/Application Support/Cockpit/cockpit.sqlite` | `%APPDATA%\Cockpit\cockpit.sqlite` |

Les sources l'écrivent, les écrans la lisent, rien d'autre. Le schéma est dans `crates/noyau/src/schema.rs`, identique à celui de la version Swift : sur Mac, les deux apps ouvrent la même base. Les écritures sont dans `crates/noyau/src/store.rs` (`Store`) et `crates/noyau/src/store_veille.rs`, les modèles dans `crates/noyau/src/modeles.rs`, les noms des métriques dans `crates/noyau/src/metrique.rs`.

Pas de migrations : une table s'ajoute dans `schema::SQL` avec `create table if not exists`, une colonne dans `schema::COLONNES` (table, colonne, définition), ajoutée au lancement si `colonne_existe` dit qu'elle manque. Une colonne nouvelle a une valeur par défaut. Effacer la base est sans risque : la collecte suivante la remplit, seul l'historique accumulé (MRR jour par jour, relevés de vidéos, rangs) est perdu.

Au lancement, `Store::ouvrir` crée le dossier, joue le schéma, ajoute les colonnes, applique la graine ([`collecte.md`](collecte.md) § Graine), puis efface les lignes de `collecte` de plus de 30 jours.

## Conventions

| Quoi | Format |
|---|---|
| `jour` | `yyyy-MM-dd`, fuseau Europe/Paris (`jour::cle`) |
| `heure` | 0 à 23, heure de Paris (`jour::heure`) ; `-1` = la journée entière |
| `at`, `maj`, `debut`, `fin`, `cree_le`, `fait_le` | ISO 8601 UTC sans fractions, `2026-11-13T14:05:00Z` (`jour::iso`) ; une `Date` passée en paramètre est toujours écrite ainsi |
| booléens | entier 0 ou 1 |
| `business` | l'`id` d'une ligne de `business`, `monapp` |
| argent | `$` pour RevenueCat (sa devise), `€` pour les coûts ; jamais converti en douce, une conversion passe par `metrique::en_euros` (`EUR_PAR_USD` = 0,92, taux fixe) et l'écran le dit |

## Tables

### `business`

Un business suivi. Créé par la graine ou dans Réglages, modifiable dans Réglages. Modèle : `Business` (`crates/noyau/src/business.rs`).

| Colonne | Contenu |
|---|---|
| `id` | clé courte, `monapp` |
| `nom` | `Mon App` |
| `lettre` | l'initiale de l'avatar quand le business n'a pas d'icône d'app, `M` ; par défaut l'initiale du nom |
| `teinte` | clé de couleur de l'avatar, `menthe` par défaut |
| `ordre` | rang dans le menu des business ; `businesses()` trie par `ordre`, puis par nom |
| `config` | JSON de `Config` : champs dans [`collecte.md`](collecte.md) § Business |

`Store::enregistrer` crée ou remplace `nom`, `lettre`, `teinte` et `config`, sans toucher à `ordre`. `supprimer_business` efface la ligne.

### `serie`

Toutes les séries chiffrées, une ligne par métrique, jour et heure. Une source réécrit la ligne du jour à chaque collecte (`Store::serie`), donc la valeur d'aujourd'hui est toujours « jusqu'à maintenant ».

| Colonne | Contenu |
|---|---|
| `business`, `metrique`, `jour`, `heure` | clé primaire |
| `valeur` | le chiffre |
| `maj` | dernier relevé |

`Store::somme(business, metrique, parts, jour)` écrit la journée d'une métrique comme la somme des journées de `parts`, et rien si aucune n'est relevée. `historique_incomplet(business, metrique, jours, aujourdhui)` dit s'il manque hier ou le plus vieux des `jours` jours : la source relit alors tout son historique (RevenueCat).

Le catalogue des métriques est fermé : une métrique nouvelle s'ajoute ici et dans `metrique.rs`, et le code n'écrit jamais le nom en dur (`metrique::ACTIFS`).

**Par store.** Les métriques de `Boutique::SEPAREES` (celles de PostHog, de RevenueCat et `telechargements`) s'écrivent trois fois : le total sous le nom nu (`actifs`), puis une ligne par store suffixée (`actifs.app_store`, `actifs.play_store`, `Boutique::metrique`). Côté PostHog, le store se lit dans `$os` : iOS et iPadOS pour l'App Store, Android pour le Play Store ; le total des actifs, essais et ronds garde tous les OS, celui des installs vaut iOS + iPadOS + Android. Côté RevenueCat, les graphiques sont demandés avec `segment=store`, sauf `customers_new` (`segment=first_platform`) ; l'overview (MRR, abonnés, essais en cours, revenu des 28 j) ne se découpe pas, chaque store en prend la part qu'il a dans les graphiques du jour. `telechargements` vient des stores eux-mêmes : chaque store écrit sa ligne suffixée, puis le total en est la somme (`Boutique::telechargements`). Les autres métriques n'ont que le total.

| Métrique | Constante | Heure | Unité | Source | Définition |
|---|---|---|---|---|---|
| `actifs` | `ACTIFS` | -1 | personnes | PostHog | personnes distinctes avec un événement de l'app ce jour-là, hors bruit (`posthog::BRUIT` : `$identify`, `$set`, `$feature_flag_called`, `$opt_in`, `$screen_view_exit`) |
| `actifs_j7` | `ACTIFS_J7` | -1 | personnes | PostHog | la même mesure, le même jour de la semaine précédente, jusqu'à la même heure et minute |
| `actifs_h` | `ACTIFS_HEURE` | 0-23 | personnes | PostHog | personnes distinctes dans l'heure |
| `installs` | `INSTALLS` | -1 | installs | PostHog | premier événement `installation` d'une personne (`onboarding_started` par défaut), iOS, iPadOS ou Android |
| `installs_j7` | `INSTALLS_J7` | -1 | installs | PostHog | idem, J-7 jusqu'à la même heure |
| `essais` | `ESSAIS` | -1 | essais | PostHog | événement `essai` (`trial_started` par défaut) |
| `essais_j7` | `ESSAIS_J7` | -1 | essais | PostHog | idem, J-7 jusqu'à la même heure |
| `ronds` | `RONDS` | -1 | ronds | PostHog | événement `usage` (`path_round_done` par défaut) |
| `ronds_j7` | `RONDS_J7` | -1 | ronds | PostHog | idem, J-7 jusqu'à la même heure |
| `achats` | `ACHATS` | -1 | achats | RevenueCat | premières transactions payantes du jour (nouvel abonnement payé, fin d'essai payée) |
| `mrr_brut` | `MRR_BRUT` | -1 | $ | RevenueCat | MRR de l'overview RevenueCat, dernier relevé du jour |
| `mrr_net` | `MRR_NET` | -1 | $ | RevenueCat | `mrr_brut × PART_NETTE` (0,72 : TVA puis commission du store) |
| `abonnes` | `ABONNES` | -1 | abonnés | RevenueCat | abonnements payants actifs |
| `essais_actifs` | `ESSAIS_ACTIFS` | -1 | essais | RevenueCat | essais en cours |
| `revenu_28j` | `REVENU_28J` | -1 | $ | RevenueCat | revenu encaissé des 28 derniers jours |
| `revenu` | `REVENU` | -1 | $ | RevenueCat | revenu brut encaissé du jour UTC, aujourd'hui compris et encore partiel |
| `nouveaux_clients` | `NOUVEAUX_CLIENTS` | -1 | clients | RevenueCat | clients vus pour la première fois par RevenueCat ce jour-là, aujourd'hui compris ; par store selon `first_platform`, les clients de plateforme inconnue ne comptent qu'au total |
| `telechargements` | `TELECHARGEMENTS` | -1 | téléchargements | App Store Connect, Play Console | premiers téléchargements comptés par le store, que l'app ait été ouverte ou non : `telechargements.app_store` (rapport de ventes Apple), `telechargements.play_store` (rapport groupé Play) ; le total, nu, est la somme des deux ; un à trois jours de retard |
| `vues_tiktok` | `VUES_TIKTOK` | -1 | vues | TikTok | vues gagnées dans la journée, toutes vidéos des comptes du business |
| `vues_tiktok_j7` | `VUES_TIKTOK_J7` | -1 | vues | TikTok | idem, J-7 jusqu'à la même heure, quand les relevés existent |

Les métriques de Firebase et de Classements ont leur constante dans `metrique.rs` ; leurs sources sont décrites dans [`sources/firebase.md`](sources/firebase.md) et [`sources/classements-itunes.md`](sources/classements-itunes.md).

| Métrique | Constante | Source (Swift) |
|---|---|---|
| `cout_ia`, `cout_texte`, `cout_voix`, `tokens_ia`, `echecs_ia` | `COUT_IA`, `COUT_TEXTE`, `COUT_VOIX`, `TOKENS_IA`, `ECHECS_IA` | Firebase |
| `req_ia.<fournisseur>`, `q429_ia.<fournisseur>`, `err_ia.<fournisseur>`, `ms_ia.<fournisseur>` | `requetes_ia`, `q429_ia`, `erreurs_ia`, `latence_ia` | Firebase, par jour (-1) et par heure |
| `messages_ouverts`, `retours_nouveaux` | `MESSAGES_OUVERTS`, `RETOURS_NOUVEAUX` | Firebase |
| `note`, `nb_notes` | `NOTE`, `NB_NOTES` | Classements |

### `fil`

Le fil « En direct » : ce qui vient de se passer. Une source ajoute une ligne par `Store::fil`, qui ignore un `id` déjà présent et rend `true` pour une ligne nouvelle ; l'`id` est stable et commence toujours par l'id de la source (`<source>:<id chez la source>`), donc une ligne déjà vue n'est jamais réinsérée et ne renotifie pas. Le collecteur notifie les lignes nouvelles d'une source par ce préfixe (`prendre_nouveaux`). Modèle : `Evenement`.

| Colonne | Contenu |
|---|---|
| `id` | clé stable, `posthog:essai:<uuid>` |
| `business`, `at` | qui, quand (l'heure de l'événement, pas celle de la collecte) |
| `type` | `TypeFil::brut` : `ESSAI`, `ACHAT`, `AVIS`, `MSG`, `RETOUR`, `VIDEO`, `RANG`, `IA`, `VERSION`, `VEILLE` |
| `texte` | une ligne lisible, `Essai annuel, Belgique` |
| `lien` | URL à ouvrir au clic, facultative |

### `compte` et `compte_releve`

Les comptes vidéo suivis, toutes plateformes, et leurs compteurs dans le temps. Ajoutés par la graine, dans l'écran Vidéos ou par un agent ([`runbook.md`](runbook.md)). Modèles : `Compte`, `ReleveCompte`.

| `compte` | Contenu |
|---|---|
| `id` | `<plateforme>:<handle>`, `tiktok:monapp` (`Compte::id`) |
| `business` | `monapp` |
| `plateforme` | `Plateforme::brut` : `tiktok`, `instagram`, `youtube`, `facebook` |
| `handle` | le nom du compte, minuscules, `[a-z0-9._-]`, 64 caractères au plus, sans `@` (`Compte::handle` normalise une saisie `@x` ou un lien de profil) ; pour Facebook, le nom ou l'id de la Page |
| `actif` | 0 = en pause : la source ne le relève plus, ses vidéos restent affichées (`activer_compte`) |
| `ajoute_le` | ISO UTC |

`Store::ajouter` crée le compte ou le remet actif. `retirer_compte` supprime sa ligne seulement : ses relevés et ses vidéos restent en base.

| `compte_releve` | Contenu |
|---|---|
| `compte`, `at` | clé primaire, un relevé par collecte (`releve_compte`) |
| `abonnes`, `videos`, `likes` | compteurs du profil, nuls si la plateforme ne les donne pas |

La pastille « +N en 7 j » d'une carte de compte est `abonnes(dernier relevé) − abonnes(dernier relevé d'il y a 7 jours ou plus)`.

### `video`, `video_releve` et `video_analyse`

Une ligne par vidéo publiée, et ses relevés de stats dans le temps. Deux sources remplissent la même ligne sans s'écraser : la source de la plateforme par `Store::video` (compte, légende, date, durée, lien ; une valeur nulle ne remplace pas une valeur connue), Contenu par `Store::fiche_video` (titre, format, fiche, verdict). Modèles : `Video`, `FicheVideo`, `Releve`, `Analyse`.

| `video` | Contenu |
|---|---|
| `id` | brut pour TikTok (les fiches s'en servent), `<plateforme>:<id>` sinon (`instagram:<code>`, `youtube:<id>`, `facebook:<id>`) : `Plateforme::id_video` |
| `plateforme` | `tiktok` (défaut des lignes d'avant la colonne), `instagram`, `youtube`, `facebook` |
| `business`, `compte` | `monapp`, le handle |
| `legende` | légende ou titre chez la plateforme |
| `titre` | titre de la fiche du dossier `bank`, écrit par Contenu ; l'écran affiche `coalesce(titre, legende)` |
| `format` | dossier du format dans `bank`, sinon nul |
| `fiche` | chemin de la fiche, sinon nul |
| `publiee_le` | ISO UTC |
| `duree_s`, `url` | durée, lien public (`Plateforme::lien`) |
| `verdict` | 1 si la fiche a déjà une section `## Verdict` |

| `video_releve` | Contenu |
|---|---|
| `video`, `at` | clé primaire, un relevé par collecte (`Store::releve`) |
| `vues`, `likes`, `commentaires`, `partages`, `enregistrements` | compteurs cumulés |
| `watch_moyen_s`, `completion` | durée moyenne regardée, taux de complétion (0 à 1), nuls si la source ne les a pas |
| `abonnes` | abonnés gagnés par la vidéo, nul si inconnu |

| `video_analyse` | Contenu |
|---|---|
| `video` | clé primaire |
| `at` | dernier passage qui l'a écrite ; remplacée à chaque passage (`Store::analyse`) |
| `donnees` | JSON d'`Analyse`, mêmes clés que la version Swift : `watchTotalS` (temps total regardé, s), `spectateurs`, `nouveaux` et `abonnesPart` (parts de 0 à 1), `retention` (part encore là, un point par pas de la courbe), `sources`, `recherches`, `ages`, `pays`, `jalons` (listes de `{nom, part}`), `vuesParJour` (7 premiers jours) |

Seules les vidéos des comptes TikTok de la session connectée et des Pages Facebook en ont une. Une analyse vide (`Analyse::est_vide`) n'est pas écrite.

La colonne « 24 h » de l'écran Vidéos est `vues(dernier relevé) − vues(relevé le plus proche d'il y a 24 h)`.

### `app` et `rang`

Toutes les apps dont on suit le classement : les nôtres (`business` non nul) et celles des autres. La source Classements les écrit, App Store Connect aussi pour l'app d'un business. Modèles : `Appli`, `Liste`.

| `app` | Contenu |
|---|---|
| `id` | id App Store |
| `nom`, `editeur`, `icone` | |
| `genre` | catégorie principale, sert à choisir les classements à lire |
| `note`, `nb_notes`, `version` | dans le premier pays suivi |
| `business` | nul pour une app d'un autre ; `Store::app` ne remplace pas un business connu par nul |
| `suivie` | 0 pour la retirer du panneau sans perdre son historique (`suivre`) ; `Store::app` n'y touche pas |

| `rang` | Contenu |
|---|---|
| `app`, `pays`, `genre`, `liste`, `at` | clé primaire ; `pays` en code ISO (`fr`), `genre` (`6017` Éducation, `36` toutes catégories), `liste` (`Liste::brut` : `gratuit`, `payant`, `recettes`) |
| `position` | 1 à 200 |

Chaque passage de Classements a un seul `at` pour toutes ses lignes ; l'index `rang_at` sert à retrouver le dernier passage et ses positions. Une app absente d'une liste au dernier passage réussi n'a pas de ligne à cet `at` : elle est « hors top 200 ».

### `avis`

Avis App Store de nos apps, lus par App Store Connect (`Store::avis`, modèle `Avis`).

| Colonne | Contenu |
|---|---|
| `id` | id App Store Connect de l'avis |
| `business`, `pays` | pays en code ISO à 3 lettres comme ASC (`FRA`) |
| `note`, `titre`, `texte`, `auteur`, `at` | l'avis |
| `repondu` | 1 si une réponse développeur existe |

### `a_traiter`

L'écran SAV et le compteur ambre de la barre du haut. Modèle : `Tache`.

| Colonne | Contenu |
|---|---|
| `id` | `manuel:<UUID en majuscules>` pour une tâche à la main (`ajouter_tache`) ; `<source>:<clé>` pour une tâche posée par une source (`asc:avis:<id>`, `contenu:verdict:<id vidéo>`, et pour Firebase `firebase:msg:<uid>:<dernier message>` et `firebase:retour:<id>`, `ids_sav`) |
| `business`, `texte` | à quoi ça sert, en une ligne |
| `etiquette`, `ton` | la pastille (`retard`, `FR`, `17 nov.`) et sa couleur : `Ton::brut`, `alerte` (ambre) ou `neutre` |
| `echeance` | jour `yyyy-MM-dd`, facultatif ; dépassé, le ton passe à `alerte` |
| `source` | `manuel`, `asc`, `firebase`, `contenu` |
| `cree_le`, `fait_le` | `fait_le` nul tant que c'est à faire |
| `lien` | où agir : l'avis dans App Store Connect, la fiche |

Une source synchronise ses tâches avec `Store::synchroniser_taches(source, business, taches)` : les nouvelles entrent, celles qui ont disparu de la source (avis répondu, verdict écrit) passent à faites. `cocher` marque une tâche faite ou la rouvre ; si la source renvoie encore une tâche cochée, elle reste faite. `supprimer_tache` n'efface qu'une tâche manuelle.

### `sav_fil`, `sav_message`, `sav_retour` et `sav_compte`

L'écran SAV, recopié de Firestore par la source Firebase. Le `Store` les écrit : `synchroniser_sav` pour un passage, `repondre_sav`, `fermer_fil_sav` et `traiter_retour_sav` pour les actions de l'écran, `fils_lus` et `comptes_sav` pour savoir quoi relire. Modèles : `FilSAV`, `MessageSAV`, `RetourSAV`, `ReleveSAV`.

| `sav_fil` | Contenu |
|---|---|
| `business`, `uid` | clé primaire, un fil par compte |
| `nom` | le nom affiché du compte, nul s'il n'en a pas |
| `dernier_at`, `dernier_equipe`, `apercu` | le dernier message : ISO, 1 s'il vient de l'équipe, son début |
| `ouvert` | 1 tant que l'équipe n'a ni répondu ni fermé le fil ; un fil ouvert absent du relevé suivant passe à 0 |
| `non_vu` | 1 si la dernière réponse de l'équipe n'a pas encore été ouverte dans l'app |

L'état affiché (`FilSAV::etat`, `EtatFil`) : ouvert, à répondre ; sinon `non_vu`, pas encore lu ; sinon dernier message de l'équipe, lu ; sinon traité (fermé sans réponse). Répondre ou fermer un fil passe ses tâches `firebase:msg:<uid>:` à faites.

| `sav_message` | Contenu |
|---|---|
| `business`, `uid`, `id` | clé primaire |
| `equipe`, `texte`, `at` | 1 si l'équipe l'a écrit, le texte, ISO |

Un fil relu remplace tous ses messages.

| `sav_retour` | Contenu |
|---|---|
| `business`, `id` | clé primaire |
| `uid` | l'auteur, nul pour un retour anonyme |
| `source`, `categorie`, `etoiles` | d'où vient le retour, sa catégorie, la note sur 5 s'il en porte une |
| `message`, `citation`, `conversation` | le texte, la bulle signalée, l'id de la conversation |
| `traite`, `traite_le` | 1 une fois traité, et quand ; un retour à traiter absent du relevé suivant passe à 1 |
| `plateforme`, `build` | `ios` ou `android`, le numéro de build de l'app |
| `at` | ISO |

| `sav_compte` | Contenu |
|---|---|
| `business`, `uid`, `email` | l'email du compte, vide pour un invité ou un compte supprimé. Un uid avec un email n'est plus relu ; un vide l'est à chaque passage |

### `install`, `paiement` et `porte`

L'arbre des installs de l'écran Data. PostHog réécrit `install` et `porte` au plus une fois par heure sur les `JOURS_INSTALLS` (97) derniers jours ; RevenueCat remplit `paiement` pour les installs qui ont converti. Modèles : `Install`, `EtatPaiement`, `Porte`. Les anciennes tables `cohorte`, `entonnoir` et `decoupe` sont effacées au lancement.

| `install` | Contenu |
|---|---|
| `business`, `jour` | jour d'install à Paris (premier événement `installation`) |
| `loin` | rang de l'écran du tunnel le plus loin atteint, 0 à 17 (`posthog::ECRANS`, le paywall compris) |
| `paywall`, `essai`, `direct` | 0 ou 1 : a vu le paywall, a pris l'essai, a acheté sans essai |
| `rond` | 0 ou 1 : a fait l'événement `usage` au moins une fois |
| `j1`, `j7`, `j14`, `j30` | 0 ou 1 : a fait l'événement `usage` entre N et N+1 jours après l'install ; ne compte qu'une fois le jour d'install complet |
| `client` | l'id RevenueCat (`distinct_id` de l'essai ou de l'achat) s'il a converti, nul sinon |
| `valeurs` | objet JSON, une liste de valeurs par `Dimension` répondue : `{"age":["18_24"],"interet":["music","series"],"pays":["FR"],"version":["2.0.3"],"build":["49"],"zone":["europe"]}`. Une dimension absente = pas répondu. `zone` est déduite du pays au décodage (`Dimension::zone`) |

`Store::installs(business, debut, lignes)` efface les lignes depuis `debut`, puis écrit les nouvelles, dans une transaction.

| `paiement` | Contenu |
|---|---|
| `business`, `client` | un converti de `install` |
| `etat` | `EtatPaiement::brut` : `paye` (un abonnement de production a rapporté), `essai` (l'un court encore), `non`, `inconnu` (pas d'abonnement de production, ou 404) |
| `lu` | ISO, dernière lecture |

`clients_a_verifier(business, depuis, relu, limite)` rend les convertis à demander à RevenueCat, les plus récents d'abord : jamais lus, ou `essai` et `non` lus avant `relu` et installés depuis `depuis`. RevenueCat relit ainsi toutes les 6 h les installs des 40 derniers jours.

| `porte` | Contenu |
|---|---|
| `business`, `jour`, `porte` | jour à Paris et porte du paywall (`properties.source` de l'événement `paywall`, vide pour les anciennes versions) |
| `vues`, `essais`, `directs` | paywalls vus, essais gratuits, achats sans essai |

### `veille_mot`, `veille_compte`, `veille_video`, `veille_releve`, `veille_analyse`, `veille_cout`, `veille_demande` et `veille_retenue`

La veille par mots-clés, à part des tables `compte` et `video` : les comptes trouvés ne sont pas suivis, leurs vidéos ne sont pas les nôtres. Écrite par la source Veille ([`sources/veille.md`](sources/veille.md)), par `crates/noyau/src/store_veille.rs`, les modèles dans `crates/noyau/src/modeles_veille.rs`.

| `veille_mot` | Contenu |
|---|---|
| `business`, `mot` | clé primaire (`ajouter_mot`, `retirer_mot`, `activer_mot`) |
| `vues_min` | seuil de vues pour garder une vidéo, 20 000 par défaut |
| `actif` | 0 = en pause |
| `ajoute`, `cherche` | ISO UTC ; `cherche` = dernier passage qui l'a cherché (`Store::cherche`) |

Retirer un mot supprime sa ligne seulement : ses vidéos restent.

| `veille_compte` | Contenu |
|---|---|
| `id` | `<plateforme>:<handle>` |
| `plateforme`, `handle`, `abonnes` | |
| `mediane`, `nb_base`, `base_at` | médiane des vues, nombre de vidéos lues, date (`Store::base`) ; `comptes_a_baser` rend ceux à relire |
| `decouvert` | premier passage qui l'a vu |

| `veille_video` | Contenu |
|---|---|
| `id` | comme `video.id` : brut pour TikTok, `instagram:<code>` |
| `business`, `plateforme`, `compte`, `url` | |
| `legende`, `publiee`, `duree_s` | |
| `mot` | le premier mot qui l'a trouvée |
| `media` | URL du fichier vidéo quand la page l'a donnée, expire en quelques heures |
| `trouvee` | premier passage qui l'a vue (`trouvaille`) |
| `app`, `langue`, `genre` | le classement de Claude ([`sources/veille.md`](sources/veille.md) § Le classement) : l'app promue si elle est clairement nommée (vide sinon), le code de langue (`LANGUES` ou `inconnue`), l'un des `GENRES` |
| `classee` | ISO UTC du classement (`classer_video`) ; nulle = à classer, une nouvelle analyse la remet à nulle |

| `veille_releve` | Contenu |
|---|---|
| `video`, `at` | clé primaire, un relevé par passage qui la voit (`relever`) |
| `vues`, `likes`, `commentaires`, `partages`, `enregistrements` | compteurs cumulés, nuls si la page ne les donne pas |

| `veille_analyse` | Contenu |
|---|---|
| `video` | clé primaire |
| `at`, `modele` | date et modèle Claude |
| `donnees` | JSON d'`AnalyseVeille` (`analyse_veille`) |

| `veille_cout` | Contenu |
|---|---|
| `video`, `at`, `modele` | un appel Claude, réussi ou non (`cout_veille`) ; `video` vaut `demande:<id>` pour les appels d'une demande à Claude (mots-clés de Sonnet, tours de Haiku), `classement` pour ceux du classement |
| `entree`, `sortie` | tokens facturés ; `entree` compte tout le prompt, cache écrit et lu compris |
| `usd` | coût de l'appel, au prix de chaque sorte de token (`Usage::usd`) ; `depense_veille_usd` en fait la somme |

Une demande à Claude depuis l'écran Veille ([`sources/veille.md`](sources/veille.md) § La demande). Ces deux tables n'existent que dans la version Rust : l'app Swift les ignore.

| `veille_demande` | Contenu |
|---|---|
| `id` | UUID en majuscules |
| `business`, `prompt` | le business choisi et la consigne, 2 000 caractères au plus |
| `debut`, `fin` | ISO UTC ; `fin` nulle tant qu'elle tourne |
| `etat` | `en_cours`, `finie` ou `echouee` ; créer une demande passe celle restée `en_cours` en `echouee`, erreur `DemandeVeille::INTERROMPUE` (`nouvelle_demande`) |
| `journal` | tableau JSON de lignes, une par étape (`noter_demande`) |
| `synthese`, `erreur` | la synthèse de Haiku, ou ce qui l'a arrêtée (`finir_demande`) |
| `fil` | l'id de la première demande de sa conversation ; nul pour les demandes d'avant les conversations, lues comme `coalesce(fil, id)` |

| `veille_retenue` | Contenu |
|---|---|
| `demande`, `video` | clé primaire ; `video` est un `veille_video.id` (`retenir_video`) |
| `raison` | pourquoi Claude l'a gardée ; la garder de nouveau la remplace |
| `ordre` | rang de la première fois où elle est gardée, à partir de 0 |

### `collecte`

Le journal du collecteur, une ligne par passage d'une source (`Store::journal`, modèle `Passage`). La bande des sources et l'écran Santé lisent la dernière ligne de chaque source (`dernier_passage`) ; `dernier_succes` rend la `fin` du dernier passage `ok`. Les lignes de plus de 30 jours sont effacées au lancement.

| Colonne | Contenu |
|---|---|
| `source` | `posthog`, un passage couvre tous les business |
| `debut`, `fin` | ISO UTC |
| `etat` | `EtatPassage::brut` : `ok`, `erreur`, `non_configure` (clé absente, business sans config : gris, pas une panne) |
| `erreur` | message lisible, jamais une clé ni une URL avec jeton |
| `lignes` | lignes écrites |

### `plan`

Le plan de l'Accueil, lu par la source Notes ([`sources/notes.md`](sources/notes.md)). `Store::plan` vide la table et la réécrit ; aucun business. Modèle : `LignePlan`.

| Colonne | Contenu |
|---|---|
| `ordre` | rang de la ligne, à partir de 0 |
| `section` | dernier titre au-dessus |
| `texte` | la ligne |
| `coche` | `1` pour une case à cocher |
| `fait` | `1` pour une case cochée |

### `reglage`

Clé, valeur texte (`Store::reglage`, `regler`, qui efface la clé pour `None`) :

| Clé | Valeur |
|---|---|
| `ecran`, `business`, `boutique`, `accueil.reseau`, `accueil.plan` | l'état de l'interface rendu au démarrage (`commandes::REGLAGES`) : dernier écran, filtre de business, store affiché (`app_store`, `play_store`, absent = total), panneaux repliés (`0`) |
| `notifs.<type>` | `0` coupe les notifications d'un type du fil (`essai`, `achat`… ; `TypeFil::reglage_notif`), absent = activé |
| `collecte.pause` | `1` met le collecteur en pause (`REGLAGE_PAUSE`) |
| `veille.videos` | vidéos gardées par mot-clé et par plateforme à chaque passage de la Veille, de 10 à 100 (`REGLAGE_VIDEOS`), absent = 40 ; la version Swift l'ignore |
| `posthog.horaire.<business>` | instant ISO du dernier calcul de `install` et `porte`, refaits au plus une fois par heure |
| `graine` | date ISO de modification de la graine déjà appliquée |
