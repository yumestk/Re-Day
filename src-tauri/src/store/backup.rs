use super::events::{assets, canonical_id, check_asset, MAX_IMAGE};
use super::*;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Seek},
    path::PathBuf,
    time::Duration,
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};
mod validate;
use validate::validate_database;
const MAX_TOTAL: u64 = 16 * 1024 * 1024 * 1024;
const MAX_DATABASE: u64 = 512 * 1024 * 1024;
const MAX_FILES: usize = 10_001;
const MAX_MANIFEST: u64 = 4 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DataCounts {
    pub goals: i64,
    pub quests: i64,
    pub plans: i64,
    pub records: i64,
    pub events: i64,
    pub images: i64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BackupFile {
    path: String,
    bytes: u64,
    sha256: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    format_version: u32,
    schema_version: u32,
    app_version: String,
    exported_at: String,
    files: Vec<BackupFile>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestorePreview {
    pub token: String,
    pub dataset_id: String,
    pub backup_digest: String,
    pub exported_at: String,
    pub current: DataCounts,
    pub incoming: DataCounts,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Receipt {
    preview: RestorePreview,
    source: PathBuf,
    candidate_id: String,
    current_hash: String,
    manifest: Manifest,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreRequest {
    pub dataset_id: String,
    pub token: String,
    pub backup_digest: String,
    pub confirmed: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    pub dataset_id: String,
    pub cleanup_pending: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub file_name: String,
    pub counts: DataCounts,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageInfo {
    pub counts: DataCounts,
    pub cleanup_pending: bool,
}
fn invalid(message: &str) -> AppError {
    AppError::new("INVALID_BACKUP", message)
}
fn zip_error(e: impl std::fmt::Display) -> AppError {
    eprintln!("Backup ZIP: {e}");
    invalid("备份 ZIP 无法读取或写入；当前数据未被替换。")
}
fn hash_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    if file.metadata()?.len() > MAX_TOTAL {
        return Err(invalid("文件体积超过支持范围。"));
    }
    let mut total = 0u64;
    let mut hasher = Sha256::new();
    let mut bytes = [0; 65536];
    loop {
        let n = file.read(&mut bytes)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > MAX_TOTAL {
            return Err(invalid("文件在读取期间超出大小限制。"));
        }
        hasher.update(&bytes[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
fn counts(conn: &Connection) -> Result<DataCounts> {
    let count = |table: &str| -> Result<i64> {
        Ok(conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?)
    };
    Ok(DataCounts {
        goals: count("goals")?,
        quests: count("quests")?,
        plans: count("plans")?,
        records: count("execution_records")?,
        events: count("life_events")?,
        images: count("assets")?,
    })
}
fn backup_conn(path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA foreign_keys=ON; PRAGMA query_only=ON;")?;
    Ok(conn)
}
fn check_space(path: &Path, required: u64) -> Result<()> {
    if fs2::available_space(path)? < required.saturating_add(64 * 1024 * 1024) {
        return Err(invalid("磁盘可用空间不足，请释放空间后重试。"));
    }
    Ok(())
}
fn valid_backup_path(path: &str) -> bool {
    if path == "lifeisrpg.sqlite3" {
        return true;
    }
    if let Some(file) = path.strip_prefix("images/") {
        if let Some((id, ext)) = file.rsplit_once('.') {
            return canonical_id(id).is_ok() && ["png", "jpg", "webp"].contains(&ext);
        }
    }
    false
}
fn manifest_valid(m: &Manifest) -> Result<u64> {
    if m.format_version != 1 || m.schema_version != 1 {
        return Err(AppError::new(
            "UNSUPPORTED_VERSION",
            "不支持此备份版本；当前数据保持不变。",
        ));
    }
    if m.app_version != env!("CARGO_PKG_VERSION")
        || chrono::DateTime::parse_from_rfc3339(&m.exported_at).is_err()
    {
        return Err(invalid("备份版本或导出时间无效。"));
    }
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    if m.files.is_empty() || m.files.len() > MAX_FILES {
        return Err(invalid("备份文件数量超出支持范围。"));
    }
    for f in &m.files {
        let limit = if f.path == "lifeisrpg.sqlite3" {
            MAX_DATABASE
        } else {
            MAX_IMAGE
        };
        if !valid_backup_path(&f.path)
            || !names.insert(&f.path)
            || f.bytes == 0
            || f.bytes > limit
            || f.sha256.len() != 64
            || !f
                .sha256
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err(invalid("备份清单包含无效路径、大小、摘要或重复项。"));
        }
        total = total
            .checked_add(f.bytes)
            .ok_or_else(|| invalid("备份体积无效。"))?;
    }
    if !names.contains(&"lifeisrpg.sqlite3".to_string()) || total > MAX_TOTAL {
        return Err(invalid("备份缺少数据库或解压体积超过 16 GiB。"));
    }
    Ok(total)
}
fn unpack<R: Read + Seek>(reader: R, destination: &Path) -> Result<Manifest> {
    let mut zip = ZipArchive::new(reader).map_err(zip_error)?;
    if zip.len() > MAX_FILES + 1 {
        return Err(invalid("备份文件数量过多。"));
    }
    let mut entries = BTreeSet::new();
    for i in 0..zip.len() {
        let f = zip.by_index(i).map_err(zip_error)?;
        let mode = f.unix_mode().unwrap_or(0) & 0o170000;
        if f.is_dir()
            || f.is_symlink()
            || f.encrypted()
            || (mode != 0 && mode != 0o100000)
            || !entries.insert(f.name().to_string())
            || (f.name() != "manifest.json" && !valid_backup_path(f.name()))
        {
            return Err(invalid("备份包含非法路径、链接、重复项或不支持的文件。"));
        }
    }
    let manifest = {
        let f = zip.by_name("manifest.json").map_err(zip_error)?;
        if f.size() > MAX_MANIFEST {
            return Err(invalid("备份清单过大。"));
        }
        let mut bytes = vec![];
        f.take(MAX_MANIFEST + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_MANIFEST {
            return Err(invalid("备份清单过大。"));
        }
        serde_json::from_slice::<Manifest>(&bytes).map_err(|_| invalid("备份清单无法解析。"))?
    };
    let total = manifest_valid(&manifest)?;
    let expected: BTreeSet<_> = manifest
        .files
        .iter()
        .map(|f| f.path.clone())
        .chain(["manifest.json".into()])
        .collect();
    if expected != entries {
        return Err(invalid("ZIP 内容与备份清单不一致。"));
    }
    check_space(destination.parent().unwrap(), total)?;
    fs::create_dir_all(destination.join("images"))?;
    for file in &manifest.files {
        let mut entry = zip.by_name(&file.path).map_err(zip_error)?;
        if entry.size() != file.bytes {
            return Err(invalid("备份文件大小不符。"));
        }
        let path = destination.join(&file.path);
        let mut out = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let mut hash = Sha256::new();
        let mut remaining = file.bytes;
        let mut bytes = [0; 65536];
        loop {
            let n = entry.read(&mut bytes[..(remaining + 1).min(65536) as usize])?;
            if n == 0 {
                break;
            }
            if n as u64 > remaining {
                return Err(invalid("实际解压大小超过清单。"));
            }
            remaining -= n as u64;
            hash.update(&bytes[..n]);
            out.write_all(&bytes[..n])?;
        }
        if remaining != 0 || format!("{:x}", hash.finalize()) != file.sha256 {
            return Err(invalid("备份文件摘要不符或内容不完整。"));
        }
        out.sync_all()?;
    }
    fs::File::open(destination.join("images"))?.sync_all()?;
    fs::File::open(destination)?.sync_all()?;
    fs::File::open(destination.parent().unwrap())?.sync_all()?;
    Ok(manifest)
}
fn validate_dataset(path: &Path) -> Result<DataCounts> {
    let conn = backup_conn(&path.join("lifeisrpg.sqlite3"))?;
    validate_database(&conn)?;
    let all = assets(&conn)?;
    for a in &all {
        check_asset(path, a)?;
    }
    let expected: BTreeSet<_> = all
        .iter()
        .map(|a| {
            Path::new(&a.relative_path)
                .file_name()
                .unwrap()
                .to_os_string()
        })
        .collect();
    let mut actual = BTreeSet::new();
    for e in fs::read_dir(path.join("images"))? {
        let e = e?;
        if !e.file_type()?.is_file() {
            return Err(invalid("图片目录包含非法条目。"));
        }
        actual.insert(e.file_name());
    }
    if expected != actual {
        return Err(invalid("图片文件与数据库引用不一致。"));
    }
    counts(&conn)
}
fn switch_pointer(root: &Path, id: &str) -> Result<()> {
    canonical_id(id)?;
    let tmp = root.join(format!("active-{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        f.write_all(
            json(&ActivePointer {
                dataset_id: Uuid::parse_str(id).unwrap(),
            })?
            .as_bytes(),
        )?;
        f.sync_all()?;
        fs::rename(&tmp, root.join("active.json"))?;
        fs::File::open(root)?.sync_all()?;
        Ok(())
    })();
    if tmp.exists() {
        let _ = fs::remove_file(tmp);
    }
    result
}
#[cfg(test)]
thread_local! {static FAIL_AT:std::cell::Cell<Option<&'static str>>=const {std::cell::Cell::new(None)};}
fn checkpoint(step: &'static str) -> Result<()> {
    #[cfg(test)]
    if FAIL_AT.get() == Some(step) || (FAIL_AT.get() == Some("rollback") && step == "activate") {
        return Err(invalid("injected restore failure"));
    }
    let _ = step;
    Ok(())
}
impl Store {
    pub fn storage_info(&mut self) -> Result<StorageInfo> {
        let cleanup_pending = self.cleanup_images().is_err();
        Ok(StorageInfo {
            counts: counts(&self.conn)?,
            cleanup_pending,
        })
    }
    pub fn cleanup_obsolete(&self) -> Result<()> {
        let mut prepared = BTreeSet::new();
        let receipts = self.root.join("staging/restores");
        if receipts.is_dir() {
            for e in fs::read_dir(receipts)? {
                let e = e?;
                if let Ok(bytes) = fs::read(e.path()) {
                    if let Ok(receipt) = serde_json::from_slice::<Receipt>(&bytes) {
                        if receipt.preview.dataset_id == self.dataset_id {
                            prepared.insert(receipt.candidate_id);
                        }
                    }
                }
            }
        }
        let datasets = self.root.join("datasets");
        if !datasets.is_dir() {
            return Ok(());
        }
        for e in fs::read_dir(&datasets)? {
            let e = e?;
            let id = e.file_name().to_string_lossy().into_owned();
            if id != self.dataset_id
                && !prepared.contains(&id)
                && canonical_id(&id).is_ok()
                && e.file_type()?.is_dir()
            {
                fs::remove_dir_all(e.path())?;
            }
        }
        let imports = self.root.join("staging/imports");
        if imports.is_dir() {
            for e in fs::read_dir(imports)? {
                let e = e?;
                if e.file_name() != self.dataset_id.as_str() && e.file_type()?.is_dir() {
                    fs::remove_dir_all(e.path())?;
                }
            }
        }
        Ok(())
    }
    pub fn export_backup(
        &self,
        dataset: &str,
        destination: &Path,
        now: &str,
    ) -> Result<ExportResult> {
        self.check_dataset(dataset)?;
        let parent = destination
            .parent()
            .ok_or_else(|| invalid("导出位置无效。"))?
            .canonicalize()?;
        if parent.starts_with(self.root.canonicalize()?) {
            return Err(invalid("请选择应用数据目录之外的位置。"));
        }
        if destination.exists() && !fs::symlink_metadata(destination)?.file_type().is_file() {
            return Err(invalid("请选择普通 ZIP 文件位置。"));
        }
        let stage = self
            .root
            .join("staging/exports")
            .join(Uuid::new_v4().to_string());
        fs::create_dir_all(&stage)?;
        let temp = parent.join(format!(".lifeisrpg-{}.tmp", Uuid::new_v4()));
        let run = (|| {
            let snapshot = stage.join("lifeisrpg.sqlite3");
            let mut conn = Connection::open(&snapshot)?;
            rusqlite::backup::Backup::new(&self.conn, &mut conn)?.run_to_completion(
                256,
                Duration::from_millis(5),
                None,
            )?;
            conn.execute("DELETE FROM assets WHERE NOT EXISTS(SELECT 1 FROM life_event_assets r WHERE r.asset_id=assets.id)",[])?;
            validate_database(&conn)?;
            let info = counts(&conn)?;
            drop(conn);
            fs::File::open(&snapshot)?.sync_all()?;
            let all = assets(&backup_conn(&snapshot)?)?;
            let mut files = vec![BackupFile {
                path: "lifeisrpg.sqlite3".into(),
                bytes: fs::metadata(&snapshot)?.len(),
                sha256: hash_file(&snapshot)?,
            }];
            let mut sources = BTreeMap::from([("lifeisrpg.sqlite3".to_string(), snapshot)]);
            for a in all {
                check_asset(&self.dataset_dir(), &a)?;
                files.push(BackupFile {
                    path: a.relative_path.clone(),
                    bytes: a.byte_size,
                    sha256: a.sha256,
                });
                sources.insert(
                    a.relative_path.clone(),
                    self.dataset_dir().join(&a.relative_path),
                );
            }
            let manifest = Manifest {
                format_version: 1,
                schema_version: 1,
                app_version: env!("CARGO_PKG_VERSION").into(),
                exported_at: now.into(),
                files,
            };
            let total = manifest_valid(&manifest)?;
            check_space(&parent, total)?;
            let output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            let mut zip = ZipWriter::new(output);
            zip.start_file(
                "manifest.json",
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
            )
            .map_err(zip_error)?;
            zip.write_all(json(&manifest)?.as_bytes())?;
            for file in &manifest.files {
                zip.start_file(
                    &file.path,
                    SimpleFileOptions::default().compression_method(
                        if file.path == "lifeisrpg.sqlite3" {
                            CompressionMethod::Deflated
                        } else {
                            CompressionMethod::Stored
                        },
                    ),
                )
                .map_err(zip_error)?;
                let mut input = fs::File::open(&sources[&file.path])?;
                let mut hasher = Sha256::new();
                let mut written = 0u64;
                let mut bytes = [0; 65536];
                loop {
                    let n = input.read(&mut bytes)?;
                    if n == 0 {
                        break;
                    }
                    written += n as u64;
                    if written > file.bytes {
                        return Err(invalid("导出期间资源大小发生变化，请重试。"));
                    }
                    hasher.update(&bytes[..n]);
                    zip.write_all(&bytes[..n])?;
                }
                if written != file.bytes || format!("{:x}", hasher.finalize()) != file.sha256 {
                    return Err(invalid("导出期间资源内容发生变化，请重试。"));
                }
            }
            let file = zip.finish().map_err(zip_error)?;
            if file.metadata()?.len() > MAX_TOTAL {
                return Err(invalid("备份文件超过 16 GiB，请减少图片体积后重试。"));
            }
            file.sync_all()?;
            fs::rename(&temp, destination)?;
            fs::File::open(parent.clone())?.sync_all()?;
            Ok(ExportResult {
                file_name: destination
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                counts: info,
            })
        })();
        let _ = fs::remove_dir_all(stage);
        if temp.exists() {
            let _ = fs::remove_file(temp);
        }
        run
    }
    pub fn inspect_backup(&self, dataset: &str, source: &Path) -> Result<RestorePreview> {
        self.check_dataset(dataset)?;
        if !fs::symlink_metadata(source)?.file_type().is_file()
            || fs::metadata(source)?.len() > MAX_TOTAL
        {
            return Err(invalid("请选择有效 ZIP 备份（最大 16 GiB）。"));
        }
        let token = Uuid::new_v4().to_string();
        let id = Uuid::new_v4().to_string();
        let dest = self.root.join("datasets").join(&id);
        let receipt_dir = self.root.join("staging/restores");
        fs::create_dir_all(&receipt_dir)?;
        let source = source.canonicalize()?;
        let source_sha = hash_file(&source)?;
        fs::create_dir(&dest)?;
        let result = (|| {
            let manifest = unpack(fs::File::open(&source)?, &dest)?;
            let incoming = validate_dataset(&dest)?;
            if hash_file(&source)? != source_sha {
                return Err(invalid("备份文件在读取期间发生变化，请重新选择。"));
            }
            checkpoint("prepared")?;
            let preview = RestorePreview {
                token: token.clone(),
                dataset_id: dataset.into(),
                backup_digest: source_sha,
                exported_at: manifest.exported_at.clone(),
                current: counts(&self.conn)?,
                incoming,
            };
            let receipt = Receipt {
                preview: preview.clone(),
                source,
                candidate_id: id,
                manifest,
                current_hash: hash_file(&self.dataset_dir().join("lifeisrpg.sqlite3"))?,
            };
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(receipt_dir.join(format!("{token}.json")))?;
            f.write_all(json(&receipt)?.as_bytes())?;
            f.sync_all()?;
            Ok(preview)
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(dest);
        }
        result
    }
    fn receipt(&self, token: &str) -> Result<Receipt> {
        canonical_id(token)?;
        let r: Receipt = read_json(&fs::read_to_string(
            self.root
                .join("staging/restores")
                .join(format!("{token}.json")),
        )?)?;
        canonical_id(&r.candidate_id)?;
        if r.preview.token != token {
            return Err(invalid("恢复预览无效。"));
        }
        Ok(r)
    }
    pub fn discard_backup(&self, dataset: &str, token: &str) -> Result<()> {
        self.check_dataset(dataset)?;
        let r = self.receipt(token)?;
        if r.candidate_id != self.dataset_id {
            let path = self.root.join("datasets").join(r.candidate_id);
            if path.exists() {
                fs::remove_dir_all(path)?;
            }
        }
        let _ = fs::remove_file(
            self.root
                .join("staging/restores")
                .join(format!("{token}.json")),
        );
        Ok(())
    }
    pub fn restore_backup(
        &mut self,
        r: RestoreRequest,
        authorize: impl FnOnce(&[PathBuf]) -> Result<()>,
    ) -> Result<RestoreResult> {
        self.check_dataset(&r.dataset_id)?;
        if !r.confirmed {
            return Err(invalid("请先明确确认替换当前全部数据。"));
        }
        let receipt = self.receipt(&r.token)?;
        if receipt.preview.dataset_id != self.dataset_id
            || receipt.preview.backup_digest != r.backup_digest
            || hash_file(&receipt.source)? != r.backup_digest
            || hash_file(&self.dataset_dir().join("lifeisrpg.sqlite3"))? != receipt.current_hash
        {
            return Err(conflict());
        }
        let candidate = self.root.join("datasets").join(&receipt.candidate_id);
        for file in &receipt.manifest.files {
            let path = candidate.join(&file.path);
            if fs::metadata(&path)?.len() != file.bytes || hash_file(&path)? != file.sha256 {
                return Err(invalid("已校验的恢复文件发生变化，请重新选择备份。"));
            }
        }
        validate_dataset(&candidate)?;
        let conn = Connection::open_with_flags(
            candidate.join("lifeisrpg.sqlite3"),
            OpenFlags::SQLITE_OPEN_READ_WRITE,
        )?;
        conn.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL; PRAGMA journal_mode=DELETE;")?;
        let scopes = vec![
            candidate.join("images"),
            self.root
                .join("staging/imports")
                .join(&receipt.candidate_id)
                .join("images"),
        ];
        for path in &scopes {
            fs::create_dir_all(path)?;
        }
        authorize(&scopes)?;
        checkpoint("before_pointer")?;
        let old = self.dataset_id.clone();
        let switch = (|| {
            switch_pointer(&self.root, &receipt.candidate_id)?;
            checkpoint("pointer_replaced")?;
            checkpoint("activate")?;
            validate_database(&conn)?;
            Ok(())
        })();
        if let Err(e) = switch {
            if checkpoint("rollback")
                .and_then(|_| switch_pointer(&self.root, &old))
                .is_err()
            {
                self.blocked = true;
                self.ensure_ready()?;
            }
            return Err(e);
        }
        let previous = std::mem::replace(&mut self.conn, conn);
        self.dataset_id = receipt.candidate_id;
        drop(previous);
        let cleanup_pending = checkpoint("cleanup")
            .and_then(|_| self.cleanup_obsolete())
            .is_err();
        let _ = fs::remove_file(
            self.root
                .join("staging/restores")
                .join(format!("{}.json", r.token)),
        );
        Ok(RestoreResult {
            dataset_id: self.dataset_id.clone(),
            cleanup_pending,
        })
    }
    pub(super) fn finish_startup(&mut self) -> Result<()> {
        let check: String = self
            .conn
            .query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if check != "ok" {
            return Err(invalid("本地数据库检查失败；原文件已保留。"));
        }
        let _ = self.cleanup_images();
        let staging = self.root.join("staging");
        if staging.exists() {
            let _ = fs::remove_dir_all(staging);
        }
        let datasets = fs::read_dir(self.root.join("datasets"))?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .count();
        if datasets > 1 {
            validate_dataset(&self.dataset_dir())?;
            let _ = self.cleanup_obsolete();
        }
        for path in self.image_scopes() {
            fs::create_dir_all(path)?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests;
