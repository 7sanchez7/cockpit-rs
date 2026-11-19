// L'état que l'Accueil et Data partagent, comme AppState en Swift : la période (aujourd'hui à chaque lancement),
// le build choisi, le chemin ouvert dans l'arbre des installs, son découpage et la colonne qui trie la découpe.

export const etat = { periode: "jour", build: null, chemin: [], decoupe: "age", tri: null };
