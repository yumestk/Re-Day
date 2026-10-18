PRAGMA foreign_keys = ON;
PRAGMA synchronous = FULL;
PRAGMA journal_mode = DELETE;

CREATE TABLE goals (
    id TEXT PRIMARY KEY, name TEXT NOT NULL CHECK(length(trim(name)) > 0),
    description TEXT NOT NULL DEFAULT '', kind TEXT NOT NULL CHECK(kind IN ('main','side')),
    status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','completed')),
    archived_at TEXT, revision INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE quests (
    id TEXT PRIMARY KEY, title TEXT NOT NULL CHECK(length(trim(title)) > 0),
    description TEXT NOT NULL DEFAULT '', parent_id TEXT REFERENCES quests(id) ON DELETE RESTRICT,
    mode TEXT NOT NULL DEFAULT 'single' CHECK(mode IN ('single','recurring')),
    status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','completed','cancelled')),
    archived_on TEXT, created_at TEXT NOT NULL, revision INTEGER NOT NULL DEFAULT 0,
    CHECK(parent_id IS NULL OR parent_id <> id)
);
CREATE TABLE quest_goals (
    quest_id TEXT NOT NULL REFERENCES quests(id) ON DELETE CASCADE,
    goal_id TEXT NOT NULL REFERENCES goals(id) ON DELETE RESTRICT,
    PRIMARY KEY(quest_id, goal_id)
);
CREATE TABLE recurrence_versions (
    id TEXT PRIMARY KEY, quest_id TEXT NOT NULL REFERENCES quests(id) ON DELETE RESTRICT,
    effective_from TEXT NOT NULL, effective_until TEXT,
    frequency TEXT NOT NULL CHECK(frequency IN ('daily','weekly','interval')),
    interval_days INTEGER, weekdays INTEGER, anchor_on TEXT NOT NULL, end_on TEXT,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1)),
    slot TEXT NOT NULL CHECK(slot IN ('morning','afternoon','evening')),
    start_time TEXT, end_time TEXT, content_snapshot TEXT NOT NULL CHECK(json_valid(content_snapshot)),
    CHECK(effective_until IS NULL OR effective_until >= effective_from),
    CHECK(interval_days IS NULL OR interval_days > 0),
    CHECK(weekdays IS NULL OR weekdays BETWEEN 1 AND 127)
);
CREATE TABLE plans (
    id TEXT PRIMARY KEY, quest_id TEXT NOT NULL REFERENCES quests(id) ON DELETE RESTRICT,
    scheduled_on TEXT NOT NULL,
    slot TEXT NOT NULL CHECK(slot IN ('morning','afternoon','evening')),
    start_time TEXT, end_time TEXT,
    origin TEXT NOT NULL DEFAULT 'manual' CHECK(origin IN ('manual','recurring','makeup')),
    recurrence_version_id TEXT REFERENCES recurrence_versions(id) ON DELETE RESTRICT,
    occurrence_on TEXT,
    disposition TEXT NOT NULL DEFAULT 'active' CHECK(disposition IN ('active','skipped','rescheduled','withdrawn')),
    next_plan_id TEXT REFERENCES plans(id) ON DELETE RESTRICT,
    content_snapshot TEXT NOT NULL CHECK(json_valid(content_snapshot)),
    is_override INTEGER NOT NULL DEFAULT 0 CHECK(is_override IN (0,1)),
    position INTEGER NOT NULL, created_at TEXT NOT NULL, revision INTEGER NOT NULL DEFAULT 0,
    UNIQUE(id, quest_id),
    CHECK(end_time IS NULL OR (start_time IS NOT NULL AND end_time > start_time)),
    CHECK(origin <> 'recurring' OR (recurrence_version_id IS NOT NULL AND occurrence_on IS NOT NULL)),
    CHECK(next_plan_id IS NULL OR next_plan_id <> id)
);
CREATE UNIQUE INDEX normal_occurrence ON plans(quest_id, occurrence_on) WHERE origin = 'recurring';
CREATE INDEX plans_by_day ON plans(scheduled_on, position);
CREATE TABLE execution_records (
    id TEXT PRIMARY KEY, quest_id TEXT NOT NULL REFERENCES quests(id) ON DELETE RESTRICT,
    plan_id TEXT, kind TEXT NOT NULL CHECK(kind IN ('progress','completion')),
    completion_scope TEXT, occurred_on TEXT NOT NULL, recorded_at TEXT NOT NULL,
    note TEXT, content_snapshot TEXT NOT NULL CHECK(json_valid(content_snapshot)), revoked_at TEXT,
    FOREIGN KEY(plan_id, quest_id) REFERENCES plans(id, quest_id) ON DELETE RESTRICT,
    CHECK((kind = 'progress' AND completion_scope IS NULL AND revoked_at IS NULL)
       OR (kind = 'completion' AND completion_scope IS NOT NULL AND completion_scope IN ('quest','instance')
           AND (completion_scope = 'quest' OR plan_id IS NOT NULL)))
);
CREATE UNIQUE INDEX active_quest_completion ON execution_records(quest_id)
    WHERE kind = 'completion' AND completion_scope = 'quest' AND revoked_at IS NULL;
CREATE UNIQUE INDEX active_instance_completion ON execution_records(plan_id)
    WHERE kind = 'completion' AND completion_scope = 'instance' AND revoked_at IS NULL;
CREATE INDEX executions_by_date ON execution_records(occurred_on);
CREATE TABLE life_events (
    id TEXT PRIMARY KEY, title TEXT NOT NULL CHECK(length(trim(title)) > 0), event_on TEXT NOT NULL,
    note TEXT NOT NULL DEFAULT '', tags TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(tags)),
    is_milestone INTEGER NOT NULL DEFAULT 0 CHECK(is_milestone IN (0,1)),
    goal_id TEXT REFERENCES goals(id) ON DELETE SET NULL,
    quest_id TEXT REFERENCES quests(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL, revision INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE assets (
    id TEXT PRIMARY KEY, relative_path TEXT NOT NULL UNIQUE,
    media_type TEXT NOT NULL, byte_size INTEGER NOT NULL CHECK(byte_size >= 0), sha256 TEXT NOT NULL,
    CHECK(relative_path NOT LIKE '/%' AND instr(relative_path, '..') = 0)
);
CREATE TABLE life_event_assets (
    life_event_id TEXT NOT NULL REFERENCES life_events(id) ON DELETE CASCADE,
    asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE RESTRICT,
    position INTEGER NOT NULL, PRIMARY KEY(life_event_id, asset_id)
);
CREATE TABLE daily_reviews (date TEXT PRIMARY KEY, reviewed_at TEXT, dismissed_on TEXT);
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL CHECK(json_valid(value)), revision INTEGER NOT NULL DEFAULT 0);
INSERT INTO settings(key, value) VALUES ('preferences', '{"soundEnabled":true,"reducedMotion":false}');
PRAGMA user_version = 1;
