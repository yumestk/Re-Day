mod store;
use chrono::{Local, SecondsFormat, Utc};
use std::sync::{Arc, Mutex};
use store::*;
use tauri::{Manager, State};

struct AppState {
    store: Arc<Mutex<Store>>,
}

async fn with_store<T: Send + 'static>(
    state: State<'_, AppState>,
    action: impl FnOnce(&mut Store) -> store::Result<T> + Send + 'static,
) -> store::Result<T> {
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // ponytail: one serialized local connection; keep SQL short, no pool for one user.
        let mut store = store.lock().map_err(|_| AppError {
            code: "STORAGE",
            message: "存储状态异常，请重启应用；本地文件已保留。".into(),
        })?;
        action(&mut store)
    })
    .await
    .map_err(|_| AppError {
        code: "INTERNAL",
        message: "操作中断，请刷新并检查结果。".into(),
    })?
}
fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}
fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

#[tauri::command]
async fn load_day(state: State<'_, AppState>, date: String) -> store::Result<DayView> {
    with_store(state, move |store| store.load_day(&date, &today())).await
}
#[tauri::command]
async fn create_quest(
    state: State<'_, AppState>,
    request: CreateRequest,
) -> store::Result<MutationResult> {
    with_store(state, move |store| store.create(request, &now())).await
}
#[tauri::command]
async fn edit_quest(
    state: State<'_, AppState>,
    request: EditRequest,
) -> store::Result<MutationResult> {
    with_store(state, move |store| store.edit(request, &today())).await
}
#[tauri::command]
async fn set_completion(
    state: State<'_, AppState>,
    request: CompletionRequest,
) -> store::Result<MutationResult> {
    with_store(state, move |store| {
        store.set_completion(request, &today(), &now())
    })
    .await
}
#[tauri::command]
async fn save_preferences(
    state: State<'_, AppState>,
    request: PreferencesRequest,
) -> store::Result<Preferences> {
    with_store(state, move |store| store.save_preferences(request)).await
}

fn main() {
    let builder = tauri::Builder::default();
    #[cfg(feature = "desktop-test")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());
    builder
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            #[cfg(not(feature = "desktop-test"))]
            let root = app.path().app_data_dir()?;
            #[cfg(feature = "desktop-test")]
            let root = std::env::var_os("LIFEISRPG_TEST_DATA_DIR")
                .map(std::path::PathBuf::from)
                .expect("desktop-test requires an isolated LIFEISRPG_TEST_DATA_DIR");
            app.manage(AppState {
                store: Arc::new(Mutex::new(Store::open(&root)?)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_day,
            create_quest,
            edit_quest,
            set_completion,
            save_preferences
        ])
        .build(tauri::generate_context!())
        .expect("LifeIsRPG could not open its local data. Existing files were preserved.")
        .run(|app, event| {
            // Route native Quit / Cmd+Q through the same pending-write and draft guard.
            if let tauri::RunEvent::ExitRequested { api, .. } = &event {
                if let Some(window) = app.get_webview_window("main") {
                    api.prevent_exit();
                    let _ = window.close();
                }
            }
            if let tauri::RunEvent::WindowEvent {
                event: tauri::WindowEvent::Destroyed,
                ..
            } = event
            {
                if app.webview_windows().is_empty() {
                    app.exit(0);
                }
            }
        });
}
