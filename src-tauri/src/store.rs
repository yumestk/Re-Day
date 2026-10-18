mod quests;
mod recurrence;
use chrono::{NaiveDate, NaiveTime};
pub use quests::*;
pub use recurrence::*;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct AppError {
    pub code: &'static str,
    pub message: String,
}
impl AppError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for AppError {}
impl From<rusqlite::Error> for AppError {
    fn from(error: rusqlite::Error) -> Self {
        eprintln!("SQLite operation failed: {error}");
        if matches!(error, rusqlite::Error::QueryReturnedNoRows) {
            Self::new("NOT_FOUND", "这条安排已不存在，请刷新后重试。")
        } else {
            Self::new(
                "STORAGE",
                "本次操作未保存。请重试；若仍失败，请检查可用磁盘空间。",
            )
        }
    }
}
impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        eprintln!("Local storage failed: {error}");
        Self::new(
            "IO",
            "无法读取或写入本地数据；原文件已保留。请检查磁盘与目录权限。",
        )
    }
}
pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    pub sound_enabled: bool,
    pub reduced_motion: bool,
    #[serde(default)]
    pub revision: i64,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct ContentSnapshot {
    pub title: String,
    pub description: String,
    pub goals: Vec<serde_json::Value>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskItem {
    pub task_mode: String,
    pub origin: String,
    pub occurrence_on: Option<String>,
    pub disposition: String,
    pub next_plan_id: Option<String>,
    pub is_override: bool,
    pub has_records: bool,
    pub source_on: Option<String>,
    pub goals: Vec<serde_json::Value>,
    pub can_execute: bool,
    pub plan_id: String,
    pub quest_id: String,
    pub title: String,
    pub description: String,
    pub current_title: String,
    pub current_description: String,
    pub scheduled_on: String,
    pub slot: String,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub position: i64,
    pub completed: bool,
    pub completed_on: Option<String>,
    pub completion_id: Option<String>,
    pub revision: i64,
    pub quest_revision: i64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayView {
    pub records: Vec<Execution>,
    pub dataset_id: String,
    pub date: String,
    pub today: String,
    pub items: Vec<TaskItem>,
    pub preferences: Preferences,
}
#[derive(Serialize)]
pub struct MutationResult {
    pub item: TaskItem,
    pub changed: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDraft {
    pub goal_ids: Vec<String>,
    pub parent_id: Option<String>,
    pub title: String,
    pub description: String,
    pub date: String,
    pub slot: String,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRequest {
    pub dataset_id: String,
    pub quest_id: String,
    pub plan_id: String,
    #[serde(flatten)]
    pub draft: TaskDraft,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditRequest {
    pub dataset_id: String,
    pub plan_id: String,
    pub expected_revision: i64,
    pub expected_quest_revision: i64,
    #[serde(flatten)]
    pub draft: TaskDraft,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionRequest {
    pub dataset_id: String,
    pub plan_id: String,
    pub record_id: String,
    pub desired_completed: bool,
    pub completion_id: Option<String>,
    pub expected_revision: i64,
    pub expected_quest_revision: i64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesRequest {
    pub dataset_id: String,
    #[serde(flatten)]
    pub preferences: Preferences,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ActivePointer {
    dataset_id: Uuid,
}

pub struct Store {
    pub dataset_id: String,
    conn: Connection,
}

pub fn validate_date(date: &str) -> Result<()> {
    if NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .is_ok_and(|d| d.format("%Y-%m-%d").to_string() == date)
    {
        Ok(())
    } else {
        Err(AppError::new("VALIDATION", "请选择有效的日历日期。"))
    }
}
fn validate_draft(draft: &TaskDraft) -> Result<()> {
    if draft.title.trim().is_empty() {
        return Err(AppError::new("VALIDATION", "任务标题不能为空。"));
    }
    validate_date(&draft.date)?;
    if !["morning", "afternoon", "evening"].contains(&draft.slot.as_str()) {
        return Err(AppError::new("VALIDATION", "请选择上午、下午或晚上。"));
    }
    for time in [&draft.start_time, &draft.end_time].into_iter().flatten() {
        if !NaiveTime::parse_from_str(time, "%H:%M")
            .is_ok_and(|t| t.format("%H:%M").to_string() == *time)
        {
            return Err(AppError::new("VALIDATION", "请填写有效的具体时间。"));
        }
    }
    if draft.end_time.is_some()
        && (draft.start_time.is_none() || draft.end_time <= draft.start_time)
    {
        return Err(AppError::new(
            "VALIDATION",
            "结束时间须晚于开始时间，且在同一天内。",
        ));
    }
    Ok(())
}
fn validate_id(id: &str) -> Result<()> {
    Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| AppError::new("VALIDATION", "操作标识无效，请重新打开应用。"))
}
fn conflict() -> AppError {
    AppError::new(
        "CONFLICT",
        "内容已更新，请刷新后再试；这次修改没有覆盖新数据。",
    )
}
fn read_json<T: serde::de::DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_str(value).map_err(|_| {
        AppError::new(
            "CORRUPT_DATA",
            "本地数据格式不正确；原文件已保留，请勿覆盖。",
        )
    })
}
fn json<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value)
        .map_err(|_| AppError::new("INTERNAL", "无法保存本次内容，请重试。"))
}

const ITEM_QUERY: &str = "SELECT p.id, p.quest_id, p.content_snapshot, p.scheduled_on,
    p.slot, p.start_time, p.end_time, p.position,
    CASE WHEN p.origin='manual' THEN q.status='completed' ELSE e.id IS NOT NULL END,
    e.occurred_on, e.id, p.revision, q.revision, q.title, q.description,
    (p.disposition='active' AND q.status IN ('open','completed') AND
      ((q.mode='single' AND q.archived_on IS NULL) OR
       (q.mode='recurring' AND p.origin IN ('recurring','makeup') AND (q.archived_on IS NULL OR p.origin='makeup')))),
    q.mode,p.origin,p.occurrence_on,p.disposition,p.next_plan_id,p.is_override,
    EXISTS(SELECT 1 FROM execution_records facts WHERE facts.plan_id=p.id),
    (SELECT source.scheduled_on FROM plans source WHERE source.next_plan_id=p.id LIMIT 1)
    FROM plans p JOIN quests q ON q.id = p.quest_id
    LEFT JOIN execution_records e ON e.quest_id=q.id AND e.kind='completion' AND e.revoked_at IS NULL
      AND ((p.origin='manual' AND e.completion_scope='quest') OR
        (p.origin IN ('recurring','makeup') AND e.completion_scope='instance' AND e.plan_id=p.id))";
fn item_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskItem> {
    let raw: String = row.get(2)?;
    let snapshot: ContentSnapshot = serde_json::from_str(&raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(TaskItem {
        task_mode: row.get(16)?,
        origin: row.get(17)?,
        occurrence_on: row.get(18)?,
        disposition: row.get(19)?,
        next_plan_id: row.get(20)?,
        is_override: row.get(21)?,
        has_records: row.get(22)?,
        source_on: row.get(23)?,
        plan_id: row.get(0)?,
        quest_id: row.get(1)?,
        title: snapshot.title,
        goals: snapshot.goals,
        can_execute: row.get(15)?,
        description: snapshot.description,
        scheduled_on: row.get(3)?,
        slot: row.get(4)?,
        start_time: row.get(5)?,
        end_time: row.get(6)?,
        position: row.get(7)?,
        completed: row.get(8)?,
        completed_on: row.get(9)?,
        completion_id: row.get(10)?,
        revision: row.get(11)?,
        quest_revision: row.get(12)?,
        current_title: row.get(13)?,
        current_description: row.get(14)?,
    })
}
fn query_item(conn: &Connection, plan_id: &str) -> Result<TaskItem> {
    Ok(conn.query_row(
        &format!("{ITEM_QUERY} WHERE p.id = ?1"),
        [plan_id],
        item_row,
    )?)
}

impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        fs::create_dir_all(root)?;
        let pointer = root.join("active.json");
        let (id, fresh) = if pointer.exists() {
            let active: ActivePointer = read_json(&fs::read_to_string(&pointer)?)?;
            (active.dataset_id, false)
        } else {
            if root.join("datasets").exists()
                && fs::read_dir(root.join("datasets"))?.next().is_some()
            {
                return Err(AppError::new(
                    "CORRUPT_DATA",
                    "发现已有数据但缺少数据指针；文件已保留，未初始化空库。",
                ));
            }
            (Uuid::new_v4(), true)
        };
        let dataset = root.join("datasets").join(id.to_string());
        let database = dataset.join("lifeisrpg.sqlite3");
        let conn = if fresh {
            fs::create_dir_all(dataset.join("images"))?;
            let conn = Connection::open(&database)?;
            conn.execute_batch(include_str!("schema.sql"))?;
            let temporary = root.join("active.json.tmp");
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(json(&ActivePointer { dataset_id: id })?.as_bytes())?;
            file.sync_all()?;
            fs::rename(temporary, &pointer)?;
            fs::File::open(root)?.sync_all()?;
            conn
        } else {
            Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_WRITE)?
        };
        conn.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL; PRAGMA journal_mode=DELETE;",
        )?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != 1 {
            return Err(AppError::new(
                "UNSUPPORTED_VERSION",
                "数据版本与当前应用不符；原数据已保留。",
            ));
        }
        Ok(Self {
            dataset_id: id.to_string(),
            conn,
        })
    }
    fn check_dataset(&self, id: &str) -> Result<()> {
        if id == self.dataset_id {
            Ok(())
        } else {
            Err(conflict())
        }
    }
    pub fn preferences(&self) -> Result<Preferences> {
        let (value, revision): (String, i64) = self.conn.query_row(
            "SELECT value, revision FROM settings WHERE key='preferences'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let mut preferences: Preferences = read_json(&value)?;
        preferences.revision = revision;
        Ok(preferences)
    }
    pub fn load_day(&mut self, date: &str, today: &str) -> Result<DayView> {
        validate_date(date)?;
        let preferences = self.preferences()?;
        let dataset_id = self.dataset_id.clone();
        let tx = self.conn.transaction()?;
        recurrence::ensure_range(&tx, date, date)?;
        let items = {
            let mut statement = tx.prepare(&format!("{ITEM_QUERY} WHERE p.scheduled_on=?1
              AND p.disposition<>'withdrawn' AND (p.scheduled_on<?2 OR (q.archived_on IS NULL AND q.status<>'cancelled') OR p.origin='makeup') ORDER BY p.position,p.id"))?;
            let rows = statement.query_map([date, today], item_row)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let records = quests::execution_rows(&tx, "occurred_on", date)?;
        tx.commit()?;
        Ok(DayView {
            records,
            dataset_id,
            date: date.into(),
            today: today.into(),
            items,
            preferences,
        })
    }
    pub fn create(&mut self, request: CreateRequest, now: &str) -> Result<MutationResult> {
        self.check_dataset(&request.dataset_id)?;
        validate_id(&request.quest_id)?;
        validate_id(&request.plan_id)?;
        validate_draft(&request.draft)?;
        let tx = self.conn.transaction()?;
        quests::validate_links(
            &tx,
            &request.quest_id,
            request.draft.parent_id.as_deref(),
            &request.draft.goal_ids,
        )?;
        if let Some(existing) = tx
            .query_row(
                &format!("{ITEM_QUERY} WHERE p.id=?1"),
                [&request.plan_id],
                item_row,
            )
            .optional()?
        {
            let existing_quest = quests::quest(&tx, &existing.quest_id)?;
            if existing_quest.parent_id != request.draft.parent_id
                || existing_quest
                    .goal_ids
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    != request.draft.goal_ids.iter().collect()
                || existing.quest_id != request.quest_id
                || existing.current_title != request.draft.title.trim()
                || existing.current_description != request.draft.description
                || existing.scheduled_on != request.draft.date
                || existing.slot != request.draft.slot
                || existing.start_time != request.draft.start_time
                || existing.end_time != request.draft.end_time
            {
                return Err(conflict());
            }
            return Ok(MutationResult {
                item: existing,
                changed: false,
            });
        }
        let draft = request.draft;

        tx.execute(
            "INSERT INTO quests(id,title,description,created_at,parent_id) VALUES (?1,?2,?3,?4,?5)",
            params![
                request.quest_id,
                draft.title.trim(),
                draft.description,
                now,
                draft.parent_id
            ],
        )?;
        quests::replace_links(&tx, &request.quest_id, &draft.goal_ids)?;
        let snapshot = json(&quests::current_snapshot(&tx, &request.quest_id)?)?;
        tx.execute("INSERT INTO plans(id,quest_id,scheduled_on,slot,start_time,end_time,content_snapshot,position,created_at)
            VALUES (?1,?2,?3,?4,?5,?6,?7,(SELECT COALESCE(MAX(position),0)+1 FROM plans WHERE scheduled_on=?3),?8)",
            params![request.plan_id, request.quest_id, draft.date, draft.slot, draft.start_time, draft.end_time, snapshot, now])?;
        let item = query_item(&tx, &request.plan_id)?;
        tx.commit()?;
        Ok(MutationResult {
            item,
            changed: true,
        })
    }
    pub fn edit(&mut self, request: EditRequest, today: &str) -> Result<MutationResult> {
        if query_item(&self.conn, &request.plan_id)?.task_mode == "recurring" {
            return self.edit_instance(request, today);
        }
        self.check_dataset(&request.dataset_id)?;
        validate_draft(&request.draft)?;
        let tx = self.conn.transaction()?;
        let old = query_item(&tx, &request.plan_id)?;
        if old.revision != request.expected_revision
            || old.quest_revision != request.expected_quest_revision
        {
            return Err(conflict());
        }
        if old.scheduled_on != request.draft.date {
            return Err(AppError::new("VALIDATION", "修改的日期需与原安排一致。"));
        }
        let draft = request.draft;
        quests::validate_links(
            &tx,
            &old.quest_id,
            draft.parent_id.as_deref(),
            &draft.goal_ids,
        )?;
        if (old.completed || old.scheduled_on.as_str() < today)
            && (draft.slot != old.slot
                || draft.start_time != old.start_time
                || draft.end_time != old.end_time)
        {
            return Err(AppError::new(
                "VALIDATION",
                "已完成或过去的安排保留原时间，不能在此改写。",
            ));
        }
        tx.execute("UPDATE quests SET title=?1,description=?2,parent_id=?3,revision=revision+1 WHERE id=?4", params![draft.title.trim(), draft.description, draft.parent_id, old.quest_id])?;
        quests::replace_links(&tx, &old.quest_id, &draft.goal_ids)?;
        let snapshot = json(&quests::current_snapshot(&tx, &old.quest_id)?)?;
        if !old.completed {
            tx.execute(
                "UPDATE plans SET content_snapshot=?1,revision=revision+1 WHERE quest_id=?2
                AND scheduled_on>=?3 AND disposition='active' AND origin='manual' AND NOT EXISTS(SELECT 1 FROM execution_records e WHERE e.plan_id=plans.id)",
                params![snapshot, old.quest_id, today],
            )?;
            tx.execute("UPDATE plans SET slot=?1,start_time=?2,end_time=?3,revision=revision+1 WHERE id=?4 AND scheduled_on>=?5",
                params![draft.slot, draft.start_time, draft.end_time, old.plan_id, today])?;
        }
        let item = query_item(&tx, &old.plan_id)?;
        tx.commit()?;
        Ok(MutationResult {
            item,
            changed: true,
        })
    }
    pub fn set_completion(
        &mut self,
        request: CompletionRequest,
        today: &str,
        now: &str,
    ) -> Result<MutationResult> {
        self.check_dataset(&request.dataset_id)?;
        let item = query_item(&self.conn, &request.plan_id)?;
        let changed = self.record_execution(
            ExecutionRequest {
                dataset_id: request.dataset_id,
                quest_id: item.quest_id,
                plan_id: Some(request.plan_id.clone()),
                record_id: request.record_id,
                kind: if request.desired_completed {
                    "completion"
                } else {
                    "undo"
                }
                .into(),
                occurred_on: today.into(),
                note: String::new(),
                expected_revision: request.expected_quest_revision,
                expected_plan_revision: Some(request.expected_revision),
                completion_id: request.completion_id,
            },
            today,
            now,
        )?;
        Ok(MutationResult {
            item: query_item(&self.conn, &request.plan_id)?,
            changed,
        })
    }
    pub fn save_preferences(&mut self, request: PreferencesRequest) -> Result<Preferences> {
        self.check_dataset(&request.dataset_id)?;
        let current = self.preferences()?;
        let next = request.preferences;
        if current.sound_enabled == next.sound_enabled
            && current.reduced_motion == next.reduced_motion
        {
            return Ok(current);
        }
        if current.revision != next.revision {
            return Err(conflict());
        }
        let value = serde_json::json!({"soundEnabled": next.sound_enabled, "reducedMotion": next.reduced_motion});
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE settings SET value=?1,revision=revision+1 WHERE key='preferences'",
            [value.to_string()],
        )?;
        tx.commit()?;
        self.preferences()
    }
}

#[cfg(test)]
mod tests;
