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
fn add(s: &mut Store) -> Quest {
    s.save_quest(
        SaveQuestRequest {
            dataset_id: s.dataset_id.clone(),
            quest_id: Uuid::new_v4().to_string(),
            expected_revision: None,
            draft: QuestDraft {
                title: "学习日语".into(),
                description: "一段输入".into(),
                parent_id: None,
                goal_ids: vec![],
            },
        },
        TODAY,
        NOW,
    )
    .unwrap()
}
fn request(s: &Store, q: &Quest, start: &str) -> RuleRequest {
    RuleRequest {
        dataset_id: s.dataset_id.clone(),
        quest_id: q.id.clone(),
        version_id: Uuid::new_v4().to_string(),
        expected_revision: q.revision,
        frequency: "daily".into(),
        interval_days: None,
        weekdays: None,
        end_on: None,
        enabled: true,
        draft: TaskDraft {
            title: q.title.clone(),
            description: q.description.clone(),
            parent_id: q.parent_id.clone(),
            goal_ids: q.goal_ids.clone(),
            date: start.into(),
            slot: "evening".into(),
            start_time: Some("19:00".into()),
            end_time: None,
        },
    }
}
fn save(s: &mut Store, r: RuleRequest, today: &str) {
    let preview = s.preview_rule(r.clone(), today).unwrap();
    assert!(s
        .save_rule(
            CommitRuleRequest {
                request: r,
                fingerprint: preview.fingerprint
            },
            today
        )
        .unwrap());
}
fn execution(s: &Store, item: &TaskItem, kind: &str, on: &str) -> ExecutionRequest {
    ExecutionRequest {
        dataset_id: s.dataset_id.clone(),
        quest_id: item.quest_id.clone(),
        plan_id: Some(item.plan_id.clone()),
        record_id: Uuid::new_v4().to_string(),
        kind: kind.into(),
        occurred_on: on.into(),
        note: "".into(),
        expected_revision: item.quest_revision,
        expected_plan_revision: Some(item.revision),
        completion_id: item.completion_id.clone(),
    }
}
fn postponement(s: &Store, item: &TaskItem, date: &str) -> RescheduleRequest {
    RescheduleRequest {
        dataset_id: s.dataset_id.clone(),
        plan_id: item.plan_id.clone(),
        makeup_id: Uuid::new_v4().to_string(),
        expected_revision: item.revision,
        expected_quest_revision: item.quest_revision,
        date: date.into(),
        slot: item.slot.clone(),
        start_time: item.start_time.clone(),
        end_time: item.end_time.clone(),
    }
}
#[test]
fn repeated_ranges_have_one_normal_instance_per_day_and_never_auto_carry_over() {
    let mut s = store();
    let q = add(&mut s);
    let r = request(&s, &q, TODAY);
    save(&mut s, r, TODAY);
    let tx = s.conn.transaction().unwrap();
    ensure_range(&tx, TODAY, "2026-10-21").unwrap();
    ensure_range(&tx, "2026-10-19", "2026-10-23").unwrap();
    tx.commit().unwrap();
    assert_eq!(
        s.conn
            .query_row("SELECT count(*) FROM plans", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        6
    );
    let first = s.load_day(TODAY, TODAY).unwrap().items[0].plan_id.clone();
    assert_eq!(s.load_day(TODAY, TODAY).unwrap().items[0].plan_id, first);
    assert_eq!(
        s.load_day("2026-10-19", "2026-10-19").unwrap().items.len(),
        1
    );
}
#[test]
fn makeup_is_separate_and_each_completion_undo_is_local() {
    let mut s = store();
    let q = add(&mut s);
    let r = request(&s, &q, TODAY);
    save(&mut s, r, TODAY);
    let original = s.load_day(TODAY, TODAY).unwrap().items.remove(0);
    let req = postponement(&s, &original, "2026-10-19");
    let makeup = s.reschedule_instance(req.clone(), TODAY, NOW).unwrap().item;
    assert!(
        !s.reschedule_instance(req.clone(), TODAY, NOW)
            .unwrap()
            .changed
    );
    // An acknowledged intent remains a no-op even after its target date has passed.
    assert!(
        !s.reschedule_instance(req, "2026-10-20", NOW)
            .unwrap()
            .changed
    );
    let day = s.load_day("2026-10-19", "2026-10-19").unwrap();
    assert_eq!(day.items.len(), 2);
    let normal = day
        .items
        .into_iter()
        .find(|p| p.origin == "recurring")
        .unwrap();
    assert_ne!(normal.plan_id, makeup.plan_id);
    s.record_execution(
        execution(&s, &normal, "completion", "2026-10-19"),
        "2026-10-19",
        NOW,
    )
    .unwrap();
    let still_open = query_item(&s.conn, &makeup.plan_id).unwrap();
    assert!(!still_open.completed);
    assert_eq!(quests::quest(&s.conn, &q.id).unwrap().status, "open");
    s.record_execution(
        execution(&s, &still_open, "progress", "2026-10-19"),
        "2026-10-19",
        NOW,
    )
    .unwrap();
    let still_open = query_item(&s.conn, &makeup.plan_id).unwrap();
    s.record_execution(
        execution(&s, &still_open, "completion", "2026-10-19"),
        "2026-10-19",
        NOW,
    )
    .unwrap();
    let done = query_item(&s.conn, &makeup.plan_id).unwrap();
    s.record_execution(
        execution(&s, &done, "undo", "2026-10-19"),
        "2026-10-19",
        NOW,
    )
    .unwrap();
    assert!(query_item(&s.conn, &normal.plan_id).unwrap().completed);
    assert_eq!(
        s.history(&q.id)
            .unwrap()
            .records
            .iter()
            .filter(|r| r.kind == "progress")
            .count(),
        1
    );
    let old = query_item(&s.conn, &original.plan_id).unwrap();
    assert_eq!(old.disposition, "rescheduled");
    assert_eq!(old.next_plan_id, Some(makeup.plan_id));
}
#[test]
fn versions_preserve_past_and_overrides_and_old_retry_never_restores_a_rule() {
    let mut s = store();
    let q = add(&mut s);
    let first = request(&s, &q, TODAY);
    save(&mut s, first.clone(), TODAY);
    let past = s.load_day(TODAY, TODAY).unwrap().items.remove(0);
    let future = s.load_day("2026-10-21", TODAY).unwrap().items.remove(0);
    s.edit(
        EditRequest {
            dataset_id: s.dataset_id.clone(),
            plan_id: future.plan_id.clone(),
            expected_revision: future.revision,
            expected_quest_revision: future.quest_revision,
            draft: TaskDraft {
                title: "单次覆盖".into(),
                description: "单次".into(),
                date: future.scheduled_on.clone(),
                slot: future.slot.clone(),
                start_time: future.start_time.clone(),
                end_time: future.end_time.clone(),
                parent_id: None,
                goal_ids: vec![],
            },
        },
        TODAY,
    )
    .unwrap();
    let q = quests::quest(&s.conn, &q.id).unwrap();
    let mut second = request(&s, &q, "2026-10-19");
    second.draft.title = "未来内容".into();
    let preview = s.preview_rule(second.clone(), TODAY).unwrap();
    assert_eq!(preview.retained_overrides, vec!["2026-10-21"]);
    assert_eq!(quests::quest(&s.conn, &q.id).unwrap().revision, q.revision);
    s.save_rule(
        CommitRuleRequest {
            request: second,
            fingerprint: preview.fingerprint,
        },
        TODAY,
    )
    .unwrap();
    assert_eq!(
        query_item(&s.conn, &past.plan_id).unwrap().title,
        "学习日语"
    );
    assert_eq!(
        query_item(&s.conn, &future.plan_id).unwrap().title,
        "单次覆盖"
    );
    assert_eq!(
        s.load_day("2026-10-20", "2026-10-22").unwrap().items[0].title,
        "未来内容"
    );
    assert!(!s
        .save_rule(
            CommitRuleRequest {
                request: first,
                fingerprint: "old".into()
            },
            "2026-10-22"
        )
        .unwrap());
    assert_eq!(
        s.load_day("2026-10-23", "2026-10-23").unwrap().items[0].title,
        "未来内容"
    );
}
#[test]
fn leaf_restriction_conversion_history_and_non_rule_date_are_enforced() {
    let mut s = store();
    let q = add(&mut s);
    let child = s
        .save_quest(
            SaveQuestRequest {
                dataset_id: s.dataset_id.clone(),
                quest_id: Uuid::new_v4().to_string(),
                expected_revision: None,
                draft: QuestDraft {
                    title: "子任务".into(),
                    description: "".into(),
                    parent_id: Some(q.id.clone()),
                    goal_ids: vec![],
                },
            },
            TODAY,
            NOW,
        )
        .unwrap();
    assert_eq!(
        s.preview_rule(request(&s, &q, TODAY), TODAY)
            .err()
            .unwrap()
            .code,
        "VALIDATION"
    );
    let preview = s.preview_delete(&s.dataset_id, &child.id).unwrap();
    s.delete_quest(DeleteRequest {
        dataset_id: s.dataset_id.clone(),
        preview,
        confirmed_history: false,
    })
    .unwrap();
    let mut r = request(&s, &q, TODAY);
    r.frequency = "weekly".into();
    r.weekdays = Some(1);
    save(&mut s, r, TODAY);
    assert!(s
        .instances(&s.dataset_id.clone(), &q.id, TODAY)
        .unwrap()
        .is_empty());
    let q = quests::quest(&s.conn, &q.id).unwrap();
    let no_plan = ExecutionRequest {
        dataset_id: s.dataset_id.clone(),
        quest_id: q.id.clone(),
        plan_id: None,
        record_id: Uuid::new_v4().to_string(),
        kind: "completion".into(),
        occurred_on: TODAY.into(),
        note: "".into(),
        expected_revision: q.revision,
        expected_plan_revision: None,
        completion_id: None,
    };
    assert_eq!(
        s.record_execution(no_plan, TODAY, NOW).err().unwrap().code,
        "VALIDATION"
    );
    assert!(s
        .save_quest(
            SaveQuestRequest {
                dataset_id: s.dataset_id.clone(),
                quest_id: Uuid::new_v4().to_string(),
                expected_revision: None,
                draft: QuestDraft {
                    title: "非法子任务".into(),
                    description: "".into(),
                    parent_id: Some(q.id),
                    goal_ids: vec![]
                }
            },
            TODAY,
            NOW
        )
        .is_err());
}
#[test]
fn archive_stops_future_normals_keeps_history_and_makeup_without_auto_resume() {
    let mut s = store();
    let q = add(&mut s);
    let r = request(&s, &q, TODAY);
    save(&mut s, r, TODAY);
    let original = s.load_day(TODAY, TODAY).unwrap().items.remove(0);
    s.load_day("2026-10-19", TODAY).unwrap();
    let makeup = s
        .reschedule_instance(postponement(&s, &original, "2026-10-19"), TODAY, NOW)
        .unwrap()
        .item;
    let q = quests::quest(&s.conn, &q.id).unwrap();
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
    let day = s.load_day("2026-10-19", "2026-10-19").unwrap();
    assert_eq!(day.items.len(), 1);
    assert_eq!(day.items[0].plan_id, makeup.plan_id);
    assert!(day.items[0].can_execute);
    s.record_execution(
        execution(&s, &day.items[0], "completion", "2026-10-19"),
        "2026-10-19",
        NOW,
    )
    .unwrap();
    let q = quests::quest(&s.conn, &q.id).unwrap();
    s.quest_state(
        QuestStateRequest {
            dataset_id: s.dataset_id.clone(),
            quest_id: q.id,
            expected_revision: q.revision,
            action: "unarchive".into(),
        },
        "2026-10-19",
    )
    .unwrap();
    assert!(s
        .load_day("2026-10-20", "2026-10-20")
        .unwrap()
        .items
        .is_empty());
    assert_eq!(
        query_item(&s.conn, &original.plan_id).unwrap().disposition,
        "rescheduled"
    );
}
#[test]
fn interval_calendar_math_handles_leap_month_and_dst_boundaries() {
    let mut s = store();
    let q = add(&mut s);
    let mut r = request(&s, &q, "2028-02-28");
    r.frequency = "interval".into();
    r.interval_days = Some(2);
    r.end_on = Some("2028-03-05".into());
    save(&mut s, r, TODAY);
    for (day, expected) in [
        ("2028-02-28", 1),
        ("2028-02-29", 0),
        ("2028-03-01", 1),
        ("2028-03-02", 0),
        ("2028-03-05", 1),
        ("2028-03-07", 0),
    ] {
        assert_eq!(s.load_day(day, day).unwrap().items.len(), expected, "{day}");
    }
    let q = quests::quest(&s.conn, &q.id).unwrap();
    let mut r = request(&s, &q, "2028-11-04");
    r.frequency = "interval".into();
    r.interval_days = Some(2);
    save(&mut s, r, TODAY);
    assert!(s
        .load_day("2028-11-05", "2028-11-06")
        .unwrap()
        .items
        .is_empty());
    assert_eq!(
        s.load_day("2028-11-06", "2028-11-06").unwrap().items.len(),
        1
    );
}

#[test]
fn preview_staleness_rule_failure_and_makeup_failure_never_leave_partial_writes() {
    let mut s = store();
    let q = add(&mut s);
    let r = request(&s, &q, TODAY);
    let p = s.preview_rule(r.clone(), TODAY).unwrap();
    assert_eq!(quests::quest(&s.conn, &q.id).unwrap().mode, "single");
    s.conn.execute_batch("CREATE TRIGGER fail_rule BEFORE INSERT ON recurrence_versions BEGIN SELECT RAISE(ABORT,'rule failure'); END;").unwrap();
    assert!(s
        .save_rule(
            CommitRuleRequest {
                request: r.clone(),
                fingerprint: p.fingerprint
            },
            TODAY
        )
        .is_err());
    assert_eq!(quests::quest(&s.conn, &q.id).unwrap().mode, "single");
    assert!(rule_versions(&s.conn, &q.id).unwrap().is_empty());
    s.conn.execute_batch("DROP TRIGGER fail_rule;").unwrap();
    save(&mut s, r, TODAY);
    let q = quests::quest(&s.conn, &q.id).unwrap();
    let r = request(&s, &q, "2026-10-19");
    let p = s.preview_rule(r.clone(), TODAY).unwrap();
    s.load_day("2026-10-20", TODAY).unwrap();
    assert_eq!(
        s.save_rule(
            CommitRuleRequest {
                request: r,
                fingerprint: p.fingerprint
            },
            TODAY
        )
        .err()
        .unwrap()
        .code,
        "CONFLICT"
    );
    let source = s.load_day(TODAY, TODAY).unwrap().items.remove(0);
    let before: i64 = s
        .conn
        .query_row("SELECT count(*) FROM plans", [], |row| row.get(0))
        .unwrap();
    s.conn.execute_batch("CREATE TRIGGER fail_reschedule BEFORE UPDATE OF disposition ON plans WHEN NEW.disposition='rescheduled' BEGIN SELECT RAISE(ABORT,'reschedule failure'); END;").unwrap();
    assert!(s
        .reschedule_instance(postponement(&s, &source, "2026-10-19"), TODAY, NOW)
        .is_err());
    assert_eq!(
        query_item(&s.conn, &source.plan_id).unwrap().disposition,
        "active"
    );
    assert_eq!(
        s.conn
            .query_row("SELECT count(*) FROM plans", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        before
    );
}
#[test]
fn instance_replay_never_resurrects_revoked_completion_or_undoes_another_instance() {
    let mut s = store();
    let q = add(&mut s);
    let r = request(&s, &q, TODAY);
    save(&mut s, r, TODAY);
    let a = s.load_day(TODAY, TODAY).unwrap().items.remove(0);
    let b = s.load_day("2026-10-19", TODAY).unwrap().items.remove(0);
    let done = execution(&s, &a, "completion", TODAY);
    s.record_execution(done.clone(), TODAY, NOW).unwrap();
    let a = query_item(&s.conn, &a.plan_id).unwrap();
    s.record_execution(execution(&s, &b, "completion", TODAY), TODAY, NOW)
        .unwrap();
    let b = query_item(&s.conn, &b.plan_id).unwrap();
    let mut wrong = execution(&s, &b, "undo", TODAY);
    wrong.completion_id = a.completion_id.clone();
    assert_eq!(
        s.record_execution(wrong, TODAY, NOW).err().unwrap().code,
        "CONFLICT"
    );
    let undo = execution(&s, &a, "undo", TODAY);
    s.record_execution(undo.clone(), TODAY, NOW).unwrap();
    assert!(!s.record_execution(done, TODAY, NOW).unwrap());
    let open = query_item(&s.conn, &a.plan_id).unwrap();
    assert!(!open.completed);
    s.record_execution(execution(&s, &open, "completion", TODAY), TODAY, NOW)
        .unwrap();
    assert!(!s.record_execution(undo, TODAY, NOW).unwrap());
    assert!(query_item(&s.conn, &a.plan_id).unwrap().completed);
    assert!(query_item(&s.conn, &b.plan_id).unwrap().completed);
}
#[test]
fn conversion_requires_explicit_undo_and_keeps_old_scope_and_manual_plan_history() {
    let mut s = store();
    let q = add(&mut s);
    let schedule = |s: &Store, q: &Quest, on: &str| ScheduleRequest {
        dataset_id: s.dataset_id.clone(),
        quest_id: q.id.clone(),
        plan_id: Uuid::new_v4().to_string(),
        expected_revision: q.revision,
        draft: TaskDraft {
            title: q.title.clone(),
            description: q.description.clone(),
            parent_id: None,
            goal_ids: vec![],
            date: on.into(),
            slot: "morning".into(),
            start_time: None,
            end_time: None,
        },
    };
    let old = s.schedule(schedule(&s, &q, TODAY), TODAY, NOW).unwrap();
    let q = quests::quest(&s.conn, &q.id).unwrap();
    let future = s
        .schedule(schedule(&s, &q, "2026-10-20"), TODAY, NOW)
        .unwrap();
    let old = query_item(&s.conn, &old.plan_id).unwrap();
    s.record_execution(execution(&s, &old, "completion", TODAY), TODAY, NOW)
        .unwrap();
    let q = quests::quest(&s.conn, &q.id).unwrap();
    assert!(s
        .preview_rule(request(&s, &q, "2026-10-19"), TODAY)
        .is_err());
    let done = query_item(&s.conn, &old.plan_id).unwrap();
    s.record_execution(execution(&s, &done, "undo", TODAY), TODAY, NOW)
        .unwrap();
    let q = quests::quest(&s.conn, &q.id).unwrap();
    let r = request(&s, &q, "2026-10-19");
    save(&mut s, r, TODAY);
    assert_eq!(
        query_item(&s.conn, &future.plan_id).unwrap().disposition,
        "withdrawn"
    );
    let old = query_item(&s.conn, &old.plan_id).unwrap();
    assert!(old.has_records);
    assert!(!old.can_execute);
    assert_eq!(old.origin, "manual");
    let history = s.history(&q.id).unwrap();
    assert_eq!(
        history.records[0].completion_scope.as_deref(),
        Some("quest")
    );
    assert!(history.records[0].revoked_at.is_some());
    let normal = s
        .load_day("2026-10-19", "2026-10-19")
        .unwrap()
        .items
        .remove(0);
    s.record_execution(
        execution(&s, &normal, "completion", "2026-10-19"),
        "2026-10-19",
        NOW,
    )
    .unwrap();
    assert_eq!(quests::quest(&s.conn, &q.id).unwrap().status, "open");
}
#[test]
fn unopened_past_uses_old_versions_and_paused_overrides_survive_explicit_resume() {
    let mut s = store();
    let q = add(&mut s);
    let r = request(&s, &q, TODAY);
    save(&mut s, r, TODAY);
    let q = quests::quest(&s.conn, &q.id).unwrap();
    let mut r = request(&s, &q, "2026-10-21");
    r.draft.title = "新模板".into();
    save(&mut s, r, "2026-10-20");
    assert_eq!(
        s.load_day("2026-10-20", "2026-10-24").unwrap().items[0].title,
        "学习日语"
    );
    assert_eq!(
        s.load_day("2026-10-22", "2026-10-24").unwrap().items[0].title,
        "新模板"
    );
    let p = s
        .load_day("2026-10-25", "2026-10-23")
        .unwrap()
        .items
        .remove(0);
    s.edit(
        EditRequest {
            dataset_id: s.dataset_id.clone(),
            plan_id: p.plan_id.clone(),
            expected_revision: p.revision,
            expected_quest_revision: p.quest_revision,
            draft: TaskDraft {
                title: "保留覆盖".into(),
                description: "".into(),
                parent_id: None,
                goal_ids: vec![],
                date: p.scheduled_on.clone(),
                slot: p.slot.clone(),
                start_time: p.start_time.clone(),
                end_time: p.end_time.clone(),
            },
        },
        "2026-10-23",
    )
    .unwrap();
    let q = quests::quest(&s.conn, &q.id).unwrap();
    let mut r = request(&s, &q, "2026-10-24");
    r.enabled = false;
    save(&mut s, r, "2026-10-23");
    assert!(s
        .load_day("2026-10-25", "2026-10-25")
        .unwrap()
        .items
        .is_empty());
    assert_eq!(query_item(&s.conn, &p.plan_id).unwrap().title, "保留覆盖");
    let q = quests::quest(&s.conn, &q.id).unwrap();
    let r = request(&s, &q, "2026-10-24");
    save(&mut s, r, "2026-10-23");
    let p2 = s
        .load_day("2026-10-25", "2026-10-25")
        .unwrap()
        .items
        .remove(0);
    assert_eq!(p2.plan_id, p.plan_id);
    assert_eq!(p2.title, "保留覆盖");
}
