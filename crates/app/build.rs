fn main() {
    // le canal du navigateur caché vers l'app, seul ouvert aux pages distantes (capabilities/veille.json)
    let veille = tauri_build::InlinedPlugin::new().commands(&["message"]).default_permission(tauri_build::DefaultPermissionRule::AllowAllCommands);
    tauri_build::try_build(tauri_build::Attributes::new().plugin("veille", veille)).expect("tauri-build");
}
