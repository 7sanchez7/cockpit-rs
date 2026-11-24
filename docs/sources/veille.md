# Veille

Depuis le 25 nov. 2026, la veille est le site de Viral Bank, dans l'onglet Veille ([../ecrans.md](../ecrans.md) § Veille), et son MCP pour un agent (`list_businesses`, `business_videos`, `search_library`, `get_video`, `top_hooks`, `find_more_videos`). L'ancien moteur par mots-clés (LurkAPI et TikHub, page TikTok cachée, analyses et demandes à Claude, classement) est retiré du code ; il reste dans l'historique git, avant le commit qui l'a retiré. Ses tables `veille_*` restent créées par `schema.rs` (pas de migration) mais plus rien ne les écrit ([../donnees.md](../donnees.md)).
