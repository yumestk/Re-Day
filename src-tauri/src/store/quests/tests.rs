use super::*;
const TODAY: &str = "2026-10-18";
const NOW: &str = "2026-10-18T08:00:00.000Z";
fn store() -> Store {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!("../../schema.sql"))
        .unwrap();
    Store {
        dataset_id: Uuid::new_v4().to_string(),
        conn,
        root: std::path::PathBuf::new(),
        blocked: false,
    }
}
fn add_goal(s: &mut Store, name: &str) -> Goal {
    s.save_goal(
        SaveGoalRequest {
            dataset_id: s.dataset_id.clone(),
            goal: Goal {
                id: Uuid::new_v4().to_string(),
                name: name.into(),
                description: "".into(),
                kind: "main".into(),
                status: "open".into(),
                archived: false,
                revision: 0,
            },
            expected_revision: None,
        },
        TODAY,
        NOW,
    )
    .unwrap()
}
fn add(s: &mut Store, title: &str, parent: Option<String>, goals: Vec<String>) -> Quest {
    s.save_quest(
        SaveQuestRequest {
            dataset_id: s.dataset_id.clone(),
            quest_id: Uuid::new_v4().to_string(),
            expected_revision: None,
            draft: QuestDraft {
                title: title.into(),
                description: "".into(),
                parent_id: parent,
                goal_ids: goals,
            },
        },
        TODAY,
        NOW,
    )
    .unwrap()
}
fn record(s: &Store, q: &Quest, kind: &str, date: &str) -> ExecutionRequest {
    ExecutionRequest {
        dataset_id: s.dataset_id.clone(),
        quest_id: q.id.clone(),
        plan_id: None,
        record_id: Uuid::new_v4().to_string(),
        kind: kind.into(),
        occurred_on: date.into(),
        note: "真实推进".into(),
        expected_revision: q.revision,
        expected_plan_revision: None,
        completion_id: q.completion_id.clone(),
    }
}
#[test]
fn backfill_uses_confirmed_current_context_preserves_snapshot_and_progress_on_undo() {
    let mut s = store();
    let g = add_goal(&mut s, "原目标");
    let q = add(&mut s, "原任务", None, vec![g.id.clone()]);
    let r = record(&s, &q, "progress", "2026-10-16");
    s.record_execution(r.clone(), TODAY, NOW).unwrap();
    assert!(!s.record_execution(r, TODAY, NOW).unwrap());
    let q = quest(&s.conn, &q.id).unwrap();
    assert_eq!(q.status, "open");
    let q = s
        .save_quest(
            SaveQuestRequest {
                dataset_id: s.dataset_id.clone(),
                quest_id: q.id.clone(),
                expected_revision: Some(q.revision),
                draft: QuestDraft {
                    title: "新任务".into(),
                    description: "新内容".into(),
                    parent_id: None,
                    goal_ids: vec![],
                },
            },
            TODAY,
            NOW,
        )
        .unwrap();
    let events = s.load_day("2026-10-16", TODAY).unwrap().records;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].snapshot.title, "原任务");
    assert_eq!(events[0].snapshot.goals[0]["name"], "原目标");
    assert_eq!(events[0].recorded_at, NOW);
    assert!(events[0].plan_id.is_none());
    assert_eq!(
        s.conn
            .query_row("SELECT count(*) FROM plans", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    s.record_execution(record(&s, &q, "completion", TODAY), TODAY, NOW)
        .unwrap();
    let done = quest(&s.conn, &q.id).unwrap();
    s.record_execution(record(&s, &done, "undo", TODAY), TODAY, NOW)
        .unwrap();
    assert_eq!(s.load_day("2026-10-16", TODAY).unwrap().records.len(), 1);
}
#[test]
fn parent_goal_and_child_completion_are_independent_and_cycles_rejected() {
    let mut s = store();
    let g = add_goal(&mut s, "目标");
    let parent = add(&mut s, "父任务", None, vec![g.id.clone()]);
    let child = add(
        &mut s,
        "子任务",
        Some(parent.id.clone()),
        vec![g.id.clone()],
    );
    let err = s
        .save_quest(
            SaveQuestRequest {
                dataset_id: s.dataset_id.clone(),
                quest_id: parent.id.clone(),
                expected_revision: Some(parent.revision),
                draft: QuestDraft {
                    title: parent.title.clone(),
                    description: "".into(),
                    parent_id: Some(child.id.clone()),
                    goal_ids: parent.goal_ids.clone(),
                },
            },
            TODAY,
            NOW,
        )
        .err()
        .unwrap();
    assert_eq!(err.code, "VALIDATION");
    s.record_execution(record(&s, &parent, "completion", TODAY), TODAY, NOW)
        .unwrap();
    assert_eq!(quest(&s.conn, &child.id).unwrap().status, "open");
    assert_eq!(s.workspace(TODAY).unwrap().goals[0].status, "open");
}
#[test]
fn delete_rechecks_preview_promotes_children_and_really_removes_history() {
    let mut s = store();
    let parent = add(&mut s, "父任务", None, vec![]);
    let child = add(&mut s, "子任务", Some(parent.id.clone()), vec![]);
    let stale = s.preview_delete(&s.dataset_id, &parent.id).unwrap();
    s.record_execution(record(&s, &parent, "progress", TODAY), TODAY, NOW)
        .unwrap();
    assert_eq!(
        s.delete_quest(DeleteRequest {
            dataset_id: s.dataset_id.clone(),
            preview: stale,
            confirmed_history: true
        })
        .err()
        .unwrap()
        .code,
        "CONFLICT"
    );
    let p = s.preview_delete(&s.dataset_id, &parent.id).unwrap();
    assert!(p.has_history);
    assert_eq!(
        s.delete_quest(DeleteRequest {
            dataset_id: s.dataset_id.clone(),
            preview: p.clone(),
            confirmed_history: false
        })
        .err()
        .unwrap()
        .code,
        "VALIDATION"
    );
    s.conn.execute_batch("CREATE TRIGGER fail_delete BEFORE DELETE ON quests BEGIN SELECT RAISE(ABORT,'injected delete failure'); END;").unwrap();
    assert!(s
        .delete_quest(DeleteRequest {
            dataset_id: s.dataset_id.clone(),
            preview: p.clone(),
            confirmed_history: true
        })
        .is_err());
    assert_eq!(
        quest(&s.conn, &child.id).unwrap().parent_id,
        Some(parent.id.clone())
    );
    assert_eq!(s.load_day(TODAY, TODAY).unwrap().records.len(), 1);
    s.conn.execute_batch("DROP TRIGGER fail_delete;").unwrap();
    s.delete_quest(DeleteRequest {
        dataset_id: s.dataset_id.clone(),
        preview: p,
        confirmed_history: true,
    })
    .unwrap();
    assert!(quest(&s.conn, &parent.id).is_err());
    assert!(quest(&s.conn, &child.id).unwrap().parent_id.is_none());
    assert!(s.load_day(TODAY, TODAY).unwrap().records.is_empty());
}
#[test]
fn archive_hides_active_tasks_but_retains_historical_facts() {
    let mut s = store();
    let q = add(&mut s, "归档任务", None, vec![]);
    s.record_execution(record(&s, &q, "progress", "2026-10-16"), TODAY, NOW)
        .unwrap();
    let q = quest(&s.conn, &q.id).unwrap();
    s.quest_state(
        QuestStateRequest {
            dataset_id: s.dataset_id.clone(),
            quest_id: q.id.clone(),
            expected_revision: q.revision,
            action: "archive".into(),
        },
        TODAY,
    )
    .unwrap();
    assert_eq!(s.load_day("2026-10-16", TODAY).unwrap().records.len(), 1);
    assert!(s.workspace(TODAY).unwrap().quests[0].archived_on.is_some());
}

#[test]
fn goal_rename_rejects_stale_context_and_preserves_prior_execution_snapshot() {
    let mut s = store();
    let g = add_goal(&mut s, "旧目标名");
    let q = add(&mut s, "任务", None, vec![g.id.clone()]);
    s.record_execution(record(&s, &q, "progress", TODAY), TODAY, NOW)
        .unwrap();
    let q = quest(&s.conn, &q.id).unwrap();
    let stale = record(&s, &q, "progress", TODAY);
    let mut renamed = g.clone();
    renamed.name = "新目标名".into();
    renamed.status = "completed".into();
    s.save_goal(
        SaveGoalRequest {
            dataset_id: s.dataset_id.clone(),
            goal: renamed,
            expected_revision: Some(g.revision),
        },
        TODAY,
        NOW,
    )
    .unwrap();
    assert_eq!(
        s.record_execution(stale, TODAY, NOW).err().unwrap().code,
        "CONFLICT"
    );
    assert_eq!(
        s.load_day(TODAY, TODAY).unwrap().records[0].snapshot.goals[0]["name"],
        "旧目标名"
    );
    assert_eq!(quest(&s.conn, &q.id).unwrap().status, "open");
}
#[test]
fn deleting_quest_preserves_independent_event_images_other_tasks_and_child_history() {
    let mut s = store();
    let g = add_goal(&mut s, "共同目标");
    let parent = add(&mut s, "父任务", None, vec![g.id.clone()]);
    let child = add(&mut s, "子任务", Some(parent.id.clone()), vec![g.id]);
    s.record_execution(record(&s, &child, "progress", TODAY), TODAY, NOW)
        .unwrap();
    let event = Uuid::new_v4().to_string();
    let asset = Uuid::new_v4().to_string();
    s.conn.execute("INSERT INTO life_events(id,title,event_on,quest_id,created_at) VALUES (?1,'独立生活事件',?2,?3,?4)",params![event,TODAY,parent.id,NOW]).unwrap();
    s.conn.execute("INSERT INTO assets(id,relative_path,media_type,byte_size,sha256) VALUES (?1,'photo.png','image/png',1,'test')",[&asset]).unwrap();
    s.conn
        .execute(
            "INSERT INTO life_event_assets(life_event_id,asset_id,position) VALUES (?1,?2,0)",
            params![event, asset],
        )
        .unwrap();
    let p = s.preview_delete(&s.dataset_id, &parent.id).unwrap();
    assert_eq!(p.life_events, 1);
    s.delete_quest(DeleteRequest {
        dataset_id: s.dataset_id.clone(),
        preview: p,
        confirmed_history: false,
    })
    .unwrap();
    assert!(quest(&s.conn, &child.id).unwrap().parent_id.is_none());
    assert_eq!(s.history(&child.id).unwrap().records.len(), 1);
    assert!(s
        .conn
        .query_row(
            "SELECT quest_id IS NULL FROM life_events WHERE id=?1",
            [event],
            |r| r.get::<_, bool>(0)
        )
        .unwrap());
    assert_eq!(
        s.conn
            .query_row(
                "SELECT count(*) FROM life_event_assets WHERE asset_id=?1",
                [asset],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    assert_eq!(s.workspace(TODAY).unwrap().goals.len(), 1);
}
#[test]
fn scheduling_same_quest_across_dates_is_atomic_and_keeps_identity() {
    let mut s = store();
    let q = add(&mut s, "长期任务", None, vec![]);
    let make = |s: &Store, q: &Quest, date: &str| ScheduleRequest {
        dataset_id: s.dataset_id.clone(),
        quest_id: q.id.clone(),
        plan_id: Uuid::new_v4().to_string(),
        expected_revision: q.revision,
        draft: TaskDraft {
            title: q.title.clone(),
            description: q.description.clone(),
            parent_id: q.parent_id.clone(),
            goal_ids: q.goal_ids.clone(),
            date: date.into(),
            slot: "morning".into(),
            start_time: None,
            end_time: None,
        },
    };
    s.conn.execute_batch("CREATE TRIGGER fail_plan BEFORE INSERT ON plans BEGIN SELECT RAISE(ABORT,'schedule failure'); END;").unwrap();
    assert!(s.schedule(make(&s, &q, TODAY), TODAY, NOW).is_err());
    assert_eq!(quest(&s.conn, &q.id).unwrap().revision, q.revision);
    s.conn.execute_batch("DROP TRIGGER fail_plan;").unwrap();
    s.schedule(make(&s, &q, TODAY), TODAY, NOW).unwrap();
    let q = quest(&s.conn, &q.id).unwrap();
    s.schedule(make(&s, &q, "2026-10-19"), TODAY, NOW).unwrap();
    assert_eq!(s.workspace(TODAY).unwrap().quests.len(), 1);
    assert_eq!(s.history(&q.id).unwrap().plans.len(), 2);
    assert!(s.history(&q.id).unwrap().records.is_empty());
}
