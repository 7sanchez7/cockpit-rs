# La carte des docs

Chaque fichier répond à une question. Un agent lit [`../AGENTS.md`](../AGENTS.md) d'abord, puis le fichier de sa tâche.

| Fichier | Répond à |
|---|---|
| [`architecture.md`](architecture.md) | Où est quoi dans les trois crates, comment une donnée va du service à l'écran, les réglages persistants. |
| [`donnees.md`](donnees.md) | Chaque table et chaque métrique de la base : colonnes, unités, qui écrit, qui lit. |
| [`collecte.md`](collecte.md) | Le collecteur, les cadences, l'hôte, les sessions, les secrets, les notifications, le contrat d'une source, ajouter un business, la graine (`graine.json`). |
| [`sources/`](sources/README.md) | Une fiche par source du registre : requêtes exactes, ce qui est écrit, ce qui peut casser. Plus [`couts.md`](sources/couts.md), les prix de la charge IA. |
| [`ecrans.md`](ecrans.md) | Chaque écran : la question à laquelle il répond, ses blocs, les commandes et lectures qu'il fait. |
| [`design.md`](design.md) | La référence noir et blanc, les jetons de `theme.css`, la fenêtre par plateforme, la barre du haut, les composants, le mouvement. |
| [`cli.md`](cli.md) | La CLI `cockpit` : ce que l'app sait, en quelques lignes, pour un agent. |
| [`runbook.md`](runbook.md) | Prérequis, installer sur une autre machine, lancer, tester, vérifier Windows depuis le Mac, déboguer, réparer l'environnement. |

L'interface elle-même (fichiers, écrire un écran, contexte, raccourcis) est décrite dans [`../crates/app/ui/README.md`](../crates/app/ui/README.md).
