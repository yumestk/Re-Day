fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "load_day",
            "create_quest",
            "edit_quest",
            "set_completion",
            "save_preferences",
        ]),
    ))
    .expect("Tauri build configuration is invalid");
}
