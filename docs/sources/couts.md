# Coût IA

Fichier : `crates/noyau/src/sources/firebase/cout.rs`. Taux : `metrique::EUR_PAR_USD`. Tests : `cout` dans `cout.rs`, `series_ia_a_l_heure_de_paris` et `jour_sans_voix_garde_le_cout_releve` dans `firebase/mod.rs`.

Ce n'est pas une source du registre : ce sont les prix avec lesquels la source [Firebase](firebase.md) convertit en euros ce qu'elle lit dans `aiLoad`.

| Métrique | Écrite par | Jour |
|---|---|---|
| `cout_texte` | Firebase, d'après les tokens (`euros_tokens`) | Paris |
| `cout_voix` | Firebase, d'après les caractères (`euros_voix`) | Paris |
| `cout_ia` | Firebase, `cout_texte + cout_voix` (`Store::somme`) | Paris |

## Texte

Même calcul que le dashboard admin de Roleo (`textCostEUR`), pour que les deux chiffres soient égaux.

```
cout_texte = tokens_ia × (PART_ENTREE × ENTREE_USD + (1 − PART_ENTREE) × SORTIE_USD) / 1 000 000 × EUR_PAR_USD
           = tokens_ia × (0,8 × 0,10 + 0,2 × 0,50) / 1 000 000 × 0,92
           = tokens_ia × 0,1656 € par million de tokens
```

`tokens_ia` est le compteur `tk` de Roleo : tokens d'entrée hors lecture de cache plus tokens de sortie. Zéro ou moins donne 0.

| Constante | Valeur | Ce que c'est |
|---|---|---|
| `ENTREE_USD` | 0,10 $ / M | prix d'entrée de Claude Haiku 5.5 |
| `SORTIE_USD` | 0,50 $ / M | prix de sortie de Claude Haiku 5.5 |
| `PART_ENTREE` | 0,8 | mélange observé par Roleo, environ 80 % d'entrée non cachée et 20 % de sortie |
| `metrique::EUR_PAR_USD` | 0,92 | taux fixe de Roleo (`usdToEUR`), pas le cours du jour |

## Voix

```
cout_voix = Σ caractères du moteur × voix_usd(moteur) / 1 000 000 × EUR_PAR_USD
```

Les caractères sont la carte `v` d'`aiLoad`, par moteur (`TTSProvider` de Roleo). `voix_usd` :

| Moteur | Rôle | $ par million de caractères |
|---|---|---|
| `voxtral3` | Voxtral 3 (Mistral) | 13 |
| `kokoro` | Kokoro (DeepInfra), d'après sa facture | 1,485 |
| `voxtral` | Voxtral mini | 16 |
| `gemini` | Gemini Flash TTS | 20 |

Un moteur sans prix fait lever le passage (`euros_voix`, « moteur de voix sans prix : … ») plutôt que de compter 0 : un nouveau moteur chez Roleo s'ajoute dans `voix_usd`, avec son prix. Exemple (test `cout`) : 1 million de caractères Voxtral 3 font 11,96 €.

## Ce qui n'est pas compté

- Les transcriptions (dictée) n'ont pas de compteur côté Roleo : le coût réel de la voix est un peu au-dessus de `cout_voix`.
- Les lectures du cache d'Anthropic ne sont pas dans `tk`.
- La reprise chez un autre fournisseur passe par le même compteur `tk` : ses tokens sont comptés au prix de Haiku. C'est un ordre de grandeur, pas la facture.
- L'infra Google, RevenueCat, la commission des stores.
- Les analyses de la [Veille](veille.md) : elles ont leur propre compte, en dollars, dans `veille_cout`.

## Quand le modèle ou les prix changent

1. Changer les constantes de `cout.rs` (et `metrique::EUR_PAR_USD` si Roleo change `usdToEUR`). Pour le texte, Roleo change `textCostEUR` en même temps, sinon le Cockpit et le dashboard divergent.
2. Mettre à jour les valeurs attendues du test `cout`.
3. Firebase ne réécrit qu'hier et aujourd'hui, sauf trou. Pour recalculer, supprimer les lignes `cout_texte` du business dans la base : le passage suivant relit 30 jours (`JOURS_HISTORIQUE`, `aiLoad` n'en garde pas plus) et refait `cout_voix` et `cout_ia`.
