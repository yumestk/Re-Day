use super::*;
fn strings(conn: &Connection, sql: &str) -> Result<Vec<String>> {
    let mut s = conn.prepare(sql)?;
    let rows = s.query_map([], |r| r.get::<_, Option<String>>(0))?;
    Ok(rows
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect())
}
fn reject_if(conn: &Connection, condition: &str) -> Result<()> {
    let n: i64 = conn.query_row(condition, [], |r| r.get(0))?;
    if n != 0 {
        return Err(invalid("备份违反业务约束，当前数据保持不变。"));
    }
    Ok(())
}
fn snapshot(raw: &str) -> Result<()> {
    let s: ContentSnapshot = read_json(raw)?;
    if s.title.trim().is_empty() {
        return Err(invalid("历史快照缺少标题。"));
    }
    let mut ids = BTreeSet::new();
    for goal in s.goals {
        let id = goal["id"]
            .as_str()
            .ok_or_else(|| invalid("快照目标标识缺失。"))?;
        canonical_id(id)?;
        if !ids.insert(id.to_string())
            || goal["name"].as_str().is_none_or(|v| v.trim().is_empty())
            || !matches!(goal["kind"].as_str(), Some("main" | "side"))
        {
            return Err(invalid("历史目标快照无效。"));
        }
    }
    Ok(())
}
type SchemaRow = (String, String, String, Option<String>);
fn schema(conn: &Connection) -> Result<Vec<SchemaRow>> {
    let mut s=conn.prepare("SELECT type,name,tbl_name,sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_autoindex_%' ORDER BY type,name")?;
    let rows = s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}
pub(super) fn validate_database(conn: &Connection) -> Result<()> {
    let pristine = Connection::open_in_memory()?;
    pristine.execute_batch(include_str!("../../schema.sql"))?;
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version != 1 || schema(conn)? != schema(&pristine)? {
        return Err(invalid("备份数据库结构不符，或含有未知表、触发器 / 视图。"));
    }
    if strings(conn, "PRAGMA integrity_check")? != vec!["ok"] {
        return Err(invalid("备份数据库完整性检查未通过。"));
    }
    reject_if(conn, "SELECT count(*) FROM pragma_foreign_key_check")?;
    for table in [
        "goals",
        "quests",
        "plans",
        "recurrence_versions",
        "execution_records",
        "life_events",
        "assets",
    ] {
        for id in strings(conn, &format!("SELECT id FROM {table}"))? {
            canonical_id(&id)?;
        }
        let n: i64 = conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
        if n > 1_000_000 {
            return Err(invalid("备份记录数量超出支持范围。"));
        }
    }
    for sql in [
        "SELECT scheduled_on FROM plans",
        "SELECT occurrence_on FROM plans",
        "SELECT effective_from FROM recurrence_versions",
        "SELECT effective_until FROM recurrence_versions",
        "SELECT anchor_on FROM recurrence_versions",
        "SELECT end_on FROM recurrence_versions",
        "SELECT occurred_on FROM execution_records",
        "SELECT event_on FROM life_events",
        "SELECT archived_on FROM quests",
        "SELECT date FROM daily_reviews",
        "SELECT dismissed_on FROM daily_reviews",
    ] {
        for on in strings(conn, sql)? {
            validate_date(&on)?;
        }
    }
    for sql in [
        "SELECT created_at FROM quests",
        "SELECT recorded_at FROM execution_records",
        "SELECT revoked_at FROM execution_records",
        "SELECT created_at FROM life_events",
        "SELECT created_at FROM plans",
        "SELECT archived_at FROM goals",
        "SELECT reviewed_at FROM daily_reviews",
    ] {
        for at in strings(conn, sql)? {
            if chrono::DateTime::parse_from_rfc3339(&at).is_err() {
                return Err(invalid("系统录入时间无效。"));
            }
        }
    }
    for table in ["goals", "quests", "plans", "life_events", "settings"] {
        reject_if(
            conn,
            &format!("SELECT count(*) FROM {table} WHERE revision<0"),
        )?;
    }
    for table in ["plans", "recurrence_versions", "execution_records"] {
        for raw in strings(conn, &format!("SELECT content_snapshot FROM {table}"))? {
            snapshot(&raw)?;
        }
    }
    for table in ["plans", "recurrence_versions"] {
        let mut s = conn.prepare(&format!("SELECT slot,start_time,end_time FROM {table}"))?;
        let rows = s.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?;
        for r in rows {
            let (slot, start_time, end_time) = r?;
            validate_draft(&TaskDraft {
                title: "检查".into(),
                description: "".into(),
                goal_ids: vec![],
                parent_id: None,
                date: "2026-01-01".into(),
                slot,
                start_time,
                end_time,
            })?;
        }
    }
    let quests = {
        let mut s = conn.prepare("SELECT id,parent_id FROM quests")?;
        let rows = s.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?;
        rows.collect::<rusqlite::Result<BTreeMap<_, _>>>()?
    };
    for id in quests.keys() {
        let mut seen = BTreeSet::new();
        let mut current = Some(id);
        while let Some(id) = current {
            if !seen.insert(id) {
                return Err(invalid("任务层级包含循环。"));
            }
            current = quests.get(id).and_then(|p| p.as_ref());
        }
    }
    let plans = {
        let mut s = conn.prepare("SELECT id,next_plan_id FROM plans")?;
        let rows = s.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?;
        rows.collect::<rusqlite::Result<BTreeMap<_, _>>>()?
    };
    for id in plans.keys() {
        let mut seen = BTreeSet::new();
        let mut current = Some(id);
        while let Some(id) = current {
            if !seen.insert(id) {
                return Err(invalid("计划后继包含循环。"));
            }
            current = plans.get(id).and_then(|p| p.as_ref());
        }
    }
    for sql in [
 "SELECT count(*) FROM quests q WHERE q.mode='recurring' AND (q.status='completed' OR EXISTS(SELECT 1 FROM quests c WHERE c.parent_id=q.id))",
 "SELECT count(*) FROM quests q WHERE (q.mode='single' AND ((q.status='completed') <> EXISTS(SELECT 1 FROM execution_records e WHERE e.quest_id=q.id AND e.kind='completion' AND e.completion_scope='quest' AND e.revoked_at IS NULL))) OR (q.mode='recurring' AND EXISTS(SELECT 1 FROM execution_records e WHERE e.quest_id=q.id AND e.kind='completion' AND e.completion_scope='quest' AND e.revoked_at IS NULL))",
 "SELECT count(*) FROM recurrence_versions v JOIN quests q ON v.quest_id=q.id WHERE q.mode<>'recurring' OR (v.end_on IS NOT NULL AND v.end_on<v.effective_from) OR (v.frequency='daily' AND (v.interval_days IS NOT NULL OR v.weekdays IS NOT NULL)) OR (v.frequency='weekly' AND (v.weekdays IS NULL OR v.interval_days IS NOT NULL)) OR (v.frequency='interval' AND (v.interval_days IS NULL OR v.weekdays IS NOT NULL))",
 "SELECT count(*) FROM recurrence_versions a JOIN recurrence_versions b ON a.quest_id=b.quest_id AND a.id<b.id WHERE (a.effective_until IS NULL OR a.effective_from<a.effective_until) AND (b.effective_until IS NULL OR b.effective_from<b.effective_until) AND (a.effective_until IS NULL OR b.effective_from<a.effective_until) AND (b.effective_until IS NULL OR a.effective_from<b.effective_until)",
 "SELECT count(*) FROM plans p JOIN quests q ON p.quest_id=q.id LEFT JOIN recurrence_versions v ON p.recurrence_version_id=v.id WHERE (p.origin<>'manual' AND (q.mode<>'recurring' OR p.recurrence_version_id IS NULL OR p.occurrence_on IS NULL OR v.quest_id<>p.quest_id)) OR (p.origin='manual' AND (p.recurrence_version_id IS NOT NULL OR p.occurrence_on IS NOT NULL)) OR (p.origin='recurring' AND p.occurrence_on<>p.scheduled_on)",
 "SELECT count(*) FROM plans p WHERE (p.disposition='rescheduled')<>(p.next_plan_id IS NOT NULL) OR EXISTS(SELECT 1 FROM plans n WHERE n.id=p.next_plan_id AND n.quest_id<>p.quest_id) OR (p.origin='makeup' AND NOT EXISTS(SELECT 1 FROM plans s WHERE s.next_plan_id=p.id AND s.origin IN ('recurring','makeup')))",
 "SELECT count(*) FROM execution_records e JOIN plans p ON e.plan_id=p.id WHERE (e.completion_scope='instance' AND p.origin='manual') OR (e.completion_scope='quest' AND p.origin<>'manual')",
 "SELECT count(*) FROM assets a WHERE NOT EXISTS(SELECT 1 FROM life_event_assets r WHERE r.asset_id=a.id)",
 "SELECT count(*) FROM daily_reviews WHERE dismissed_on IS NOT NULL AND date(dismissed_on)<>date(date,'+1 day')",
 "SELECT count(*) FROM settings WHERE key<>'preferences'"
 ]{reject_if(conn,sql)?;}
    let raw: String = conn.query_row(
        "SELECT value FROM settings WHERE key='preferences'",
        [],
        |r| r.get(0),
    )?;
    let _: Preferences = read_json(&raw)?;
    for id in strings(conn, "SELECT id FROM life_events")? {
        let d=conn.query_row("SELECT title,event_on,note,tags,is_milestone,goal_id,quest_id FROM life_events WHERE id=?1",[&id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,bool>(4)?,r.get::<_,Option<String>>(5)?,r.get::<_,Option<String>>(6)?)))?;
        super::super::events::validate_event(&EventDraft {
            title: d.0,
            event_on: d.1,
            note: d.2,
            tags: read_json(&d.3)?,
            is_milestone: d.4,
            goal_id: d.5,
            quest_id: d.6,
            image_ids: super::super::events::event_ids(conn, &id)?,
        })?;
    }
    Ok(())
}
