# Contenu (fiches vidéo)

Fichier : `crates/noyau/src/sources/contenu.rs`. Tests : dans `contenu.rs` (`mod tests`). Id : `contenu`. Cadence : 15 min. Aucune clé, aucun réseau.

Lit les fiches de vidéos du dépôt du business (`contenuDir` dans la config, [`../collecte.md`](../collecte.md) § Business) : relie chaque vidéo TikTok à sa fiche et rappelle les verdicts à écrire.

Aucun business avec `contenuDir` : `NonConfigure` « aucun dossier de fiches dans les réglages ».

## Le dossier

`contenuDir` accepte `~` en tête (`etendre`), suivi de `/` ou de `\` :

| Mac | Windows |
|---|---|
| `~/code/monapp/contenu/bank` | `~\code\monapp\contenu\bank` ou `C:\Users\<moi>\code\monapp\contenu\bank` |

Dossier absent : `NonConfigure` « dossier de fiches introuvable : <chemin> », au nom du business.

## Les fiches lues

`<famille>/<format>/*.md` et `<famille>/<format>/fait/*.md` sous `contenuDir` (`lire`), dans l'ordre des chemins. Le reste est ignoré : les fichiers à la racine (`_fiche.md`), un niveau plus haut ou plus bas, un sous-dossier autre que `fait/`.

D'une fiche on lit (`fiche`) :

- **Le frontmatter**, entre deux lignes `---` : chaque ligne `clé: valeur` (`format`, `statut`, `tiktok`…). Le commentaire de fin de ligne (`statut: publiée   # écrite → …`) est retiré, un champ vide est ignoré.
- **Le titre** : la première ligne `# ` après le frontmatter, sans la date de tête ni le tiret qui la suit, simple, demi-cadratin ou cadratin (`# 2026-09-16 - 4-astuces-bonjour` donne `4-astuces-bonjour`). Si ce titre est un slug (pas d'espace), la ligne `Titre : ` du script le remplace, sinon le slug sans ses tirets.
- **Le verdict** : une ligne qui commence par `## Verdict`.
- **La date** : le `AAAA-MM-JJ` en tête du nom de fichier, sinon la création du fichier.
- **La vidéo** : le champ `tiktok`, un lien `https://www.tiktok.com/@<compte>/video/<id>` (`video`). Sans lien, la fiche ne fait rien.

## Ce que la source fait

Pour chaque business qui a un `contenuDir`, dans un `lot` (`ecrire`) :

1. Pour chaque fiche liée à une vidéo, `Store::fiche_video` : titre (l'id à défaut), format, chemin absolu de la fiche, verdict. Ne touche pas à ce que la source TikTok écrit ([`tiktok.md`](tiktok.md)), qui reprend ce titre pour ses paliers. Format : le `format` du frontmatter, sinon le dossier.
2. Une tâche de verdict pour chaque vidéo sans `## Verdict` sortie depuis au moins 7 jours (`DELAI_VERDICT`). Sortie : `video.publiee_le` si la source TikTok l'a écrite, sinon la date de la fiche. Échéance : sortie + 7 jours (jours de Paris).

   | Champ | Valeur |
   |---|---|
   | id | `contenu:verdict:<id vidéo>` |
   | texte | `Verdict : <titre>` |
   | étiquette | l'échéance courte, `20 oct.` (`date_courte`) |
   | ton | `Alerte` passé 3 jours de retard (`RETARD_ALERTE`), sinon `Neutre` |
   | échéance | `2026-10-20` |
   | lien | `file:///…/fait/2026-10-13-ma-video.md` (`lien_fichier` ; un chemin Windows y passe avec des `/`, `file:///C:/…`) |

3. `Store::synchroniser_taches("contenu", …)` : un verdict écrit ferme la tâche au passage suivant.

La source rend le nombre de vidéos plus le nombre de tâches. Une erreur arrête le passage : `NonConfigure` au nom du business, toute autre erreur devient `Commande("<business> : <erreur>")`.

## Ce qui peut casser

- `contenuDir` absent ou dossier déplacé : source en gris.
- Une fiche pas en UTF-8 : le passage échoue, sans nommer la fiche (voir plus bas).
- Une fiche rangée plus profond, ou un `fait/` renommé : ignorée.
- Le titre de section `## Verdict` renommé : la tâche reste ouverte.
- Deux fiches avec le même lien : la dernière dans l'ordre des chemins gagne.
- Sans date dans le nom de fichier ni `publiee_le`, la date est la création du fichier, qu'un `git clone` remet à zéro.

## Différences avec la version Swift

- **Fiche illisible** : `std::fs::read_to_string` rend une erreur d'entrée-sortie sans le chemin du fichier. La version Swift nommait la fiche en échec.
- **Chemins** : `~` est suivi de `/` ou de `\`, et le lien `file://` d'un chemin Windows est écrit avec des `/`.
- Le reste suit la version Swift : mêmes fiches lues, même titre, mêmes tâches, mêmes ids.
