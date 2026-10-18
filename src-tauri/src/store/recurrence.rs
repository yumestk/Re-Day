use super::*;
use chrono::{Datelike, Days};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuleVersion {
    pub id: String,
    pub quest_id: String,
    pub effective_from: String,
    pub effective_until: Option<String>,
    pub frequency: String,
    pub interval_days: Option<i64>,
    pub weekdays: Option<i64>,
    pub anchor_on: String,
    pub end_on: Option<String>,
    pub enabled: bool,
    pub slot: String,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub snapshot: ContentSnapshot,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleRequest {
    pub dataset_id: String,
    pub quest_id: String,
    pub version_id: String,
    pub expected_revision: i64,
    pub frequency: String,
    pub interval_days: Option<i64>,
    pub weekdays: Option<i64>,
    pub end_on: Option<String>,
    pub enabled: bool,
    #[serde(flatten)]
    pub draft: TaskDraft,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitRuleRequest {
    pub request: RuleRequest,
    pub fingerprint: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RulePreview {
    pub fingerprint: String,
    pub changes: Vec<PlanChange>,
    pub retained_overrides: Vec<String>,
    pub preserved_facts: i64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanChange {
    pub date: String,
    pub title: String,
    pub action: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RescheduleRequest {
    pub dataset_id: String,
    pub plan_id: String,
    pub makeup_id: String,
    pub expected_revision: i64,
    pub expected_quest_revision: i64,
    pub date: String,
    pub slot: String,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
}
fn invalid(message: &str) -> AppError {
    AppError::new("VALIDATION", message)
}
fn date(value: &str) -> Result<NaiveDate> {
    validate_date(value)?;
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| invalid("日期无效。"))
}
fn next_day(value: &str) -> Result<String> {
    Ok(date(value)?
        .checked_add_days(Days::new(1))
        .ok_or_else(|| invalid("日期超出支持范围。"))?
        .format("%Y-%m-%d")
        .to_string())
}

pub(super) fn rule_versions(conn: &Connection, quest_id: &str) -> Result<Vec<RuleVersion>> {
    let mut s=conn.prepare("SELECT id,quest_id,effective_from,effective_until,frequency,interval_days,weekdays,anchor_on,end_on,enabled,slot,start_time,end_time,content_snapshot FROM recurrence_versions WHERE quest_id=?1 ORDER BY effective_from,rowid")?;
    let rows = s.query_map([quest_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, Option<i64>>(5)?,
            r.get::<_, Option<i64>>(6)?,
            r.get::<_, String>(7)?,
            r.get::<_, Option<String>>(8)?,
            r.get::<_, bool>(9)?,
            r.get::<_, String>(10)?,
            r.get::<_, Option<String>>(11)?,
            r.get::<_, Option<String>>(12)?,
            r.get::<_, String>(13)?,
        ))
    })?;
    rows.map(|r| {
        let (
            id,
            quest_id,
            effective_from,
            effective_until,
            frequency,
            interval_days,
            weekdays,
            anchor_on,
            end_on,
            enabled,
            slot,
            start_time,
            end_time,
            snapshot,
        ) = r?;
        Ok(RuleVersion {
            id,
            quest_id,
            effective_from,
            effective_until,
            frequency,
            interval_days,
            weekdays,
            anchor_on,
            end_on,
            enabled,
            slot,
            start_time,
            end_time,
            snapshot: read_json(&snapshot)?,
        })
    })
    .collect()
}
fn matches(rule: &RuleVersion, day: NaiveDate) -> Result<bool> {
    let value = day.format("%Y-%m-%d").to_string();
    if !rule.enabled
        || value < rule.effective_from
        || rule
            .effective_until
            .as_ref()
            .is_some_and(|until| &value >= until)
        || rule.end_on.as_ref().is_some_and(|until| &value > until)
    {
        return Ok(false);
    }
    Ok(match rule.frequency.as_str() {
        "daily" => true,
        "weekly" => rule
            .weekdays
            .is_some_and(|bits| bits & (1_i64 << day.weekday().num_days_from_monday()) != 0),
        "interval" => {
            let n = rule
                .interval_days
                .filter(|n| *n > 0)
                .ok_or_else(|| invalid("间隔规则无效。"))?;
            let delta = day.signed_duration_since(date(&rule.anchor_on)?).num_days();
            delta >= 0 && delta % n == 0
        }
        _ => return Err(invalid("重复规则类型无效。")),
    })
}
fn all_rules(conn: &Connection) -> Result<Vec<RuleVersion>> {
    let ids = {
        let mut s =
            conn.prepare("SELECT DISTINCT quest_id FROM recurrence_versions ORDER BY quest_id")?;
        let rows = s.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut rules = vec![];
    for id in ids {
        rules.extend(rule_versions(conn, &id)?);
    }
    Ok(rules)
}
pub(super) fn ensure_range(conn: &Connection, from: &str, until: &str) -> Result<()> {
    let mut day = date(from)?;
    let end = date(until)?;
    if day > end {
        return Err(invalid("日期范围无效。"));
    }
    let rules = all_rules(conn)?;
    loop {
        let value = day.format("%Y-%m-%d").to_string();
        for rule in &rules {
            if !matches(rule, day)? {
                continue;
            }
            let exists:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM plans WHERE quest_id=?1 AND occurrence_on=?2 AND origin='recurring')",params![rule.quest_id,value],|r|r.get(0))?;
            if !exists {
                conn.execute("INSERT INTO plans(id,quest_id,scheduled_on,slot,start_time,end_time,origin,recurrence_version_id,occurrence_on,content_snapshot,position,created_at) VALUES (?1,?2,?3,?4,?5,?6,'recurring',?7,?3,?8,(SELECT COALESCE(MAX(position),0)+1 FROM plans WHERE scheduled_on=?3),strftime('%Y-%m-%dT%H:%M:%fZ','now'))",params![Uuid::new_v4().to_string(),rule.quest_id,value,rule.slot,rule.start_time,rule.end_time,rule.id,json(&rule.snapshot)?])?;
            }
        }
        if day == end {
            break;
        }
        day = day
            .checked_add_days(Days::new(1))
            .ok_or_else(|| invalid("日期超出支持范围。"))?;
    }
    Ok(())
}
fn close_versions(conn: &Connection, id: &str, cutoff: &str) -> Result<()> {
    conn.execute("UPDATE recurrence_versions SET effective_until=?1 WHERE quest_id=?2 AND effective_from<?1 AND (effective_until IS NULL OR effective_until>?1)",params![cutoff,id])?;
    conn.execute("UPDATE recurrence_versions SET effective_until=effective_from WHERE quest_id=?1 AND effective_from>=?2",params![id,cutoff])?;
    Ok(())
}
pub(super) fn stop_future(conn: &Connection, id: &str, today: &str) -> Result<()> {
    let cutoff = next_day(today)?;
    close_versions(conn, id, &cutoff)?;
    conn.execute("UPDATE plans SET disposition='withdrawn',revision=revision+1 WHERE quest_id=?1 AND origin='recurring' AND scheduled_on>=?2 AND disposition='active' AND NOT EXISTS(SELECT 1 FROM execution_records e WHERE e.plan_id=plans.id)",params![id,cutoff])?;
    Ok(())
}
fn validate_rule(conn: &Connection, r: &RuleRequest, today: &str) -> Result<Quest> {
    validate_id(&r.version_id)?;
    validate_draft(&r.draft)?;
    if r.draft.date.as_str() < today {
        return Err(invalid("新规则或本次及未来修改只能从今天或未来生效。"));
    }
    if let Some(end) = &r.end_on {
        validate_date(end)?;
        if end < &r.draft.date {
            return Err(invalid("截止日期不能早于生效日期。"));
        }
    }
    match r.frequency.as_str() {
        "daily" if r.interval_days.is_none() && r.weekdays.is_none() => {}
        "weekly"
            if r.interval_days.is_none()
                && r.weekdays.is_some_and(|bits| (1..=127).contains(&bits)) => {}
        "interval" if r.weekdays.is_none() && r.interval_days.is_some_and(|days| days > 0) => {}
        _ => return Err(invalid("请选择有效的每日、每周星期或每隔 N 天规则。")),
    }
    let q = quests::quest(conn, &r.quest_id)?;
    if q.revision != r.expected_revision {
        return Err(conflict());
    }
    if q.status != "open" || q.archived_on.is_some() {
        return Err(invalid(
            "请先明确撤销完成，或恢复已取消/归档的任务，再设置重复。",
        ));
    }
    let children: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM quests WHERE parent_id=?1)",
        [&q.id],
        |row| row.get(0),
    )?;
    if children {
        return Err(invalid(
            "有子任务的任务不能设置重复规则；子任务不会被忽略或删除。",
        ));
    }
    quests::validate_links(conn, &q.id, r.draft.parent_id.as_deref(), &r.draft.goal_ids)?;
    Ok(q)
}
fn rule_fingerprint(conn: &Connection, r: &RuleRequest) -> Result<String> {
    let q = quests::quest(conn, &r.quest_id)?;
    let stamps = {
        let mut s = conn.prepare("SELECT id,revision FROM plans WHERE quest_id=?1 ORDER BY id")?;
        let rows = s.query_map([&q.id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    json(&(r, q.revision, rule_versions(conn, &q.id)?, stamps))
}
fn replay(conn: &Connection, r: &RuleRequest) -> Result<bool> {
    let old = rule_versions(conn, &r.quest_id)?
        .into_iter()
        .find(|v| v.id == r.version_id);
    if let Some(v) = old {
        let goal_ids: std::collections::BTreeSet<_> = v
            .snapshot
            .goals
            .iter()
            .filter_map(|g| g["id"].as_str())
            .collect();
        if v.effective_from != r.draft.date
            || v.frequency != r.frequency
            || v.interval_days != r.interval_days
            || v.weekdays != r.weekdays
            || v.end_on != r.end_on
            || v.enabled != r.enabled
            || v.slot != r.draft.slot
            || v.start_time != r.draft.start_time
            || v.end_time != r.draft.end_time
            || v.snapshot.title != r.draft.title.trim()
            || v.snapshot.description != r.draft.description
            || goal_ids != r.draft.goal_ids.iter().map(String::as_str).collect()
        {
            return Err(conflict());
        }
        return Ok(true);
    }
    Ok(false)
}
fn apply_rule(conn: &Connection, r: &RuleRequest, today: &str) -> Result<RulePreview> {
    let q = validate_rule(conn, r, today)?;
    let fingerprint = rule_fingerprint(conn, r)?;
    let before = {
        let mut s = conn.prepare(
            "SELECT id FROM plans WHERE quest_id=?1 AND scheduled_on>=?2 ORDER BY scheduled_on,id",
        )?;
        let rows = s.query_map(params![q.id, today], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    close_versions(conn, &q.id, &r.draft.date)?;
    conn.execute("UPDATE quests SET mode='recurring',title=?1,description=?2,parent_id=?3,revision=revision+1 WHERE id=?4",params![r.draft.title.trim(),r.draft.description,r.draft.parent_id,q.id])?;
    quests::replace_links(conn, &q.id, &r.draft.goal_ids)?;
    let snapshot = quests::current_snapshot(conn, &q.id)?;
    conn.execute("INSERT INTO recurrence_versions(id,quest_id,effective_from,frequency,interval_days,weekdays,anchor_on,end_on,enabled,slot,start_time,end_time,content_snapshot) VALUES (?1,?2,?3,?4,?5,?6,?3,?7,?8,?9,?10,?11,?12)",params![r.version_id,q.id,r.draft.date,r.frequency,r.interval_days,r.weekdays,r.end_on,r.enabled,r.draft.slot,r.draft.start_time,r.draft.end_time,json(&snapshot)?])?;
    let version = rule_versions(conn, &q.id)?
        .into_iter()
        .find(|v| v.id == r.version_id)
        .ok_or_else(conflict)?;
    let mut changes = vec![];
    let mut retained_overrides = vec![];
    let mut preserved_facts = 0;
    for id in before {
        let old = query_item(conn, &id)?;
        let facts: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM execution_records WHERE plan_id=?1)",
            [&id],
            |row| row.get(0),
        )?;
        if facts {
            preserved_facts += 1;
            continue;
        }
        if old.origin != "manual" && old.scheduled_on < r.draft.date {
            continue;
        }
        if old.origin == "makeup" || ["rescheduled", "skipped"].contains(&old.disposition.as_str())
        {
            continue;
        }
        let action = if old.origin == "manual" {
            if q.mode != "single" {
                continue;
            }
            conn.execute(
                "UPDATE plans SET disposition='withdrawn',revision=revision+1 WHERE id=?1",
                [&id],
            )?;
            "旧普通安排保留原行并停止"
        } else if old.is_override
            && r.enabled
            && (old.disposition == "active" || matches(&version, date(&old.scheduled_on)?)?)
        {
            if old.disposition != "active" {
                conn.execute(
                    "UPDATE plans SET disposition='active',revision=revision+1 WHERE id=?1",
                    [&id],
                )?;
            }
            retained_overrides.push(old.scheduled_on.clone());
            continue;
        } else if matches(&version, date(&old.scheduled_on)?)? {
            conn.execute("UPDATE plans SET content_snapshot=?1,slot=?2,start_time=?3,end_time=?4,recurrence_version_id=?5,disposition='active',revision=revision+1 WHERE id=?6",params![json(&snapshot)?,r.draft.slot,r.draft.start_time,r.draft.end_time,r.version_id,id])?;
            "按新规则更新，保留实例身份"
        } else {
            conn.execute(
                "UPDATE plans SET disposition='withdrawn',revision=revision+1 WHERE id=?1",
                [&id],
            )?;
            "停止正常安排，保留原行"
        };
        changes.push(PlanChange {
            date: old.scheduled_on,
            title: old.title,
            action: action.into(),
        });
    }
    Ok(RulePreview {
        fingerprint,
        changes,
        retained_overrides,
        preserved_facts,
    })
}

impl Store {
    pub fn preview_rule(&mut self, r: RuleRequest, today: &str) -> Result<RulePreview> {
        self.check_dataset(&r.dataset_id)?;
        let tx = self.conn.transaction()?;
        let result = apply_rule(&tx, &r, today)?;
        tx.rollback()?;
        Ok(result)
    }
    pub fn save_rule(&mut self, request: CommitRuleRequest, today: &str) -> Result<bool> {
        let r = request.request;
        self.check_dataset(&r.dataset_id)?;
        let tx = self.conn.transaction()?;
        if replay(&tx, &r)? {
            return Ok(false);
        }
        if request.fingerprint != rule_fingerprint(&tx, &r)? {
            return Err(conflict());
        }
        apply_rule(&tx, &r, today)?;
        tx.commit()?;
        Ok(true)
    }
    pub fn instances(&mut self, dataset: &str, id: &str, on: &str) -> Result<Vec<TaskItem>> {
        self.check_dataset(dataset)?;
        let tx = self.conn.transaction()?;
        let q = quests::quest(&tx, id)?;
        if q.mode != "recurring" {
            return Err(invalid("请选择重复任务。"));
        }
        ensure_range(&tx, on, on)?;
        let items = {
            let mut s=tx.prepare(&format!("{ITEM_QUERY} WHERE p.quest_id=?1 AND p.scheduled_on=?2 AND p.origin IN ('recurring','makeup') AND p.disposition<>'withdrawn' ORDER BY p.position,p.id"))?;
            let rows = s.query_map(params![id, on], item_row)?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        tx.commit()?;
        Ok(items)
    }
    pub(super) fn edit_instance(&mut self, r: EditRequest, today: &str) -> Result<MutationResult> {
        self.check_dataset(&r.dataset_id)?;
        validate_draft(&r.draft)?;
        let tx = self.conn.transaction()?;
        let old = query_item(&tx, &r.plan_id)?;
        let q = quests::quest(&tx, &old.quest_id)?;
        if old.task_mode != "recurring"
            || !["recurring", "makeup"].contains(&old.origin.as_str())
            || old.disposition != "active"
            || !old.can_execute
            || old.completed
            || old.has_records
            || old.scheduled_on.as_str() < today
        {
            return Err(invalid(
                "仅能修改今天或未来尚无执行记录的正常实例或补做安排。历史内容保持不变。",
            ));
        }
        if old.revision != r.expected_revision || q.revision != r.expected_quest_revision {
            return Err(conflict());
        }
        if r.draft.date != old.scheduled_on || r.draft.parent_id != q.parent_id {
            return Err(invalid(
                "仅本次修改不更改日期或父子结构；改日请明确生成补做。",
            ));
        }
        quests::validate_links(&tx, &q.id, q.parent_id.as_deref(), &r.draft.goal_ids)?;
        let goals = quests::snapshot_goals(&tx, &r.draft.goal_ids)?;
        let snapshot = ContentSnapshot {
            title: r.draft.title.trim().into(),
            description: r.draft.description,
            goals,
        };
        tx.execute("UPDATE plans SET content_snapshot=?1,slot=?2,start_time=?3,end_time=?4,is_override=1,revision=revision+1 WHERE id=?5",params![json(&snapshot)?,r.draft.slot,r.draft.start_time,r.draft.end_time,old.plan_id])?;
        let item = query_item(&tx, &old.plan_id)?;
        tx.commit()?;
        Ok(MutationResult {
            item,
            changed: true,
        })
    }
    pub fn reschedule_instance(
        &mut self,
        r: RescheduleRequest,
        today: &str,
        now: &str,
    ) -> Result<MutationResult> {
        self.check_dataset(&r.dataset_id)?;
        validate_id(&r.makeup_id)?;
        let tx = self.conn.transaction()?;
        let old = query_item(&tx, &r.plan_id)?;
        validate_draft(&TaskDraft {
            title: old.title.clone(),
            description: old.description.clone(),
            goal_ids: vec![],
            parent_id: None,
            date: r.date.clone(),
            slot: r.slot.clone(),
            start_time: r.start_time.clone(),
            end_time: r.end_time.clone(),
        })?;
        if let Some(next) = &old.next_plan_id {
            let existing = query_item(&tx, next)?;
            if existing.scheduled_on != r.date
                || existing.slot != r.slot
                || existing.start_time != r.start_time
                || existing.end_time != r.end_time
            {
                return Err(conflict());
            }
            return Ok(MutationResult {
                item: existing,
                changed: false,
            });
        }
        if r.date.as_str() < today || r.date == old.scheduled_on {
            return Err(invalid("补做日期请选择今天或未来，且与原安排不同。"));
        }
        if old.revision != r.expected_revision || old.quest_revision != r.expected_quest_revision {
            return Err(conflict());
        }
        if old.task_mode != "recurring"
            || !["recurring", "makeup"].contains(&old.origin.as_str())
            || old.disposition != "active"
            || old.completed
            || !old.can_execute
        {
            return Err(invalid("请选择未完成的正常重复实例或补做安排。"));
        }
        let (snapshot, version): (String, Option<String>) = tx.query_row(
            "SELECT content_snapshot,recurrence_version_id FROM plans WHERE id=?1",
            [&old.plan_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        tx.execute("INSERT INTO plans(id,quest_id,scheduled_on,slot,start_time,end_time,origin,recurrence_version_id,occurrence_on,content_snapshot,position,created_at) VALUES (?1,?2,?3,?4,?5,?6,'makeup',?7,?8,?9,(SELECT COALESCE(MAX(position),0)+1 FROM plans WHERE scheduled_on=?3),?10)",params![r.makeup_id,old.quest_id,r.date,r.slot,r.start_time,r.end_time,version,old.occurrence_on,snapshot,now])?;
        tx.execute("UPDATE plans SET disposition='rescheduled',next_plan_id=?1,revision=revision+1 WHERE id=?2",params![r.makeup_id,old.plan_id])?;
        let item = query_item(&tx, &r.makeup_id)?;
        tx.commit()?;
        Ok(MutationResult {
            item,
            changed: true,
        })
    }
}

#[cfg(test)]
mod tests;
