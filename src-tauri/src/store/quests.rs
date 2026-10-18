use super::*;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Goal {
    pub id: String,
    pub name: String,
    pub description: String,
    pub kind: String,
    pub status: String,
    pub archived: bool,
    pub revision: i64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveGoalRequest {
    pub dataset_id: String,
    pub goal: Goal,
    pub expected_revision: Option<i64>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Quest {
    pub recurrence: Vec<RuleVersion>,
    pub id: String,
    pub title: String,
    pub description: String,
    pub parent_id: Option<String>,
    pub mode: String,
    pub status: String,
    pub archived_on: Option<String>,
    pub revision: i64,
    pub goal_ids: Vec<String>,
    pub completion_id: Option<String>,
    pub completed_on: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub dataset_id: String,
    pub today: String,
    pub goals: Vec<Goal>,
    pub quests: Vec<Quest>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestDraft {
    pub title: String,
    pub description: String,
    pub parent_id: Option<String>,
    pub goal_ids: Vec<String>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveQuestRequest {
    pub dataset_id: String,
    pub quest_id: String,
    pub expected_revision: Option<i64>,
    #[serde(flatten)]
    pub draft: QuestDraft,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestStateRequest {
    pub dataset_id: String,
    pub quest_id: String,
    pub expected_revision: i64,
    pub action: String,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionRequest {
    pub dataset_id: String,
    pub quest_id: String,
    pub plan_id: Option<String>,
    pub record_id: String,
    pub kind: String,
    pub occurred_on: String,
    pub note: String,
    pub expected_revision: i64,
    pub expected_plan_revision: Option<i64>,
    pub completion_id: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Execution {
    pub id: String,
    pub quest_id: String,
    pub plan_id: Option<String>,
    pub kind: String,
    pub occurred_on: String,
    pub recorded_at: String,
    pub note: Option<String>,
    pub snapshot: ContentSnapshot,
    pub revoked_at: Option<String>,
    pub completion_scope: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestHistory {
    pub records: Vec<Execution>,
    pub plans: Vec<HistoricalPlan>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoricalPlan {
    pub id: String,
    pub date: String,
    pub slot: String,
    pub title: String,
    pub disposition: String,
    pub origin: String,
    pub occurrence_on: Option<String>,
    pub next_plan_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleRequest {
    pub dataset_id: String,
    pub quest_id: String,
    pub plan_id: String,
    pub expected_revision: i64,
    #[serde(flatten)]
    pub draft: TaskDraft,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChildSummary {
    pub id: String,
    pub title: String,
    pub revision: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeletePreview {
    pub quest_id: String,
    pub title: String,
    pub plans: i64,
    pub progress: i64,
    pub completions: i64,
    pub children: Vec<ChildSummary>,
    pub life_events: i64,
    pub has_history: bool,
    pub fingerprint: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteRequest {
    pub dataset_id: String,
    pub preview: DeletePreview,
    pub confirmed_history: bool,
}

fn validation(message: &str) -> AppError {
    AppError::new("VALIDATION", message)
}
fn goal_rows(conn: &Connection) -> Result<Vec<Goal>> {
    let mut stmt = conn.prepare("SELECT id,name,description,kind,status,archived_at IS NOT NULL,revision FROM goals ORDER BY kind,id")?;
    let rows = stmt.query_map([], |r| {
        Ok(Goal {
            id: r.get(0)?,
            name: r.get(1)?,
            description: r.get(2)?,
            kind: r.get(3)?,
            status: r.get(4)?,
            archived: r.get(5)?,
            revision: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}
const QUEST_QUERY: &str = "SELECT q.id,q.title,q.description,q.parent_id,q.mode,q.status,q.archived_on,q.revision,e.id,e.occurred_on FROM quests q LEFT JOIN execution_records e ON e.quest_id=q.id AND e.kind='completion' AND e.completion_scope='quest' AND e.revoked_at IS NULL";
fn quest_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Quest> {
    Ok(Quest {
        id: r.get(0)?,
        title: r.get(1)?,
        description: r.get(2)?,
        parent_id: r.get(3)?,
        mode: r.get(4)?,
        status: r.get(5)?,
        archived_on: r.get(6)?,
        revision: r.get(7)?,
        completion_id: r.get(8)?,
        completed_on: r.get(9)?,
        goal_ids: vec![],
        recurrence: vec![],
    })
}
pub(super) fn quest(conn: &Connection, id: &str) -> Result<Quest> {
    let mut q = conn.query_row(&format!("{QUEST_QUERY} WHERE q.id=?1"), [id], quest_row)?;
    let mut stmt =
        conn.prepare("SELECT goal_id FROM quest_goals WHERE quest_id=?1 ORDER BY goal_id")?;
    q.goal_ids = stmt
        .query_map([id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    q.recurrence = recurrence::rule_versions(conn, id)?;
    Ok(q)
}
pub(super) fn snapshot_goals(conn: &Connection, ids: &[String]) -> Result<Vec<serde_json::Value>> {
    let mut ids = ids.to_vec();
    ids.sort();
    ids.iter().map(|id|Ok(conn.query_row("SELECT id,name,kind FROM goals WHERE id=?1",[id],|r|Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"kind":r.get::<_,String>(2)?})))?)).collect()
}
pub(super) fn current_snapshot(conn: &Connection, id: &str) -> Result<ContentSnapshot> {
    let q = quest(conn, id)?;
    let mut stmt=conn.prepare("SELECT g.id,g.name,g.kind FROM goals g JOIN quest_goals l ON l.goal_id=g.id WHERE l.quest_id=?1 ORDER BY g.id")?;
    let goals=stmt.query_map([id],|r|Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"kind":r.get::<_,String>(2)?})))?.collect::<rusqlite::Result<_>>()?;
    Ok(ContentSnapshot {
        title: q.title,
        description: q.description,
        goals,
    })
}
pub(super) fn validate_links(
    conn: &Connection,
    id: &str,
    parent: Option<&str>,
    goals: &[String],
) -> Result<()> {
    let unique: std::collections::HashSet<_> = goals.iter().collect();
    if unique.len() != goals.len() {
        return Err(validation("目标关联不能重复。"));
    }
    for goal in goals {
        validate_id(goal)?;
        conn.query_row("SELECT id FROM goals WHERE id=?1", [goal], |r| {
            r.get::<_, String>(0)
        })?;
    }
    if let Some(parent) = parent {
        validate_id(parent)?;
        if parent == id {
            return Err(validation("任务不能成为自己的父任务。"));
        }
        let p = quest(conn, parent)?;
        if p.mode != "single" {
            return Err(validation("重复任务不能添加子任务。"));
        }
        let cycle:bool=conn.query_row("WITH RECURSIVE ancestors(id,parent_id) AS (SELECT id,parent_id FROM quests WHERE id=?1 UNION SELECT q.id,q.parent_id FROM quests q JOIN ancestors a ON q.id=a.parent_id) SELECT EXISTS(SELECT 1 FROM ancestors WHERE id=?2)",params![parent,id],|r|r.get(0))?;
        if cycle {
            return Err(validation("父子任务不能形成循环。请选择其他父任务。"));
        }
    }
    Ok(())
}
pub(super) fn replace_links(conn: &Connection, id: &str, goals: &[String]) -> Result<()> {
    conn.execute("DELETE FROM quest_goals WHERE quest_id=?1", [id])?;
    for goal in goals {
        conn.execute(
            "INSERT INTO quest_goals(quest_id,goal_id) VALUES (?1,?2)",
            params![id, goal],
        )?;
    }
    Ok(())
}
fn refresh_open_plans(conn: &Connection, id: &str, today: &str) -> Result<()> {
    let snapshot = json(&current_snapshot(conn, id)?)?;
    conn.execute("UPDATE plans SET content_snapshot=?1,revision=revision+1 WHERE quest_id=?2 AND scheduled_on>=?3 AND disposition='active' AND origin='manual' AND NOT EXISTS(SELECT 1 FROM execution_records e WHERE e.plan_id=plans.id) AND EXISTS(SELECT 1 FROM quests q WHERE q.id=?2 AND q.status='open')",params![snapshot,id,today])?;
    Ok(())
}
pub(super) fn execution_rows(
    conn: &Connection,
    column: &str,
    value: &str,
) -> Result<Vec<Execution>> {
    // column is a fixed internal selector, never supplied by the frontend.
    let mut stmt=conn.prepare(&format!("SELECT id,quest_id,plan_id,kind,occurred_on,recorded_at,note,content_snapshot,revoked_at,completion_scope FROM execution_records WHERE {column}=?1 ORDER BY occurred_on DESC,recorded_at DESC,id"))?;
    let raw = stmt.query_map([value], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, String>(7)?,
            r.get::<_, Option<String>>(8)?,
            r.get::<_, Option<String>>(9)?,
        ))
    })?;
    raw.map(|row| {
        let (
            id,
            quest_id,
            plan_id,
            kind,
            occurred_on,
            recorded_at,
            note,
            snapshot,
            revoked_at,
            completion_scope,
        ) = row?;
        Ok(Execution {
            id,
            quest_id,
            plan_id,
            kind,
            occurred_on,
            recorded_at,
            note,
            snapshot: read_json(&snapshot)?,
            revoked_at,
            completion_scope,
        })
    })
    .collect()
}
fn preview(conn: &Connection, id: &str) -> Result<DeletePreview> {
    let q = quest(conn, id)?;
    let mut stmt =
        conn.prepare("SELECT id,title,revision FROM quests WHERE parent_id=?1 ORDER BY id")?;
    let children = stmt
        .query_map([id], |r| {
            Ok(ChildSummary {
                id: r.get(0)?,
                title: r.get(1)?,
                revision: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let plans: i64 = conn.query_row("SELECT count(*) FROM plans WHERE quest_id=?1", [id], |r| {
        r.get(0)
    })?;
    let progress: i64 = conn.query_row(
        "SELECT count(*) FROM execution_records WHERE quest_id=?1 AND kind='progress'",
        [id],
        |r| r.get(0),
    )?;
    let completions: i64 = conn.query_row(
        "SELECT count(*) FROM execution_records WHERE quest_id=?1 AND kind='completion'",
        [id],
        |r| r.get(0),
    )?;
    let life_events: i64 = conn.query_row(
        "SELECT count(*) FROM life_events WHERE quest_id=?1",
        [id],
        |r| r.get(0),
    )?;
    let stamps = |sql: &str| -> Result<Vec<(String, i64)>> {
        let mut s = conn.prepare(sql)?;
        let rows = s.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    };
    let fingerprint = json(&(
        q.revision,
        &children,
        stamps("SELECT id,revision FROM plans WHERE quest_id=?1 ORDER BY id")?,
        stamps(
            "SELECT id,revoked_at IS NOT NULL FROM execution_records WHERE quest_id=?1 ORDER BY id",
        )?,
        stamps("SELECT id,revision FROM life_events WHERE quest_id=?1 ORDER BY id")?,
    ))?;
    Ok(DeletePreview {
        quest_id: id.into(),
        title: q.title,
        plans,
        progress,
        completions,
        children,
        life_events,
        has_history: plans + progress + completions > 0,
        fingerprint,
    })
}

impl Store {
    pub fn workspace(&self, today: &str) -> Result<Workspace> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM quests ORDER BY created_at,id")?;
        let ids = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let quests = ids
            .iter()
            .map(|id| quest(&self.conn, id))
            .collect::<Result<_>>()?;
        Ok(Workspace {
            dataset_id: self.dataset_id.clone(),
            today: today.into(),
            goals: goal_rows(&self.conn)?,
            quests,
        })
    }
    pub fn save_goal(&mut self, request: SaveGoalRequest, today: &str, now: &str) -> Result<Goal> {
        self.check_dataset(&request.dataset_id)?;
        validate_id(&request.goal.id)?;
        let g = request.goal;
        if g.name.trim().is_empty()
            || !["main", "side"].contains(&g.kind.as_str())
            || !["open", "completed"].contains(&g.status.as_str())
        {
            return Err(validation("目标需要名称、主线/支线类别与有效状态。"));
        }
        let tx = self.conn.transaction()?;
        let old = goal_rows(&tx)?.into_iter().find(|old| old.id == g.id);
        match (old, request.expected_revision) {
            (None, None) => {
                tx.execute("INSERT INTO goals(id,name,description,kind,status,archived_at) VALUES (?1,?2,?3,?4,?5,?6)",params![g.id,g.name.trim(),g.description,g.kind,g.status,if g.archived {Some(now)}else{None}])?;
            }
            (Some(old), Some(expected)) if old.revision == expected => {
                tx.execute("UPDATE goals SET name=?1,description=?2,kind=?3,status=?4,archived_at=?5,revision=revision+1 WHERE id=?6",params![g.name.trim(),g.description,g.kind,g.status,if g.archived {Some(now)}else{None},g.id])?;
            }
            (Some(old), None)
                if old.name == g.name.trim()
                    && old.description == g.description
                    && old.kind == g.kind
                    && old.status == g.status
                    && old.archived == g.archived =>
            {
                return Ok(old)
            }
            _ => return Err(conflict()),
        }
        let ids = {
            let mut s = tx.prepare("SELECT quest_id FROM quest_goals WHERE goal_id=?1")?;
            let rows = s.query_map([&g.id], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        for id in ids {
            tx.execute("UPDATE quests SET revision=revision+1 WHERE id=?1", [&id])?;
            refresh_open_plans(&tx, &id, today)?;
        }
        let result = goal_rows(&tx)?
            .into_iter()
            .find(|x| x.id == g.id)
            .ok_or_else(conflict)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn save_quest(
        &mut self,
        request: SaveQuestRequest,
        today: &str,
        now: &str,
    ) -> Result<Quest> {
        self.check_dataset(&request.dataset_id)?;
        validate_id(&request.quest_id)?;
        if request.draft.title.trim().is_empty() {
            return Err(validation("任务标题不能为空。"));
        }
        let tx = self.conn.transaction()?;
        let id = &request.quest_id;
        let d = request.draft;
        validate_links(&tx, id, d.parent_id.as_deref(), &d.goal_ids)?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM quests WHERE id=?1)",
            [id],
            |r| r.get(0),
        )?;
        if exists {
            let old = quest(&tx, id)?;
            if request.expected_revision.is_none()
                && old.title == d.title.trim()
                && old.description == d.description
                && old.parent_id == d.parent_id
                && old
                    .goal_ids
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    == d.goal_ids.iter().collect()
            {
                return Ok(old);
            }
            if request.expected_revision != Some(old.revision) {
                return Err(conflict());
            }
            if old.mode != "single" {
                return Err(validation("重复任务的编辑在重复规则阶段提供。"));
            }
            tx.execute("UPDATE quests SET title=?1,description=?2,parent_id=?3,revision=revision+1 WHERE id=?4",params![d.title.trim(),d.description,d.parent_id,id])?;
        } else {
            if request.expected_revision.is_some() {
                return Err(conflict());
            }
            tx.execute("INSERT INTO quests(id,title,description,parent_id,created_at) VALUES (?1,?2,?3,?4,?5)",params![id,d.title.trim(),d.description,d.parent_id,now])?;
        }
        replace_links(&tx, id, &d.goal_ids)?;
        refresh_open_plans(&tx, id, today)?;
        let result = quest(&tx, id)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn quest_state(&mut self, request: QuestStateRequest, today: &str) -> Result<Quest> {
        self.check_dataset(&request.dataset_id)?;
        let tx = self.conn.transaction()?;
        let q = quest(&tx, &request.quest_id)?;
        if q.revision != request.expected_revision {
            return Err(conflict());
        }
        if q.mode == "recurring" && ["archive", "cancel"].contains(&request.action.as_str()) {
            recurrence::stop_future(&tx, &q.id, today)?;
        }
        match request.action.as_str() {
            "archive" => {
                tx.execute(
                    "UPDATE quests SET archived_on=?1,revision=revision+1 WHERE id=?2",
                    params![today, q.id],
                )?;
            }
            "unarchive" => {
                tx.execute(
                    "UPDATE quests SET archived_on=NULL,revision=revision+1 WHERE id=?1",
                    [&q.id],
                )?;
            }
            "cancel" if q.status == "open" => {
                tx.execute(
                    "UPDATE quests SET status='cancelled',revision=revision+1 WHERE id=?1",
                    [&q.id],
                )?;
            }
            "reopen" if q.status == "cancelled" => {
                tx.execute(
                    "UPDATE quests SET status='open',revision=revision+1 WHERE id=?1",
                    [&q.id],
                )?;
            }
            _ => {
                return Err(validation(
                    "当前状态不能执行这项操作；完成状态请使用明确的完成或撤销操作。",
                ))
            }
        }
        tx.execute(
            "UPDATE plans SET revision=revision+1 WHERE quest_id=?1",
            [&q.id],
        )?;
        let result = quest(&tx, &q.id)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn record_execution(
        &mut self,
        request: ExecutionRequest,
        today: &str,
        now: &str,
    ) -> Result<bool> {
        self.check_dataset(&request.dataset_id)?;
        validate_id(&request.record_id)?;
        validate_date(&request.occurred_on)?;
        if request.occurred_on.as_str() > today {
            return Err(validation("执行事实只能记录今天或过去的日期。"));
        }
        if !["progress", "completion", "undo"].contains(&request.kind.as_str()) {
            return Err(validation("请选择进展、完成或撤销。"));
        }
        let tx = self.conn.transaction()?;
        let q = quest(&tx, &request.quest_id)?;
        let item = request
            .plan_id
            .as_deref()
            .map(|id| query_item(&tx, id))
            .transpose()?;
        if item.as_ref().is_some_and(|p| p.quest_id != q.id) {
            return Err(conflict());
        }
        let recurring = q.mode == "recurring";
        if recurring
            && item
                .as_ref()
                .is_none_or(|p| !["recurring", "makeup"].contains(&p.origin.as_str()))
        {
            return Err(validation(
                "重复任务必须选择已有的正常实例或补做安排，不能新增无实例执行。",
            ));
        }
        let scope = if recurring { "instance" } else { "quest" };
        let completion_id = if recurring {
            item.as_ref().and_then(|p| p.completion_id.clone())
        } else {
            q.completion_id.clone()
        };
        let old_records = execution_rows(&tx, "quest_id", &q.id)?;
        if request.kind != "undo" {
            if let Some(old) = old_records.iter().find(|r| r.id == request.record_id) {
                if old.plan_id != request.plan_id
                    || old.kind != request.kind
                    || old.occurred_on != request.occurred_on
                    || old.note.as_deref() != Some(request.note.as_str())
                    || (request.kind == "completion"
                        && old.completion_scope.as_deref() != Some(scope))
                {
                    return Err(conflict());
                }
                return Ok(false);
            }
            if request.kind == "completion" && completion_id.is_some() {
                return Ok(false);
            }
        } else {
            let id = request
                .completion_id
                .as_ref()
                .ok_or_else(|| validation("请选择要撤销的完成记录。"))?;
            let old = old_records
                .iter()
                .find(|r| {
                    &r.id == id
                        && r.kind == "completion"
                        && r.completion_scope.as_deref() == Some(scope)
                })
                .ok_or_else(conflict)?;
            if recurring && old.plan_id != request.plan_id {
                return Err(conflict());
            }
            if old.revoked_at.is_some() {
                return Ok(false);
            }
            if completion_id.as_ref() != Some(id) {
                return Err(conflict());
            }
        }
        if q.revision != request.expected_revision {
            return Err(conflict());
        }
        if let Some(p) = &item {
            if request.expected_plan_revision != Some(p.revision) {
                return Err(conflict());
            }
            if p.disposition != "active" {
                return Err(validation("这条安排已结束处置，不能直接执行或撤销。"));
            }
        }
        if request.kind != "undo"
            && (q.status != "open"
                || (q.archived_on.is_some()
                    && !(recurring && item.as_ref().is_some_and(|p| p.origin == "makeup")))
                || completion_id.is_some())
        {
            return Err(validation(
                "请选择可执行的未完成任务或实例。归档重复任务只保留已有补做的执行入口。",
            ));
        }
        let snapshot = if let Some(p) = &item {
            tx.query_row(
                "SELECT content_snapshot FROM plans WHERE id=?1",
                [&p.plan_id],
                |r| r.get::<_, String>(0),
            )?
        } else {
            json(&current_snapshot(&tx, &q.id)?)?
        };
        if request.kind == "undo" {
            tx.execute(
                "UPDATE execution_records SET revoked_at=?1 WHERE id=?2",
                params![now, request.completion_id],
            )?;
        } else {
            tx.execute("INSERT INTO execution_records(id,quest_id,plan_id,kind,completion_scope,occurred_on,recorded_at,note,content_snapshot) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![request.record_id,q.id,request.plan_id,request.kind,if request.kind=="completion"{Some(scope)}else{None},request.occurred_on,now,request.note,snapshot])?;
        }
        if recurring {
            tx.execute(
                "UPDATE plans SET revision=revision+1 WHERE id=?1",
                [&request.plan_id],
            )?;
        } else {
            tx.execute(
                "UPDATE quests SET status=?1,revision=revision+1 WHERE id=?2",
                params![
                    if request.kind == "completion" {
                        "completed"
                    } else {
                        "open"
                    },
                    q.id
                ],
            )?;
            tx.execute(
                "UPDATE plans SET revision=revision+1 WHERE quest_id=?1",
                [&q.id],
            )?;
        }
        tx.commit()?;
        Ok(true)
    }
    pub fn history(&self, id: &str) -> Result<QuestHistory> {
        quest(&self.conn, id)?;
        let mut stmt=self.conn.prepare("SELECT id,scheduled_on,slot,content_snapshot,disposition,origin,occurrence_on,next_plan_id FROM plans WHERE quest_id=?1 ORDER BY scheduled_on DESC,position")?;
        let raw = stmt.query_map([id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<String>>(7)?,
            ))
        })?;
        let plans = raw
            .map(|row| {
                let (id, date, slot, snapshot, disposition, origin, occurrence_on, next_plan_id) =
                    row?;
                Ok(HistoricalPlan {
                    id,
                    date,
                    slot,
                    title: read_json::<ContentSnapshot>(&snapshot)?.title,
                    disposition,
                    origin,
                    occurrence_on,
                    next_plan_id,
                })
            })
            .collect::<Result<_>>()?;
        Ok(QuestHistory {
            records: execution_rows(&self.conn, "quest_id", id)?,
            plans,
        })
    }
    pub fn schedule(
        &mut self,
        request: ScheduleRequest,
        _today: &str,
        now: &str,
    ) -> Result<TaskItem> {
        self.check_dataset(&request.dataset_id)?;
        validate_id(&request.plan_id)?;
        validate_draft(&request.draft)?;
        let tx = self.conn.transaction()?;
        let q = quest(&tx, &request.quest_id)?;
        if let Some(item) = tx
            .query_row(
                &format!("{ITEM_QUERY} WHERE p.id=?1"),
                [&request.plan_id],
                item_row,
            )
            .optional()?
        {
            if item.quest_id != q.id
                || item.scheduled_on != request.draft.date
                || item.slot != request.draft.slot
                || item.start_time != request.draft.start_time
                || item.end_time != request.draft.end_time
            {
                return Err(conflict());
            }
            return Ok(item);
        }
        if q.revision != request.expected_revision {
            return Err(conflict());
        }
        if q.mode != "single" || q.status != "open" || q.archived_on.is_some() {
            return Err(validation("仅活跃的未完成普通任务可添加计划。"));
        }
        let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM plans WHERE quest_id=?1 AND scheduled_on=?2 AND disposition='active')",params![q.id,request.draft.date],|r|r.get(0))?;
        if exists {
            return Err(validation(
                "该任务在这一天已有安排，请在 Today 查看或调整原安排。",
            ));
        }
        let snapshot = json(&current_snapshot(&tx, &q.id)?)?;
        let d = request.draft;
        tx.execute("INSERT INTO plans(id,quest_id,scheduled_on,slot,start_time,end_time,content_snapshot,position,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,(SELECT COALESCE(MAX(position),0)+1 FROM plans WHERE scheduled_on=?3),?8)",params![request.plan_id,q.id,d.date,d.slot,d.start_time,d.end_time,snapshot,now])?;
        tx.execute("UPDATE quests SET revision=revision+1 WHERE id=?1", [&q.id])?;
        let result = query_item(&tx, &request.plan_id)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn preview_delete(&self, dataset: &str, id: &str) -> Result<DeletePreview> {
        self.check_dataset(dataset)?;
        preview(&self.conn, id)
    }
    pub fn delete_quest(&mut self, request: DeleteRequest) -> Result<bool> {
        self.check_dataset(&request.dataset_id)?;
        let tx = self.conn.transaction()?;
        let current = preview(&tx, &request.preview.quest_id)?;
        if current != request.preview {
            return Err(conflict());
        }
        if current.has_history && !request.confirmed_history {
            return Err(validation("删除历史前需要查看影响范围并二次确认。"));
        }
        let id = &current.quest_id;
        tx.execute(
            "UPDATE quests SET parent_id=NULL,revision=revision+1 WHERE parent_id=?1",
            [id],
        )?;
        tx.execute(
            "UPDATE life_events SET quest_id=NULL,revision=revision+1 WHERE quest_id=?1",
            [id],
        )?;
        tx.execute("UPDATE plans SET next_plan_id=NULL WHERE quest_id=?1 OR next_plan_id IN (SELECT id FROM plans WHERE quest_id=?1)",[id])?;
        tx.execute("DELETE FROM execution_records WHERE quest_id=?1", [id])?;
        tx.execute("DELETE FROM plans WHERE quest_id=?1", [id])?;
        tx.execute("DELETE FROM recurrence_versions WHERE quest_id=?1", [id])?;
        tx.execute("DELETE FROM quests WHERE id=?1", [id])?;
        tx.commit()?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
