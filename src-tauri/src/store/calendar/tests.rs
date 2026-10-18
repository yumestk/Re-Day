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
    }
}
fn draft(on: &str) -> TaskDraft {
    TaskDraft {
        title: "编写技术文档".into(),
        description: "完成架构".into(),
        parent_id: None,
        goal_ids: vec![],
        date: on.into(),
        slot: "afternoon".into(),
        start_time: Some("14:00".into()),
        end_time: Some("15:00".into()),
    }
}
fn create(s: &mut Store, on: &str) -> TaskItem {
    s.create(
        CreateRequest {
            dataset_id: s.dataset_id.clone(),
            quest_id: Uuid::new_v4().to_string(),
            plan_id: Uuid::new_v4().to_string(),
            draft: draft(on),
        },
        NOW,
    )
    .unwrap()
    .item
}
fn act(s: &mut Store, item: &TaskItem, kind: &str, on: &str) {
    s.record_execution(
        ExecutionRequest {
            dataset_id: s.dataset_id.clone(),
            quest_id: item.quest_id.clone(),
            plan_id: Some(item.plan_id.clone()),
            record_id: Uuid::new_v4().to_string(),
            kind: kind.into(),
            occurred_on: on.into(),
            note: "保留独立推进".into(),
            expected_revision: item.quest_revision,
            expected_plan_revision: Some(item.revision),
            completion_id: item.completion_id.clone(),
        },
        TODAY,
        NOW,
    )
    .unwrap();
}
fn request(s: &Store, item: &TaskItem, on: &str) -> RescheduleRequest {
    RescheduleRequest {
        dataset_id: s.dataset_id.clone(),
        plan_id: item.plan_id.clone(),
        makeup_id: Uuid::new_v4().to_string(),
        expected_revision: item.revision,
        expected_quest_revision: item.quest_revision,
        date: on.into(),
        slot: "evening".into(),
        start_time: Some("19:00".into()),
        end_time: None,
    }
}
fn skip(s: &Store, item: &TaskItem) -> SkipRequest {
    SkipRequest {
        dataset_id: s.dataset_id.clone(),
        plan_id: item.plan_id.clone(),
        expected_revision: item.revision,
        expected_quest_revision: item.quest_revision,
    }
}
fn daily(s: &mut Store, on: &str) -> TaskItem {
    let q = s
        .save_quest(
            SaveQuestRequest {
                dataset_id: s.dataset_id.clone(),
                quest_id: Uuid::new_v4().to_string(),
                expected_revision: None,
                draft: QuestDraft {
                    title: "日语输入".into(),
                    description: "".into(),
                    parent_id: None,
                    goal_ids: vec![],
                },
            },
            TODAY,
            NOW,
        )
        .unwrap();
    let r = RuleRequest {
        dataset_id: s.dataset_id.clone(),
        quest_id: q.id.clone(),
        version_id: Uuid::new_v4().to_string(),
        expected_revision: q.revision,
        frequency: "daily".into(),
        interval_days: None,
        weekdays: None,
        end_on: None,
        enabled: true,
        draft: draft(on),
    };
    let preview = s.preview_rule(r.clone(), TODAY).unwrap();
    s.save_rule(
        CommitRuleRequest {
            request: r,
            fingerprint: preview.fingerprint,
        },
        TODAY,
    )
    .unwrap();
    s.load_date(on, TODAY)
        .unwrap()
        .plans
        .into_iter()
        .find(|p| p.item.quest_id == q.id)
        .unwrap()
        .item
}
#[test]
fn ordinary_reschedule_reuses_target_preserves_times_progress_and_actual_dates() {
    let mut s = store();
    let source = create(&mut s, "2026-10-16");
    act(&mut s, &source, "progress", "2026-10-16");
    let q = quests::quest(&s.conn, &source.quest_id).unwrap();
    let target = s
        .schedule(
            ScheduleRequest {
                dataset_id: s.dataset_id.clone(),
                quest_id: q.id.clone(),
                plan_id: Uuid::new_v4().to_string(),
                expected_revision: q.revision,
                draft: draft(TODAY),
            },
            TODAY,
            NOW,
        )
        .unwrap();
    act(&mut s, &target, "progress", TODAY);
    let source = query_item(&s.conn, &source.plan_id).unwrap();
    let target = query_item(&s.conn, &target.plan_id).unwrap();
    let req = request(&s, &source, TODAY);
    let preview = s.preview_ordinary_reschedule(req.clone(), TODAY).unwrap();
    assert_eq!(preview.target.unwrap().plan_id, target.plan_id);
    let result = s
        .reschedule_ordinary(
            OrdinaryReschedule {
                request: req.clone(),
                fingerprint: preview.fingerprint,
            },
            TODAY,
            NOW,
        )
        .unwrap();
    assert_eq!(result.item.plan_id, target.plan_id);
    assert_eq!(result.item.start_time.as_deref(), Some("14:00"));
    assert_eq!(result.item.revision, target.revision);
    let original = query_item(&s.conn, &source.plan_id).unwrap();
    assert_eq!(original.disposition, "rescheduled");
    assert_eq!(original.next_plan_id, Some(target.plan_id.clone()));
    assert_eq!(original.title, source.title);
    assert!(
        !s.reschedule_ordinary(
            OrdinaryReschedule {
                request: req,
                fingerprint: "stale ignored on replay".into()
            },
            "2026-10-20",
            NOW
        )
        .unwrap()
        .changed
    );
    act(&mut s, &result.item, "completion", TODAY);
    let old = s.load_date("2026-10-16", TODAY).unwrap();
    assert_eq!(old.summary.completed, 0);
    assert_eq!(old.summary.progressed, 1);
    assert_eq!(old.plans[0].result, "progressed");
    assert!(!old.plans[0].can_manage);
    let actual = s.load_date(TODAY, TODAY).unwrap();
    assert_eq!(actual.summary.completed, 1);
    assert_eq!(actual.summary.progressed, 0);
    assert_eq!(actual.records.len(), 2);
    let month = s.load_calendar("2026-10", TODAY).unwrap();
    assert_eq!(month.days[15].completed, 0);
    assert_eq!(month.days[17].completed, 1);
    assert_eq!(s.history(&q.id).unwrap().plans.len(), 2);
}
#[test]
fn new_ordinary_target_and_skip_are_atomic_and_progress_survives() {
    let mut s = store();
    let source = create(&mut s, TODAY);
    act(&mut s, &source, "progress", TODAY);
    let source = query_item(&s.conn, &source.plan_id).unwrap();
    let req = request(&s, &source, "2026-10-19");
    let preview = s.preview_ordinary_reschedule(req.clone(), TODAY).unwrap();
    assert!(preview.target.is_none());
    s.conn.execute_batch("CREATE TRIGGER fail_move BEFORE UPDATE OF next_plan_id ON plans BEGIN SELECT RAISE(ABORT,'move failed'); END;").unwrap();
    assert!(s
        .reschedule_ordinary(
            OrdinaryReschedule {
                request: req.clone(),
                fingerprint: preview.fingerprint.clone()
            },
            TODAY,
            NOW
        )
        .is_err());
    assert!(query_item(&s.conn, &req.makeup_id).is_err());
    assert_eq!(
        query_item(&s.conn, &source.plan_id).unwrap().disposition,
        "active"
    );
    s.conn.execute_batch("DROP TRIGGER fail_move").unwrap();
    let target = s
        .reschedule_ordinary(
            OrdinaryReschedule {
                request: req,
                fingerprint: preview.fingerprint,
            },
            TODAY,
            NOW,
        )
        .unwrap()
        .item;
    assert_eq!(source.quest_id, target.quest_id);
    s.conn.execute_batch("CREATE TRIGGER fail_skip BEFORE UPDATE OF disposition ON plans BEGIN SELECT RAISE(ABORT,'skip failed'); END;").unwrap();
    assert!(s.skip_plan(skip(&s, &target)).is_err());
    assert_eq!(
        query_item(&s.conn, &target.plan_id).unwrap().disposition,
        "active"
    );
    s.conn.execute_batch("DROP TRIGGER fail_skip").unwrap();
    assert!(s.skip_plan(skip(&s, &target)).unwrap());
    assert!(!s.skip_plan(skip(&s, &target)).unwrap());
    assert_eq!(
        quests::quest(&s.conn, &source.quest_id).unwrap().status,
        "open"
    );
    assert_eq!(s.load_date(TODAY, TODAY).unwrap().summary.progressed, 1);
}
#[test]
fn reschedule_preview_rejects_new_target_and_new_target_progress() {
    let mut s = store();
    let source = create(&mut s, TODAY);
    let req = request(&s, &source, "2026-10-19");
    let preview = s.preview_ordinary_reschedule(req.clone(), TODAY).unwrap();
    let q = quests::quest(&s.conn, &source.quest_id).unwrap();
    let target = s
        .schedule(
            ScheduleRequest {
                dataset_id: s.dataset_id.clone(),
                quest_id: q.id,
                plan_id: Uuid::new_v4().to_string(),
                expected_revision: q.revision,
                draft: draft("2026-10-19"),
            },
            TODAY,
            NOW,
        )
        .unwrap();
    assert!(s
        .reschedule_ordinary(
            OrdinaryReschedule {
                request: req,
                fingerprint: preview.fingerprint
            },
            TODAY,
            NOW
        )
        .is_err());
    let source = query_item(&s.conn, &source.plan_id).unwrap();
    let req = request(&s, &source, "2026-10-19");
    let preview = s.preview_ordinary_reschedule(req.clone(), TODAY).unwrap();
    act(&mut s, &target, "progress", TODAY);
    assert!(s
        .reschedule_ordinary(
            OrdinaryReschedule {
                request: req,
                fingerprint: preview.fingerprint
            },
            TODAY,
            NOW
        )
        .is_err());
    assert_eq!(
        query_item(&s.conn, &source.plan_id).unwrap().disposition,
        "active"
    );
}
#[test]
fn recurring_skip_does_not_stop_next_day_and_late_completion_is_counted_once() {
    let mut s = store();
    let source = daily(&mut s, TODAY);
    act(&mut s, &source, "progress", TODAY);
    let source = query_item(&s.conn, &source.plan_id).unwrap();
    s.skip_plan(skip(&s, &source)).unwrap();
    let next = s
        .load_date("2026-10-19", TODAY)
        .unwrap()
        .plans
        .remove(0)
        .item;
    assert_eq!(next.disposition, "active");
    assert!(!next.completed);
    let old = s.load_date(TODAY, TODAY).unwrap();
    assert_eq!(old.summary.progressed, 1);
    assert_eq!(old.plans[0].item.disposition, "skipped");
    let other = daily(&mut s, TODAY);
    s.record_execution(
        ExecutionRequest {
            dataset_id: s.dataset_id.clone(),
            quest_id: other.quest_id.clone(),
            plan_id: Some(other.plan_id.clone()),
            record_id: Uuid::new_v4().to_string(),
            kind: "completion".into(),
            occurred_on: "2026-10-19".into(),
            note: "次日实际完成".into(),
            expected_revision: other.quest_revision,
            expected_plan_revision: Some(other.revision),
            completion_id: None,
        },
        "2026-10-19",
        NOW,
    )
    .unwrap();
    assert_eq!(
        s.load_date(TODAY, "2026-10-19").unwrap().summary.completed,
        0
    );
    let actual = s.load_date("2026-10-19", "2026-10-19").unwrap();
    assert_eq!(actual.summary.completed, 1);
    assert_eq!(actual.activities.len(), 1);
    assert!(actual.plans.iter().all(|p| p.result != "completed"));
}
#[test]
fn backfill_unplanned_events_and_revocations_use_actual_date_and_snapshot() {
    let mut s = store();
    let p = create(&mut s, TODAY);
    let q = quests::quest(&s.conn, &p.quest_id).unwrap();
    let r = ExecutionRequest {
        dataset_id: s.dataset_id.clone(),
        quest_id: q.id.clone(),
        plan_id: None,
        record_id: Uuid::new_v4().to_string(),
        kind: "progress".into(),
        occurred_on: "2026-10-16".into(),
        note: "无计划历史补记".into(),
        expected_revision: q.revision,
        expected_plan_revision: None,
        completion_id: None,
    };
    s.record_execution(r, TODAY, NOW).unwrap();
    s.conn.execute("INSERT INTO life_events(id,title,event_on,note,is_milestone,created_at) VALUES (?1,'第一个成果','2026-10-16','真实事件',1,?2)",params![Uuid::new_v4().to_string(),NOW]).unwrap();
    let view = s.load_date("2026-10-16", TODAY).unwrap();
    assert_eq!(view.summary.progressed, 1);
    assert_eq!(view.summary.plans, 0);
    assert_eq!(view.events.len(), 1);
    assert!(view.events[0].is_milestone);
    assert_eq!(view.records[0].snapshot.title, "编写技术文档");
    let p = query_item(&s.conn, &p.plan_id).unwrap();
    act(&mut s, &p, "completion", TODAY);
    let p = query_item(&s.conn, &p.plan_id).unwrap();
    act(&mut s, &p, "undo", TODAY);
    assert_eq!(s.load_date(TODAY, TODAY).unwrap().summary.completed, 0);
    assert_eq!(
        s.load_date("2026-10-16", TODAY).unwrap().summary.progressed,
        1
    );
}
#[test]
fn review_is_explicit_idempotent_and_dismiss_is_separate_from_completion() {
    let mut s = store();
    let old = "2026-10-17";
    assert_eq!(s.review_prompt(TODAY).unwrap().unwrap().date, old);
    s.load_date(old, TODAY).unwrap();
    assert!(s.review_prompt(TODAY).unwrap().is_some());
    s.save_review(
        ReviewRequest {
            dataset_id: s.dataset_id.clone(),
            date: old.into(),
            action: "dismiss".into(),
        },
        TODAY,
        NOW,
    )
    .unwrap();
    assert!(s.review_prompt(TODAY).unwrap().is_none());
    assert!(!s.load_date(old, TODAY).unwrap().summary.reviewed);
    assert!(s.review_prompt("2026-10-19").unwrap().is_some());
    s.save_review(
        ReviewRequest {
            dataset_id: s.dataset_id.clone(),
            date: old.into(),
            action: "reviewed".into(),
        },
        TODAY,
        NOW,
    )
    .unwrap();
    s.save_review(
        ReviewRequest {
            dataset_id: s.dataset_id.clone(),
            date: old.into(),
            action: "reviewed".into(),
        },
        TODAY,
        "later retry",
    )
    .unwrap();
    assert_eq!(
        s.load_date(old, TODAY)
            .unwrap()
            .review
            .reviewed_at
            .as_deref(),
        Some(NOW)
    );
    assert!(s
        .save_review(
            ReviewRequest {
                dataset_id: s.dataset_id.clone(),
                date: "2026-10-19".into(),
                action: "reviewed".into()
            },
            TODAY,
            NOW
        )
        .is_err());
    let pending = create(&mut s, TODAY);
    s.save_review(
        ReviewRequest {
            dataset_id: s.dataset_id.clone(),
            date: TODAY.into(),
            action: "reviewed".into(),
        },
        TODAY,
        NOW,
    )
    .unwrap();
    assert!(s.load_date(TODAY, TODAY).unwrap().summary.reviewed);
    assert_eq!(
        query_item(&s.conn, &pending.plan_id).unwrap().disposition,
        "active"
    );
}
#[test]
fn calendar_ranges_validate_and_ended_plan_times_cannot_be_rewritten() {
    let mut s = store();
    let source = create(&mut s, TODAY);
    s.skip_plan(skip(&s, &source)).unwrap();
    let ended = query_item(&s.conn, &source.plan_id).unwrap();
    let mut d = draft(TODAY);
    d.slot = "evening".into();
    assert!(s
        .edit(
            EditRequest {
                dataset_id: s.dataset_id.clone(),
                plan_id: ended.plan_id,
                expected_revision: ended.revision,
                expected_quest_revision: ended.quest_revision,
                draft: d
            },
            TODAY
        )
        .is_err());
    assert_eq!(s.load_calendar("2028-02", TODAY).unwrap().days.len(), 29);
    assert_eq!(s.load_calendar("2026-12", TODAY).unwrap().days.len(), 31);
    assert!(s.load_calendar("2026-13", TODAY).is_err());
    assert!(s.load_calendar("../2026", TODAY).is_err());
    let q = quests::quest(&s.conn, &source.quest_id).unwrap();
    s.schedule(
        ScheduleRequest {
            dataset_id: s.dataset_id.clone(),
            quest_id: q.id,
            plan_id: Uuid::new_v4().to_string(),
            expected_revision: q.revision,
            draft: draft("2026-10-16"),
        },
        TODAY,
        NOW,
    )
    .unwrap();
    let past = s.load_date("2026-10-16", TODAY).unwrap();
    assert_eq!(past.summary.completed, 0);
    assert_eq!(past.summary.progressed, 0);
    assert_eq!(past.summary.unexecuted, 1);
}

#[test]
fn calendar_generation_failure_rolls_back_the_entire_visible_range() {
    let mut s = store();
    let item = daily(&mut s, TODAY);
    s.conn.execute_batch("CREATE TRIGGER fail_month BEFORE INSERT ON plans WHEN NEW.scheduled_on='2026-10-20' BEGIN SELECT RAISE(ABORT,'month interrupted'); END;").unwrap();
    assert!(s.load_calendar("2026-10", TODAY).is_err());
    assert_eq!(
        s.conn
            .query_row(
                "SELECT count(*) FROM plans WHERE scheduled_on='2026-10-19'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        query_item(&s.conn, &item.plan_id).unwrap().disposition,
        "active"
    );
    s.conn.execute_batch("DROP TRIGGER fail_month").unwrap();
    assert_eq!(s.load_calendar("2026-10", TODAY).unwrap().days[18].plans, 1);
    assert_eq!(s.load_calendar("2026-10", TODAY).unwrap().days[18].plans, 1);
}
