use super::*;
use image::{ImageFormat, ImageReader};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::{Cursor, Read},
    path::PathBuf,
};

pub(super) const MAX_IMAGE: u64 = 50 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Asset {
    pub id: String,
    pub relative_path: String,
    pub media_type: String,
    pub byte_size: u64,
    pub sha256: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageView {
    pub id: String,
    pub url: String,
    pub byte_size: u64,
    pub staged: bool,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LifeEvent {
    pub id: String,
    pub title: String,
    pub event_on: String,
    pub note: String,
    pub tags: Vec<String>,
    pub is_milestone: bool,
    pub goal_id: Option<String>,
    pub quest_id: Option<String>,
    pub created_at: String,
    pub revision: i64,
    pub images: Vec<ImageView>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventDraft {
    pub title: String,
    pub event_on: String,
    pub note: String,
    pub tags: Vec<String>,
    pub is_milestone: bool,
    pub goal_id: Option<String>,
    pub quest_id: Option<String>,
    pub image_ids: Vec<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveEvent {
    pub dataset_id: String,
    pub event_id: String,
    pub expected_revision: Option<i64>,
    pub draft: EventDraft,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteEvent {
    pub dataset_id: String,
    pub event_id: String,
    pub expected_revision: i64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventResult {
    pub event: Option<LifeEvent>,
    pub cleanup_pending: bool,
}
fn invalid(s: &str) -> AppError {
    AppError::new("VALIDATION", s)
}
pub(super) fn canonical_id(id: &str) -> Result<()> {
    validate_id(id)?;
    if Uuid::parse_str(id).unwrap().to_string() != id {
        return Err(invalid("资源标识无效。"));
    }
    Ok(())
}
pub(super) fn image_url(path: &Path) -> Result<String> {
    let mut url = tauri::Url::parse("asset://localhost/").unwrap();
    url.set_path(&format!(
        "/{}",
        path.to_str().ok_or_else(|| invalid("图片路径无法编码。"))?
    ));
    Ok(url.to_string())
}
pub(super) fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(invalid("请选择普通文件。"));
    }
    let f = fs::File::open(path)?;
    if f.metadata()?.len() > limit {
        return Err(invalid("文件超出支持的大小。"));
    }
    let mut bytes = vec![];
    f.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(invalid("文件超出支持的大小。"));
    }
    Ok(bytes)
}
pub(super) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(super) fn image_info(bytes: &[u8]) -> Result<(&'static str, &'static str)> {
    if bytes.len() as u64 > MAX_IMAGE {
        return Err(invalid("单张图片最多支持 50 MiB。"));
    }
    let format = image::guess_format(bytes)
        .map_err(|_| invalid("图片格式无效；支持 PNG、JPEG 和 WebP。"))?;
    let info = match format {
        ImageFormat::Png => ("png", "image/png"),
        ImageFormat::Jpeg => ("jpg", "image/jpeg"),
        ImageFormat::WebP => ("webp", "image/webp"),
        _ => return Err(invalid("图片格式暂不支持；请选择 PNG、JPEG 或 WebP。")),
    };
    let (w, h) = ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|_| invalid("图片无法解码。"))?;
    if w == 0 || h == 0 || w as u64 * h as u64 > 40_000_000 {
        return Err(invalid("图片最多支持 4000 万像素。"));
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().map_err(|_| invalid("图片无法完整解码。"))?;
    Ok(info)
}
pub(super) fn assets(conn: &Connection) -> Result<Vec<Asset>> {
    let mut s = conn
        .prepare("SELECT id,relative_path,media_type,byte_size,sha256 FROM assets ORDER BY id")?;
    let rows = s.query_map([], |r| {
        Ok(Asset {
            id: r.get(0)?,
            relative_path: r.get(1)?,
            media_type: r.get(2)?,
            byte_size: r.get::<_, i64>(3)? as u64,
            sha256: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}
pub(super) fn validate_asset(asset: &Asset) -> Result<()> {
    canonical_id(&asset.id)?;
    let ext = match asset.media_type.as_str() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        _ => return Err(invalid("图片类型无效。")),
    };
    if asset.relative_path != format!("images/{}.{}", asset.id, ext)
        || asset.byte_size == 0
        || asset.byte_size > MAX_IMAGE
        || asset.sha256.len() != 64
        || !asset
            .sha256
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
    {
        return Err(invalid("图片资源描述无效。"));
    }
    Ok(())
}
pub(super) fn check_asset(root: &Path, asset: &Asset) -> Result<()> {
    validate_asset(asset)?;
    let bytes = read_limited(&root.join(&asset.relative_path), MAX_IMAGE)?;
    let (_, mime) = image_info(&bytes)?;
    if bytes.len() as u64 != asset.byte_size
        || digest(&bytes) != asset.sha256
        || mime != asset.media_type
    {
        return Err(invalid("图片与登记的资源不一致。"));
    }
    Ok(())
}
pub(super) fn validate_event(d: &EventDraft) -> Result<()> {
    validate_date(&d.event_on)?;
    if d.title.trim().is_empty()
        || d.title.chars().count() > 200
        || d.note.chars().count() > 4000
        || d.tags.len() > 30
        || d.tags
            .iter()
            .any(|t| t.trim().is_empty() || t.chars().count() > 40)
        || d.tags.iter().collect::<BTreeSet<_>>().len() != d.tags.len()
        || d.image_ids.len() > 30
        || d.image_ids.iter().collect::<BTreeSet<_>>().len() != d.image_ids.len()
    {
        return Err(invalid(
            "请填写有效标题、简短备注、标签和图片（单条最多 30 张）。",
        ));
    }
    for id in d
        .image_ids
        .iter()
        .chain(d.goal_id.iter())
        .chain(d.quest_id.iter())
    {
        canonical_id(id)?;
    }
    Ok(())
}
pub(super) fn event_ids(conn: &Connection, id: &str) -> Result<Vec<String>> {
    let mut s = conn.prepare(
        "SELECT asset_id FROM life_event_assets WHERE life_event_id=?1 ORDER BY position,asset_id",
    )?;
    let rows = s.query_map([id], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}
impl Store {
    pub fn dataset_dir(&self) -> PathBuf {
        self.root.join("datasets").join(&self.dataset_id)
    }
    pub fn image_scopes(&self) -> Vec<PathBuf> {
        vec![
            self.dataset_dir().join("images"),
            self.root
                .join("staging/imports")
                .join(&self.dataset_id)
                .join("images"),
        ]
    }
    fn import_meta(&self, id: &str) -> Result<PathBuf> {
        canonical_id(id)?;
        Ok(self
            .root
            .join("staging/imports")
            .join(&self.dataset_id)
            .join("metadata")
            .join(format!("{id}.json")))
    }
    fn staged_asset(&self, id: &str) -> Result<Asset> {
        let a: Asset = read_json(&fs::read_to_string(self.import_meta(id)?)?)?;
        validate_asset(&a)?;
        if a.id != id {
            return Err(invalid("暂存图片标识不符。"));
        }
        Ok(a)
    }
    pub fn import_images(&self, dataset: &str, paths: &[PathBuf]) -> Result<Vec<ImageView>> {
        self.check_dataset(dataset)?;
        if paths.len() > 30 {
            return Err(invalid("一次最多选择 30 张图片。"));
        }
        let available = assets(&self.conn)?;
        let mut result = vec![];
        let mut added = vec![];
        let run = (|| {
            for path in paths {
                let bytes = read_limited(path, MAX_IMAGE)?;
                let (ext, mime) = image_info(&bytes)?;
                let hash = digest(&bytes);
                if let Some(a) = available.iter().find(|a| a.sha256 == hash) {
                    check_asset(&self.dataset_dir(), a)?;
                    result.push(ImageView {
                        id: a.id.clone(),
                        url: image_url(&self.dataset_dir().join(&a.relative_path))?,
                        byte_size: a.byte_size,
                        staged: false,
                    });
                    continue;
                }
                let id = Uuid::new_v4().to_string();
                let a = Asset {
                    id: id.clone(),
                    relative_path: format!("images/{id}.{ext}"),
                    media_type: mime.into(),
                    byte_size: bytes.len() as u64,
                    sha256: hash,
                };
                let meta = self.import_meta(&id)?;
                fs::create_dir_all(meta.parent().unwrap())?;
                let dest = self.image_scopes()[1].join(format!("{id}.{ext}"));
                fs::create_dir_all(dest.parent().unwrap())?;
                added.push(id.clone());
                let mut f = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&dest)?;
                f.write_all(&bytes)?;
                f.sync_all()?;
                fs::write(&meta, json(&a)?)?;
                result.push(ImageView {
                    id,
                    url: image_url(&dest)?,
                    byte_size: a.byte_size,
                    staged: true,
                });
            }
            Ok(())
        })();
        if let Err(e) = run {
            let _ = self.discard_images(dataset, &added);
            return Err(e);
        }
        Ok(result)
    }
    pub fn discard_images(&self, dataset: &str, ids: &[String]) -> Result<()> {
        self.check_dataset(dataset)?;
        for id in ids {
            let meta = self.import_meta(id)?;
            if meta.exists() {
                let a = self.staged_asset(id)?;
                let name = Path::new(&a.relative_path).file_name().unwrap();
                let path = self.image_scopes()[1].join(name);
                if path.exists() {
                    fs::remove_file(path)?;
                }
                fs::remove_file(meta)?;
            }
        }
        Ok(())
    }
    pub fn list_events(&self) -> Result<Vec<LifeEvent>> {
        let ids = {
            let mut s = self
                .conn
                .prepare("SELECT id FROM life_events ORDER BY event_on DESC,created_at DESC,id")?;
            let rows = s.query_map([], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        ids.iter().map(|id| self.event(id)).collect()
    }
    pub fn event(&self, id: &str) -> Result<LifeEvent> {
        let mut e=self.conn.query_row("SELECT id,title,event_on,note,tags,is_milestone,goal_id,quest_id,created_at,revision FROM life_events WHERE id=?1",[id],|r|Ok((LifeEvent{id:r.get(0)?,title:r.get(1)?,event_on:r.get(2)?,note:r.get(3)?,tags:vec![],is_milestone:r.get(5)?,goal_id:r.get(6)?,quest_id:r.get(7)?,created_at:r.get(8)?,revision:r.get(9)?,images:vec![]},r.get::<_,String>(4)?)))?;
        e.0.tags = read_json(&e.1)?;
        let all = assets(&self.conn)?;
        for id in event_ids(&self.conn, id)? {
            let a = all
                .iter()
                .find(|a| a.id == id)
                .ok_or_else(|| invalid("图片资源缺失。"))?;
            validate_asset(a)?;
            e.0.images.push(ImageView {
                id: a.id.clone(),
                url: image_url(&self.dataset_dir().join(&a.relative_path))?,
                byte_size: a.byte_size,
                staged: false,
            });
        }
        Ok(e.0)
    }
    pub fn save_event(&mut self, r: SaveEvent, now: &str) -> Result<EventResult> {
        self.check_dataset(&r.dataset_id)?;
        canonical_id(&r.event_id)?;
        validate_event(&r.draft)?;
        let old = self
            .conn
            .query_row(
                "SELECT revision FROM life_events WHERE id=?1",
                [&r.event_id],
                |r| r.get::<_, i64>(0),
            )
            .optional()?;
        if old.is_some() {
            let e = self.event(&r.event_id)?;
            let same = EventDraft {
                title: e.title,
                event_on: e.event_on,
                note: e.note,
                tags: e.tags,
                is_milestone: e.is_milestone,
                goal_id: e.goal_id,
                quest_id: e.quest_id,
                image_ids: e.images.into_iter().map(|a| a.id).collect(),
            };
            let mut normalized = r.draft.clone();
            normalized.title = normalized.title.trim().into();
            if same == normalized {
                return Ok(EventResult {
                    event: Some(self.event(&r.event_id)?),
                    cleanup_pending: false,
                });
            }
        }
        if old != r.expected_revision {
            return Err(conflict());
        }
        let available = assets(&self.conn)?;
        let mut new = vec![];
        for id in &r.draft.image_ids {
            if let Some(a) = available.iter().find(|a| &a.id == id) {
                check_asset(&self.dataset_dir(), a)?;
            } else {
                let a = self.staged_asset(id)?;
                let temp_root = self.image_scopes()[1].parent().unwrap().to_path_buf();
                check_asset(&temp_root, &a)?;
                new.push(a);
            }
        }
        let mut created = vec![];
        let dir = self.dataset_dir();
        let write = (|| {
            for a in &new {
                let dest = dir.join(&a.relative_path);
                if dest.exists() {
                    return Err(invalid("发现同名未登记图片，请重启后重试；原文件已保留。"));
                }
                let source =
                    self.image_scopes()[1].join(Path::new(&a.relative_path).file_name().unwrap());
                created.push(dest.clone());
                fs::copy(source, &dest)?;
                fs::File::open(dest)?.sync_all()?;
            }
            fs::File::open(dir.join("images"))?.sync_all()?;
            let tx = self.conn.transaction()?;
            let d = &r.draft;
            if old.is_some() {
                tx.execute("UPDATE life_events SET title=?1,event_on=?2,note=?3,tags=?4,is_milestone=?5,goal_id=?6,quest_id=?7,revision=revision+1 WHERE id=?8",params![d.title.trim(),d.event_on,d.note,json(&d.tags)?,d.is_milestone,d.goal_id,d.quest_id,r.event_id])?;
            } else {
                tx.execute("INSERT INTO life_events(id,title,event_on,note,tags,is_milestone,goal_id,quest_id,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![r.event_id,d.title.trim(),d.event_on,d.note,json(&d.tags)?,d.is_milestone,d.goal_id,d.quest_id,now])?;
            }
            for a in &new {
                tx.execute("INSERT INTO assets(id,relative_path,media_type,byte_size,sha256) VALUES (?1,?2,?3,?4,?5)",params![a.id,a.relative_path,a.media_type,a.byte_size as i64,a.sha256])?;
            }
            tx.execute(
                "DELETE FROM life_event_assets WHERE life_event_id=?1",
                [&r.event_id],
            )?;
            for (i, id) in d.image_ids.iter().enumerate() {
                tx.execute("INSERT INTO life_event_assets(life_event_id,asset_id,position) VALUES (?1,?2,?3)",params![r.event_id,id,i as i64])?;
            }
            tx.commit()?;
            Ok(())
        })();
        if let Err(e) = write {
            for path in created {
                let _ = fs::remove_file(path);
            }
            return Err(e);
        }
        let pending = self.cleanup_images().is_err();
        Ok(EventResult {
            event: Some(self.event(&r.event_id)?),
            cleanup_pending: pending,
        })
    }
    pub fn delete_event(&mut self, r: DeleteEvent) -> Result<EventResult> {
        self.check_dataset(&r.dataset_id)?;
        let tx = self.conn.transaction()?;
        let revision: Option<i64> = tx
            .query_row(
                "SELECT revision FROM life_events WHERE id=?1",
                [&r.event_id],
                |r| r.get(0),
            )
            .optional()?;
        if revision.is_some_and(|v| v != r.expected_revision) {
            return Err(conflict());
        }
        tx.execute("DELETE FROM life_events WHERE id=?1", [&r.event_id])?;
        tx.commit()?;
        let pending = self.cleanup_images().is_err();
        Ok(EventResult {
            event: None,
            cleanup_pending: pending,
        })
    }
    pub fn cleanup_images(&mut self) -> Result<()> {
        let dir = self.dataset_dir();
        let unused = {
            let mut s=self.conn.prepare("SELECT a.id FROM assets a WHERE NOT EXISTS(SELECT 1 FROM life_event_assets r WHERE r.asset_id=a.id)")?;
            let rows = s.query_map([], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        for a in assets(&self.conn)?
            .iter()
            .filter(|a| unused.contains(&a.id))
        {
            validate_asset(a)?;
            let p = dir.join(&a.relative_path);
            if p.exists() {
                fs::remove_file(&p)?;
            }
            self.conn.execute("DELETE FROM assets WHERE id=?1 AND NOT EXISTS(SELECT 1 FROM life_event_assets WHERE asset_id=?1)",[&a.id])?;
        }
        let known: BTreeSet<_> = assets(&self.conn)?
            .into_iter()
            .map(|a| a.relative_path)
            .collect();
        for entry in fs::read_dir(dir.join("images"))? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some((id, ext)) = name.rsplit_once('.') {
                if canonical_id(id).is_ok()
                    && ["png", "jpg", "webp"].contains(&ext)
                    && !known.contains(&format!("images/{name}"))
                {
                    fs::remove_file(entry.path())?;
                }
            }
        }
        Ok(())
    }
}
