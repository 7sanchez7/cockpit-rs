# Notes (app Notes de macOS)

Fichier : `crates/noyau/src/sources/notes.rs`. id `notes`, cadence 1 min. Aucune clé : l'accès complet au disque pour Cockpit, le même que pour lire les cookies de Safari ([../collecte.md](../collecte.md) § Sessions). Tests dans `notes.rs` : `intertitres_et_cases`, `emojis_comptes_en_utf16`, `gzip_avec_nom_de_fichier`, `refuse_ce_qui_n_est_pas_du_gzip`.

Le plan de l'Accueil : ce qui est en cours et à faire, écrit dans une note iCloud, sur le Mac comme sur l'iPhone. Mac seulement : ailleurs, `lire` rend `non_configure`, « l'app Notes d'Apple n'existe que sur Mac ».

## La note

Une note dont la première ligne est **Cockpit** (`TITRE`, casse indifférente), dans n'importe quel dossier :

```
Cockpit                       titre, sauté
Sortir le build               avant le premier intertitre : section « Plan »
Cette semaine                 intertitre (ou titre, sous-titre) : ouvre une section
☐ 3 vidéos par jour           case à cocher : tâche, cochée ou non
☑ Relancer l'essai
- carte de leçon              puce, numéro, texte : ligne simple
```

Plusieurs notes « Cockpit » : celle modifiée en dernier. Une note dans la corbeille, marquée supprimée ou verrouillée par mot de passe est ignorée.

## D'où viennent les lignes

`~/Library/Group Containers/group.com.apple.notes/NoteStore.sqlite`, ouvert en lecture seule (`Database::ouvrir(…, true)`), par `notes::lire` :

- la note : `ZICCLOUDSYNCINGOBJECT` (`ZTITLE1`, `ZMARKEDFORDELETION`, `ZISPASSWORDPROTECTED`, `ZMODIFICATIONDATE1`, dossier `ZFOLDER` dont `ZFOLDERTYPE = 1` est la corbeille) ;
- son contenu : `ZICNOTEDATA.ZDATA`, un protobuf en gzip (`notes::note`). Document (champ 2), note (3), texte (2), morceaux de même style (5). Un morceau porte sa longueur en unités UTF-16 (1) et son style de paragraphe (2) : type (1) et, pour une case, cochée ou non (5 puis 2).

| Style de Notes | Valeur (`notes::style`) | Ligne du plan |
|---|---|---|
| titre, intertitre, sous-titre | 0, 1, 2 | nouvelle section |
| case à cocher | 103 | `coche = 1`, `fait` selon la case |
| autre (texte, puces, numéros) | | ligne simple |

Le style d'une ligne est celui de son premier caractère (`notes::lignes`). Les pièces jointes (U+FFFC), les blancs autour et les lignes vides sont retirés. Le gzip est lu par `notes::gunzip` (en-tête et champs optionnels sautés à la main, puis `flate2` pour le deflate), le protobuf par `notes::Protobuf`, sans bibliothèque protobuf.

## Ce qui est écrit

La table `plan` ([../donnees.md](../donnees.md), § `plan`), vidée et réécrite dans un seul lot à chaque passage (`Store::plan`). Le plan ne dépend d'aucun business. L'Accueil le lit par `Lectures::plan` (`lecture/accueil.rs`), dans l'ordre de la note.

## Ce qui peut casser

- **Accès complet au disque** absent ou retiré : macOS refuse l'ouverture, `non_configure`, « Notes illisible : donner l'accès complet au disque à Cockpit ».
- **Pas de note « Cockpit »** : `non_configure`, « pas de note « Cockpit » dans Notes : la créer, intertitres et cases à cocher ».
- **Apple change le format** de `ZDATA` ou les colonnes : erreur de format (« note Notes sans texte », « protobuf tronqué ») ou SQL, visible dans Santé ; le plan garde son dernier état.
- Une note modifiée sur l'iPhone arrive quand iCloud a synchronisé le Mac, puis au passage suivant (1 min).
