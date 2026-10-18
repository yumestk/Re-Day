fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "load_day",
            "create_quest",
            "edit_quest",
            "set_completion",
            "save_preferences",
            "load_workspace",
            "save_goal",
            "save_quest",
            "set_quest_state",
            "record_execution",
            "load_quest_history",
            "schedule_quest",
            "preview_quest_delete",
            "delete_quest",
            "preview_recurrence",
            "save_recurrence",
            "load_instances",
            "reschedule_instance",
        ]),
    ))
    .expect("Tauri build configuration is invalid");
}
