use super::*;
use chrono::{Datelike, Days};
use std::collections::BTreeMap;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewState {
    pub date: String,
    pub reviewed_at: Option<String>,
    pub dismissed_on: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventSummary {
    pub id: String,
    pub title: String,
    pub note: String,
    pub is_milestone: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanReview {
    pub item: TaskItem,
    pub result: String,
    pub can_manage: bool,
    pub destination_on: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityGroup {
    pub key: String,
    pub result: String,
    pub records: Vec<Execution>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaySummary {
    pub date: String,
    pub plans: usize,
    pub completed: usize,
    pub progressed: usize,
    pub unexecuted: usize,
    pub life_events: usize,
    pub reviewed: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DateView {
    pub dataset_id: String,
    pub date: String,
    pub today: String,
    pub plans: Vec<PlanReview>,
    pub records: Vec<Execution>,
    pub activities: Vec<ActivityGroup>,
    pub events: Vec<EventSummary>,
    pub review: ReviewState,
    pub summary: DaySummary,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthView {
    pub month: String,
    pub days: Vec<DaySummary>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewRequest {
    pub dataset_id: String,
    pub date: String,
    pub action: String,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkipRequest {
    pub dataset_id: String,
    pub plan_id: String,
    pub expected_revision: i64,
    pub expected_quest_revision: i64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReschedulePreview {
    pub target: Option<TaskItem>,
    pub fingerprint: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrdinaryReschedule {
    pub request: RescheduleRequest,
    pub fingerprint: String,
}
fn invalid(message: &str) -> AppError {
    AppError::new("VALIDATION", message)
}
fn parsed(on: &str) -> Result<NaiveDate> {
    validate_date(on)?;
    NaiveDate::parse_from_str(on, "%Y-%m-%d").map_err(|_| invalid("日期无效。"))
}
fn day_after(on: &str, days: i64) -> Result<String> {
    Ok(parsed(on)?
        .checked_add_signed(chrono::Duration::days(days))
        .ok_or_else(|| invalid("日期超出范围。"))?
        .format("%Y-%m-%d")
        .to_string())
}
fn review_state(conn: &Connection, on: &str) -> Result<ReviewState> {
    Ok(conn
        .query_row(
            "SELECT reviewed_at,dismissed_on FROM daily_reviews WHERE date=?1",
            [on],
            |r| {
                Ok(ReviewState {
                    date: on.into(),
                    reviewed_at: r.get(0)?,
                    dismissed_on: r.get(1)?,
                })
            },
        )
        .optional()?
        .unwrap_or(ReviewState {
            date: on.into(),
            reviewed_at: None,
            dismissed_on: None,
        }))
}
fn activity_key(conn: &Connection, r: &Execution) -> Result<String> {
    let instance = if r.kind == "completion" {
        r.completion_scope.as_deref() == Some("instance")
    } else if let Some(id) = &r.plan_id {
        conn.query_row(
            "SELECT origin IN ('recurring','makeup') FROM plans WHERE id=?1",
            [id],
            |r| r.get(0),
        )?
    } else {
        false
    };
    Ok(if instance {
        format!("plan:{}", r.plan_id.as_deref().unwrap_or_default())
    } else {
        format!("quest:{}", r.quest_id)
    })
}
fn date_view(conn: &Connection, on: &str, today: &str, dataset: &str) -> Result<DateView> {
    let items = {
        let mut s = conn.prepare(&format!(
            "{ITEM_QUERY} WHERE p.scheduled_on=?1 ORDER BY p.position,p.id"
        ))?;
        let rows = s.query_map([on], item_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let records = quests::execution_rows(conn, "occurred_on", on)?;
    let mut groups: BTreeMap<String, Vec<Execution>> = BTreeMap::new();
    for r in records.iter().filter(|r| r.revoked_at.is_none()) {
        groups
            .entry(activity_key(conn, r)?)
            .or_default()
            .push(r.clone());
    }
    let activities: Vec<_> = groups
        .into_iter()
        .map(|(key, records)| ActivityGroup {
            key,
            result: if records.iter().any(|r| r.kind == "completion") {
                "completed"
            } else {
                "progressed"
            }
            .into(),
            records,
        })
        .collect();
    let mut plans = vec![];
    for item in items {
        let key = if item.origin == "manual" {
            format!("quest:{}", item.quest_id)
        } else {
            format!("plan:{}", item.plan_id)
        };
        let result = activities
            .iter()
            .find(|a| a.key == key)
            .map(|a| a.result.as_str())
            .unwrap_or(if on > today { "planned" } else { "unexecuted" })
            .into();
        let destination_on = item
            .next_plan_id
            .as_ref()
            .map(|id| {
                conn.query_row("SELECT scheduled_on FROM plans WHERE id=?1", [id], |r| {
                    r.get(0)
                })
            })
            .transpose()?;
        let can_manage = item.can_execute && !item.completed && item.disposition == "active";
        plans.push(PlanReview {
            item,
            result,
            can_manage,
            destination_on,
        });
    }
    let events = {
        let mut s=conn.prepare("SELECT id,title,note,is_milestone FROM life_events WHERE event_on=?1 ORDER BY created_at,id")?;
        let rows = s.query_map([on], |r| {
            Ok(EventSummary {
                id: r.get(0)?,
                title: r.get(1)?,
                note: r.get(2)?,
                is_milestone: r.get(3)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let review = review_state(conn, on)?;
    let summary = DaySummary {
        date: on.into(),
        plans: plans
            .iter()
            .filter(|p| p.item.disposition != "withdrawn")
            .count(),
        completed: activities
            .iter()
            .filter(|a| a.result == "completed")
            .count(),
        progressed: activities
            .iter()
            .filter(|a| a.result == "progressed")
            .count(),
        unexecuted: plans
            .iter()
            .filter(|p| p.result == "unexecuted" && p.item.disposition != "withdrawn")
            .count(),
        life_events: events.len(),
        reviewed: review.reviewed_at.is_some(),
    };
    Ok(DateView {
        dataset_id: dataset.into(),
        date: on.into(),
        today: today.into(),
        plans,
        records,
        activities,
        events,
        review,
        summary,
    })
}
fn reschedule_preview(
    conn: &Connection,
    r: &RescheduleRequest,
    today: &str,
) -> Result<ReschedulePreview> {
    validate_id(&r.makeup_id)?;
    let source = query_item(conn, &r.plan_id)?;
    if source.task_mode != "single" || source.origin != "manual" {
        return Err(invalid("请选择普通任务安排；重复任务使用独立补做。"));
    }
    validate_draft(&TaskDraft {
        title: source.title.clone(),
        description: source.description.clone(),
        goal_ids: vec![],
        parent_id: None,
        date: r.date.clone(),
        slot: r.slot.clone(),
        start_time: r.start_time.clone(),
        end_time: r.end_time.clone(),
    })?;
    if r.date.as_str() < today || r.date == source.scheduled_on {
        return Err(invalid("请选择今天或未来，且与原安排不同的日期。"));
    }
    if source.revision != r.expected_revision || source.quest_revision != r.expected_quest_revision
    {
        return Err(conflict());
    }
    if !source.can_execute || source.completed || source.disposition != "active" {
        return Err(invalid("只能重新安排未完成且仍活跃的计划。"));
    }
    let target=conn.query_row(&format!("{ITEM_QUERY} WHERE p.quest_id=?1 AND p.scheduled_on=?2 AND p.disposition='active' ORDER BY p.position,p.id LIMIT 1"),params![source.quest_id,r.date],item_row).optional()?;
    let fingerprint = json(&(
        r,
        source.revision,
        source.quest_revision,
        target.as_ref().map(|t| (&t.plan_id, t.revision)),
    ))?;
    Ok(ReschedulePreview {
        target,
        fingerprint,
    })
}
impl Store {
    pub fn load_date(&mut self, on: &str, today: &str) -> Result<DateView> {
        validate_date(on)?;
        let tx = self.conn.transaction()?;
        recurrence::ensure_range(&tx, on, on)?;
        let result = date_view(&tx, on, today, &self.dataset_id)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn load_calendar(&mut self, month: &str, today: &str) -> Result<MonthView> {
        if month.len() != 7 {
            return Err(invalid("月份无效。"));
        }
        let first = parsed(&format!("{month}-01"))?;
        let next = if first.month() == 12 {
            NaiveDate::from_ymd_opt(first.year() + 1, 1, 1)
        } else {
            NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1)
        }
        .ok_or_else(|| invalid("月份超出范围。"))?;
        let last = next.pred_opt().ok_or_else(|| invalid("月份超出范围。"))?;
        let tx = self.conn.transaction()?;
        recurrence::ensure_range(&tx, &first.to_string(), &last.to_string())?;
        let mut days = vec![];
        let mut day = first;
        while day < next {
            days.push(date_view(&tx, &day.to_string(), today, &self.dataset_id)?.summary);
            day = day
                .checked_add_days(Days::new(1))
                .ok_or_else(|| invalid("日期超出范围。"))?;
        }
        tx.commit()?;
        Ok(MonthView {
            month: month.into(),
            days,
        })
    }
    pub fn review_prompt(&self, today: &str) -> Result<Option<ReviewState>> {
        let state = review_state(&self.conn, &day_after(today, -1)?)?;
        Ok(
            if state.reviewed_at.is_none() && state.dismissed_on.as_deref() != Some(today) {
                Some(state)
            } else {
                None
            },
        )
    }
    pub fn save_review(&mut self, r: ReviewRequest, today: &str, now: &str) -> Result<bool> {
        self.check_dataset(&r.dataset_id)?;
        validate_date(&r.date)?;
        if r.date.as_str() > today {
            return Err(invalid("未来日期不能标记已回顾。"));
        }
        let tx = self.conn.transaction()?;
        match r.action.as_str() {
            "reviewed" => {
                tx.execute("INSERT INTO daily_reviews(date,reviewed_at) VALUES (?1,?2) ON CONFLICT(date) DO UPDATE SET reviewed_at=COALESCE(reviewed_at,excluded.reviewed_at)",params![r.date,now])?;
            }
            "dismiss" if r.date == day_after(today, -1)? => {
                tx.execute("INSERT INTO daily_reviews(date,dismissed_on) VALUES (?1,?2) ON CONFLICT(date) DO UPDATE SET dismissed_on=excluded.dismissed_on",params![r.date,today])?;
            }
            _ => return Err(invalid("回顾操作无效。")),
        }
        tx.commit()?;
        Ok(true)
    }
    pub fn skip_plan(&mut self, r: SkipRequest) -> Result<bool> {
        self.check_dataset(&r.dataset_id)?;
        let tx = self.conn.transaction()?;
        let item = query_item(&tx, &r.plan_id)?;
        if item.disposition == "skipped" {
            return Ok(false);
        }
        if item.revision != r.expected_revision || item.quest_revision != r.expected_quest_revision
        {
            return Err(conflict());
        }
        if !item.can_execute || item.completed || item.disposition != "active" {
            return Err(invalid(
                "只能放弃未完成且仍活跃的这次计划；任务本身和已有进展会保留。",
            ));
        }
        tx.execute(
            "UPDATE plans SET disposition='skipped',revision=revision+1 WHERE id=?1",
            [&item.plan_id],
        )?;
        tx.commit()?;
        Ok(true)
    }
    pub fn preview_ordinary_reschedule(
        &self,
        r: RescheduleRequest,
        today: &str,
    ) -> Result<ReschedulePreview> {
        self.check_dataset(&r.dataset_id)?;
        reschedule_preview(&self.conn, &r, today)
    }
    pub fn reschedule_ordinary(
        &mut self,
        r: OrdinaryReschedule,
        today: &str,
        now: &str,
    ) -> Result<MutationResult> {
        let req = r.request;
        self.check_dataset(&req.dataset_id)?;
        let tx = self.conn.transaction()?;
        let source = query_item(&tx, &req.plan_id)?;
        validate_id(&req.makeup_id)?;
        validate_date(&req.date)?;
        if source.task_mode != "single" || source.origin != "manual" {
            return Err(invalid("请选择普通任务安排。"));
        }
        if let Some(next) = &source.next_plan_id {
            let item = query_item(&tx, next)?;
            if item.scheduled_on != req.date
                || (item.plan_id == req.makeup_id
                    && (item.slot != req.slot
                        || item.start_time != req.start_time
                        || item.end_time != req.end_time))
            {
                return Err(conflict());
            }
            return Ok(MutationResult {
                item,
                changed: false,
            });
        }
        let preview = reschedule_preview(&tx, &req, today)?;
        if preview.fingerprint != r.fingerprint {
            return Err(conflict());
        }
        let target = if let Some(item) = preview.target {
            item
        } else {
            let snapshot = quests::current_snapshot(&tx, &source.quest_id)?;
            tx.execute("INSERT INTO plans(id,quest_id,scheduled_on,slot,start_time,end_time,content_snapshot,position,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,(SELECT COALESCE(MAX(position),0)+1 FROM plans WHERE scheduled_on=?3),?8)",params![req.makeup_id,source.quest_id,req.date,req.slot,req.start_time,req.end_time,json(&snapshot)?,now])?;
            query_item(&tx, &req.makeup_id)?
        };
        tx.execute("UPDATE plans SET disposition='rescheduled',next_plan_id=?1,revision=revision+1 WHERE id=?2",params![target.plan_id,source.plan_id])?;
        tx.commit()?;
        Ok(MutationResult {
            item: target,
            changed: true,
        })
    }
}
#[cfg(test)]
mod tests;
