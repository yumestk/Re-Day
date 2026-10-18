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

#[tauri::command]
async fn load_workspace(state: State<'_, AppState>) -> store::Result<Workspace> {
    with_store(state, |s| s.workspace(&today())).await
}
#[tauri::command]
async fn save_goal(state: State<'_, AppState>, request: SaveGoalRequest) -> store::Result<Goal> {
    with_store(state, move |s| s.save_goal(request, &today(), &now())).await
}
#[tauri::command]
async fn save_quest(state: State<'_, AppState>, request: SaveQuestRequest) -> store::Result<Quest> {
    with_store(state, move |s| s.save_quest(request, &today(), &now())).await
}
#[tauri::command]
async fn set_quest_state(
    state: State<'_, AppState>,
    request: QuestStateRequest,
) -> store::Result<Quest> {
    with_store(state, move |s| s.quest_state(request, &today())).await
}
#[tauri::command]
async fn record_execution(
    state: State<'_, AppState>,
    request: ExecutionRequest,
) -> store::Result<bool> {
    with_store(state, move |s| {
        s.record_execution(request, &today(), &now())
    })
    .await
}
#[tauri::command]
async fn load_quest_history(
    state: State<'_, AppState>,
    quest_id: String,
) -> store::Result<QuestHistory> {
    with_store(state, move |s| s.history(&quest_id)).await
}
#[tauri::command]
async fn schedule_quest(
    state: State<'_, AppState>,
    request: ScheduleRequest,
) -> store::Result<TaskItem> {
    with_store(state, move |s| s.schedule(request, &today(), &now())).await
}
#[tauri::command]
async fn preview_quest_delete(
    state: State<'_, AppState>,
    dataset_id: String,
    quest_id: String,
) -> store::Result<DeletePreview> {
    with_store(state, move |s| s.preview_delete(&dataset_id, &quest_id)).await
}
#[tauri::command]
async fn delete_quest(state: State<'_, AppState>, request: DeleteRequest) -> store::Result<bool> {
    with_store(state, move |s| s.delete_quest(request)).await
}

#[tauri::command]
async fn preview_recurrence(
    state: State<'_, AppState>,
    request: RuleRequest,
) -> store::Result<RulePreview> {
    with_store(state, move |s| s.preview_rule(request, &today())).await
}
#[tauri::command]
async fn save_recurrence(
    state: State<'_, AppState>,
    request: CommitRuleRequest,
) -> store::Result<bool> {
    with_store(state, move |s| s.save_rule(request, &today())).await
}
#[tauri::command]
async fn load_instances(
    state: State<'_, AppState>,
    dataset_id: String,
    quest_id: String,
    date: String,
) -> store::Result<Vec<TaskItem>> {
    with_store(state, move |s| s.instances(&dataset_id, &quest_id, &date)).await
}
#[tauri::command]
async fn reschedule_instance(
    state: State<'_, AppState>,
    request: RescheduleRequest,
) -> store::Result<MutationResult> {
    with_store(state, move |s| {
        s.reschedule_instance(request, &today(), &now())
    })
    .await
}

#[tauri::command]
async fn load_calendar(state: State<'_, AppState>, month: String) -> store::Result<MonthView> {
    with_store(state, move |s| s.load_calendar(&month, &today())).await
}
#[tauri::command]
async fn load_date(state: State<'_, AppState>, date: String) -> store::Result<DateView> {
    with_store(state, move |s| s.load_date(&date, &today())).await
}
#[tauri::command]
async fn load_review_prompt(state: State<'_, AppState>) -> store::Result<Option<ReviewState>> {
    with_store(state, |s| s.review_prompt(&today())).await
}
#[tauri::command]
async fn save_review(state: State<'_, AppState>, request: ReviewRequest) -> store::Result<bool> {
    with_store(state, move |s| s.save_review(request, &today(), &now())).await
}
#[tauri::command]
async fn skip_plan(state: State<'_, AppState>, request: SkipRequest) -> store::Result<bool> {
    with_store(state, move |s| s.skip_plan(request)).await
}
#[tauri::command]
async fn preview_ordinary_reschedule(
    state: State<'_, AppState>,
    request: RescheduleRequest,
) -> store::Result<ReschedulePreview> {
    with_store(state, move |s| {
        s.preview_ordinary_reschedule(request, &today())
    })
    .await
}
#[tauri::command]
async fn reschedule_ordinary(
    state: State<'_, AppState>,
    request: OrdinaryReschedule,
) -> store::Result<MutationResult> {
    with_store(state, move |s| {
        s.reschedule_ordinary(request, &today(), &now())
    })
    .await
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
            save_preferences,
            load_workspace,
            save_goal,
            save_quest,
            set_quest_state,
            record_execution,
            load_quest_history,
            schedule_quest,
            preview_quest_delete,
            delete_quest,
            preview_recurrence,
            save_recurrence,
            load_instances,
            reschedule_instance,
            load_calendar,
            load_date,
            load_review_prompt,
            save_review,
            skip_plan,
            preview_ordinary_reschedule,
            reschedule_ordinary
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
