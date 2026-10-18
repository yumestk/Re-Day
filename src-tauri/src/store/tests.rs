use super::*;

const DATE: &str = "2026-10-18";
const NOW: &str = "2026-10-18T06:00:00.000Z";
fn memory() -> Store {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!("../schema.sql")).unwrap();
    Store {
        dataset_id: Uuid::new_v4().to_string(),
        conn,
    }
}
fn request(store: &Store) -> CreateRequest {
    CreateRequest {
        dataset_id: store.dataset_id.clone(),
        quest_id: Uuid::new_v4().to_string(),
        plan_id: Uuid::new_v4().to_string(),
        draft: TaskDraft {
            goal_ids: vec![],
            parent_id: None,
            title: "完成技术文档".into(),
            description: "写清楚事务边界".into(),
            date: DATE.into(),
            slot: "afternoon".into(),
            start_time: Some("14:00".into()),
            end_time: None,
        },
    }
}
fn completion(store: &Store, item: &TaskItem, desired: bool) -> CompletionRequest {
    CompletionRequest {
        dataset_id: store.dataset_id.clone(),
        plan_id: item.plan_id.clone(),
        record_id: Uuid::new_v4().to_string(),
        desired_completed: desired,
        completion_id: item.completion_id.clone(),
        expected_revision: item.revision,
        expected_quest_revision: item.quest_revision,
    }
}
fn count(store: &Store, table: &str) -> i64 {
    store
        .conn
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn create_is_atomic_and_replay_does_not_duplicate() {
    let mut store = memory();
    let request = request(&store);
    let first = store.create(request.clone(), NOW).unwrap();
    assert!(first.changed);
    assert!(!store.create(request.clone(), NOW).unwrap().changed);
    assert_eq!(count(&store, "quests"), 1);
    assert_eq!(count(&store, "plans"), 1);
    let mut conflicting = request;
    conflicting.draft.title = "另一件事".into();
    assert_eq!(
        store.create(conflicting, NOW).err().unwrap().code,
        "CONFLICT"
    );

    store.conn.execute_batch("CREATE TRIGGER fail_plan BEFORE INSERT ON plans BEGIN SELECT RAISE(ABORT, 'injected failure'); END;").unwrap();
    let new_request = request_for(&store);
    assert!(store.create(new_request, NOW).is_err());
    assert_eq!(count(&store, "quests"), 1);
}
fn request_for(store: &Store) -> CreateRequest {
    request(store)
}

#[test]
fn completion_undo_replay_and_redo_have_one_effective_result() {
    let mut store = memory();
    let create = request(&store);
    let original = store.create(create, NOW).unwrap().item;
    let done_request = completion(&store, &original, true);
    let done = store
        .set_completion(done_request.clone(), DATE, NOW)
        .unwrap()
        .item;
    assert!(done.completed);
    assert!(
        !store
            .set_completion(done_request.clone(), DATE, NOW)
            .unwrap()
            .changed
    );
    store.conn.execute("INSERT INTO execution_records(id,quest_id,plan_id,kind,occurred_on,recorded_at,content_snapshot)
        VALUES (?1,?2,?3,'progress',?4,?5,?6)", params![Uuid::new_v4().to_string(), original.quest_id, original.plan_id, DATE, NOW,
            r#"{"title":"完成技术文档","description":"","goals":[]}"#]).unwrap();
    let undo_request = completion(&store, &done, false);
    let open = store
        .set_completion(undo_request.clone(), DATE, NOW)
        .unwrap()
        .item;
    assert!(!open.completed);
    assert!(
        !store
            .set_completion(undo_request, DATE, NOW)
            .unwrap()
            .changed
    );
    let replay = store.set_completion(done_request, DATE, NOW).unwrap();
    assert!(!replay.changed && !replay.item.completed);
    assert!(
        store
            .set_completion(completion(&store, &open, true), DATE, NOW)
            .unwrap()
            .item
            .completed
    );
    let active: i64 = store
        .conn
        .query_row(
            "SELECT count(*) FROM execution_records WHERE kind='completion' AND revoked_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(active, 1);
    assert_eq!(count(&store, "execution_records"), 3); // two completions and the independent progress
}

#[test]
fn failure_after_completion_insert_rolls_back_record_status_and_revision() {
    let mut store = memory();
    let create = request(&store);
    let item = store.create(create, NOW).unwrap().item;
    store
        .conn
        .execute_batch(
            "CREATE TRIGGER fail_completion BEFORE UPDATE OF status ON quests
        WHEN NEW.status='completed' BEGIN SELECT RAISE(ABORT, 'injected update failure'); END;",
        )
        .unwrap();
    let result = store.set_completion(completion(&store, &item, true), DATE, NOW);
    assert_eq!(result.err().unwrap().code, "STORAGE");
    let after = store.load_day(DATE, DATE).unwrap().items.remove(0);
    assert!(!after.completed);
    assert_eq!(after.revision, item.revision);
    assert_eq!(after.quest_revision, item.quest_revision);
    assert_eq!(count(&store, "execution_records"), 0);
}

#[test]
fn editing_preserves_completed_snapshot_and_stale_edits_are_rejected() {
    let mut store = memory();
    let create = request(&store);
    let item = store.create(create, NOW).unwrap().item;
    let done = store
        .set_completion(completion(&store, &item, true), DATE, NOW)
        .unwrap()
        .item;
    let draft = TaskDraft {
        goal_ids: vec![],
        parent_id: None,
        title: "新的任务标题".into(),
        description: "新的说明".into(),
        date: DATE.into(),
        slot: done.slot.clone(),
        start_time: done.start_time.clone(),
        end_time: done.end_time.clone(),
    };
    let edited = store
        .edit(
            EditRequest {
                dataset_id: store.dataset_id.clone(),
                plan_id: done.plan_id.clone(),
                expected_revision: done.revision,
                expected_quest_revision: done.quest_revision,
                draft: draft.clone(),
            },
            DATE,
        )
        .unwrap()
        .item;
    assert_eq!(edited.current_title, "新的任务标题");
    assert_eq!(edited.title, "完成技术文档");
    let raw: String = store
        .conn
        .query_row(
            "SELECT content_snapshot FROM execution_records WHERE id=?1",
            [&done.completion_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        read_json::<ContentSnapshot>(&raw).unwrap().title,
        "完成技术文档"
    );
    let stale = store.edit(
        EditRequest {
            dataset_id: store.dataset_id.clone(),
            plan_id: done.plan_id,
            expected_revision: done.revision,
            expected_quest_revision: done.quest_revision,
            draft,
        },
        DATE,
    );
    assert_eq!(stale.err().unwrap().code, "CONFLICT");
}

#[test]
fn invalid_dates_and_cross_midnight_times_never_write_data() {
    let mut store = memory();
    for (date, start, end) in [
        ("2026-02-30", "09:00", "10:00"),
        (DATE, "23:00", "01:00"),
        (DATE, "09:00", "09:00"),
        (DATE, "25:00", "26:00"),
    ] {
        let mut create = request(&store);
        create.draft.date = date.into();
        create.draft.start_time = Some(start.into());
        create.draft.end_time = Some(end.into());
        assert_eq!(store.create(create, NOW).err().unwrap().code, "VALIDATION");
    }
    assert_eq!(count(&store, "quests"), 0);
}

#[test]
fn dates_slots_completion_and_preferences_survive_reopening() {
    let root = std::env::temp_dir().join(format!("lifeisrpg-store-test-{}", Uuid::new_v4()));
    let mut store = Store::open(&root).unwrap();
    let dataset = store.dataset_id.clone();
    let create = request(&store);
    let item = store.create(create, NOW).unwrap().item;
    store
        .set_completion(completion(&store, &item, true), "2026-10-19", NOW)
        .unwrap();
    store
        .save_preferences(PreferencesRequest {
            dataset_id: dataset.clone(),
            preferences: Preferences {
                sound_enabled: false,
                reduced_motion: true,
                revision: 0,
            },
        })
        .unwrap();
    drop(store);
    let reopened = Store::open(&root).unwrap();
    let day = reopened.load_day(DATE, "2026-10-19").unwrap();
    assert_eq!(day.dataset_id, dataset);
    assert_eq!(day.items[0].slot, "afternoon");
    assert_eq!(day.items[0].completed_on.as_deref(), Some("2026-10-19"));
    assert!(!day.preferences.sound_enabled && day.preferences.reduced_motion);
    drop(reopened);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_pointer_never_reinitializes_existing_data() {
    let root = std::env::temp_dir().join(format!("lifeisrpg-pointer-test-{}", Uuid::new_v4()));
    let store = Store::open(&root).unwrap();
    let database = root
        .join("datasets")
        .join(&store.dataset_id)
        .join("lifeisrpg.sqlite3");
    drop(store);
    fs::remove_file(root.join("active.json")).unwrap();
    assert_eq!(Store::open(&root).err().unwrap().code, "CORRUPT_DATA");
    assert!(database.exists());
    fs::remove_dir_all(root).unwrap();
}
