use super::super::events::digest;
use super::*;
use std::io::Cursor;
const TODAY: &str = "2026-10-18";
const NOW: &str = "2026-10-18T12:00:00.000Z";
struct TestDir(PathBuf);
impl TestDir {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("lifeisrpg-storage-{}", Uuid::new_v4()));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn open(dir: &TestDir) -> Store {
    Store::open(&dir.0.join("app")).unwrap()
}
fn image(path: &Path) -> Vec<u8> {
    let mut bytes = Cursor::new(vec![]);
    image::DynamicImage::new_rgb8(3, 2)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    fs::write(path, bytes.get_ref()).unwrap();
    bytes.into_inner()
}
fn draft(images: &[ImageView]) -> EventDraft {
    EventDraft {
        title: "真实成果".into(),
        event_on: "2026-10-17".into(),
        note: "保存原图与必要关联".into(),
        tags: vec!["日常".into(), "成果".into()],
        is_milestone: true,
        goal_id: None,
        quest_id: None,
        image_ids: images.iter().map(|a| a.id.clone()).collect(),
    }
}
fn save(s: &mut Store, id: &str, revision: Option<i64>, d: EventDraft) -> LifeEvent {
    s.save_event(
        SaveEvent {
            dataset_id: s.dataset_id.clone(),
            event_id: id.into(),
            expected_revision: revision,
            draft: d,
        },
        NOW,
    )
    .unwrap()
    .event
    .unwrap()
}
fn import(s: &Store, dir: &TestDir) -> Vec<ImageView> {
    let p = dir.0.join("original.png");
    image(&p);
    s.import_images(&s.dataset_id, &[p]).unwrap()
}
fn populated(s: &mut Store, dir: &TestDir) -> LifeEvent {
    let goal = s
        .save_goal(
            SaveGoalRequest {
                dataset_id: s.dataset_id.clone(),
                expected_revision: None,
                goal: Goal {
                    id: Uuid::new_v4().to_string(),
                    name: "长期目标".into(),
                    description: "真实目标".into(),
                    kind: "main".into(),
                    status: "completed".into(),
                    archived: false,
                    revision: 0,
                },
            },
            TODAY,
            NOW,
        )
        .unwrap();
    let q = s
        .create(
            CreateRequest {
                dataset_id: s.dataset_id.clone(),
                quest_id: Uuid::new_v4().to_string(),
                plan_id: Uuid::new_v4().to_string(),
                draft: TaskDraft {
                    title: "原任务标题".into(),
                    description: "一步".into(),
                    goal_ids: vec![goal.id.clone()],
                    parent_id: None,
                    date: TODAY.into(),
                    slot: "morning".into(),
                    start_time: None,
                    end_time: None,
                },
            },
            NOW,
        )
        .unwrap()
        .item;
    s.record_execution(
        ExecutionRequest {
            dataset_id: s.dataset_id.clone(),
            quest_id: q.quest_id.clone(),
            plan_id: None,
            record_id: Uuid::new_v4().to_string(),
            kind: "progress".into(),
            occurred_on: "2026-10-17".into(),
            note: "历史补记".into(),
            expected_revision: q.quest_revision,
            expected_plan_revision: None,
            completion_id: None,
        },
        TODAY,
        NOW,
    )
    .unwrap();
    s.save_review(
        ReviewRequest {
            dataset_id: s.dataset_id.clone(),
            date: "2026-10-17".into(),
            action: "reviewed".into(),
        },
        TODAY,
        NOW,
    )
    .unwrap();
    let images = import(s, dir);
    let mut d = draft(&images);
    d.goal_id = Some(goal.id);
    d.quest_id = Some(q.quest_id);
    let e = save(s, &Uuid::new_v4().to_string(), None, d);
    s.discard_images(
        &s.dataset_id,
        &images.iter().map(|a| a.id.clone()).collect::<Vec<_>>(),
    )
    .unwrap();
    let mut prefs = s.preferences().unwrap();
    prefs.sound_enabled = false;
    s.save_preferences(PreferencesRequest {
        dataset_id: s.dataset_id.clone(),
        preferences: prefs,
    })
    .unwrap();
    e
}
fn export(s: &Store, dir: &TestDir) -> PathBuf {
    let path = dir.0.join("complete.zip");
    s.export_backup(&s.dataset_id, &path, NOW).unwrap();
    path
}
fn request(s: &Store, p: &RestorePreview) -> RestoreRequest {
    RestoreRequest {
        dataset_id: s.dataset_id.clone(),
        token: p.token.clone(),
        backup_digest: p.backup_digest.clone(),
        confirmed: true,
    }
}
fn change_zip(
    source: &Path,
    destination: &Path,
    change: impl FnOnce(&mut BTreeMap<String, Vec<u8>>),
) {
    let mut zip = ZipArchive::new(fs::File::open(source).unwrap()).unwrap();
    let mut files = BTreeMap::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).unwrap();
        let mut bytes = vec![];
        f.read_to_end(&mut bytes).unwrap();
        files.insert(f.name().into(), bytes);
    }
    change(&mut files);
    let mut out = ZipWriter::new(fs::File::create(destination).unwrap());
    for (name, bytes) in files {
        out.start_file(name, SimpleFileOptions::default()).unwrap();
        out.write_all(&bytes).unwrap();
    }
    out.finish().unwrap();
}
fn update_manifest(files: &mut BTreeMap<String, Vec<u8>>) {
    let mut m: Manifest = serde_json::from_slice(&files["manifest.json"]).unwrap();
    for f in &mut m.files {
        if let Some(bytes) = files.get(&f.path) {
            f.bytes = bytes.len() as u64;
            f.sha256 = digest(bytes);
        }
    }
    files.insert("manifest.json".into(), serde_json::to_vec(&m).unwrap());
}
#[test]
fn image_import_snapshot_sharing_milestone_and_real_deletion_keep_other_references() {
    let dir = TestDir::new();
    let mut s = open(&dir);
    let imported = import(&s, &dir);
    let original = fs::read(dir.0.join("original.png")).unwrap();
    let first = save(&mut s, &Uuid::new_v4().to_string(), None, draft(&imported));
    fs::remove_file(dir.0.join("original.png")).unwrap();
    let a = assets(&s.conn).unwrap().remove(0);
    assert_eq!(
        fs::read(s.dataset_dir().join(&a.relative_path)).unwrap(),
        original
    );
    let other = save(
        &mut s,
        &Uuid::new_v4().to_string(),
        None,
        draft(&first.images),
    );
    let mut edit = draft(&first.images);
    edit.is_milestone = false;
    let first = save(&mut s, &first.id, Some(first.revision), edit);
    assert!(!first.is_milestone);
    assert_eq!(s.list_events().unwrap().len(), 2);
    s.delete_event(DeleteEvent {
        dataset_id: s.dataset_id.clone(),
        event_id: first.id,
        expected_revision: first.revision,
    })
    .unwrap();
    assert!(s.dataset_dir().join(&a.relative_path).is_file());
    assert_eq!(s.event(&other.id).unwrap().images.len(), 1);
    s.delete_event(DeleteEvent {
        dataset_id: s.dataset_id.clone(),
        event_id: other.id,
        expected_revision: other.revision,
    })
    .unwrap();
    assert!(!s.dataset_dir().join(&a.relative_path).exists());
    assert!(assets(&s.conn).unwrap().is_empty());
    s.discard_images(
        &s.dataset_id,
        &imported.iter().map(|a| a.id.clone()).collect::<Vec<_>>(),
    )
    .unwrap();
}
#[test]
fn image_save_failure_rolls_back_and_keeps_staging_for_retry() {
    let dir = TestDir::new();
    let mut s = open(&dir);
    let picked = import(&s, &dir);
    let id = Uuid::new_v4().to_string();
    s.conn.execute_batch("CREATE TRIGGER fail_event BEFORE INSERT ON life_event_assets BEGIN SELECT RAISE(ABORT,'event failed'); END;").unwrap();
    let r = SaveEvent {
        dataset_id: s.dataset_id.clone(),
        event_id: id.clone(),
        expected_revision: None,
        draft: draft(&picked),
    };
    assert!(s.save_event(r, NOW).is_err());
    assert!(s.list_events().unwrap().is_empty());
    assert!(assets(&s.conn).unwrap().is_empty());
    assert_eq!(
        fs::read_dir(s.dataset_dir().join("images"))
            .unwrap()
            .count(),
        0
    );
    s.conn.execute_batch("DROP TRIGGER fail_event").unwrap();
    let event = save(&mut s, &id, None, draft(&picked));
    let again = save(&mut s, &id, None, draft(&picked));
    assert_eq!(event.revision, again.revision);
    assert_eq!(s.list_events().unwrap().len(), 1);
    let mut invalid = draft(&event.images);
    invalid.title = "".into();
    assert!(s
        .save_event(
            SaveEvent {
                dataset_id: s.dataset_id.clone(),
                event_id: id,
                expected_revision: Some(event.revision),
                draft: invalid
            },
            NOW
        )
        .is_err());
    let broken = dir.0.join("broken.png");
    fs::write(&broken, b"not an image").unwrap();
    assert!(s.import_images(&s.dataset_id, &[broken]).is_err());
}
#[test]
fn failed_resource_cleanup_is_retryable_and_never_deletes_shared_files() {
    let dir = TestDir::new();
    let mut s = open(&dir);
    let images = import(&s, &dir);
    let event = save(&mut s, &Uuid::new_v4().to_string(), None, draft(&images));
    let a = assets(&s.conn).unwrap().remove(0);
    let path = s.dataset_dir().join(&a.relative_path);
    let bytes = fs::read(&path).unwrap();
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    let result = s
        .delete_event(DeleteEvent {
            dataset_id: s.dataset_id.clone(),
            event_id: event.id,
            expected_revision: event.revision,
        })
        .unwrap();
    assert!(result.cleanup_pending);
    assert_eq!(assets(&s.conn).unwrap().len(), 1);
    assert!(s.list_events().unwrap().is_empty());
    fs::remove_dir(&path).unwrap();
    fs::write(&path, bytes).unwrap();
    s.cleanup_images().unwrap();
    assert!(assets(&s.conn).unwrap().is_empty());
    assert!(!path.exists());
}
#[test]
fn complete_backup_restores_database_images_preferences_and_rejects_old_session() {
    let dir = TestDir::new();
    let mut s = open(&dir);
    let event = populated(&mut s, &dir);
    let before = counts(&s.conn).unwrap();
    let path = export(&s, &dir);
    let old = s.dataset_id.clone();
    let old_dir = s.dataset_dir();
    let image = assets(&s.conn).unwrap().remove(0);
    let bytes = fs::read(old_dir.join(&image.relative_path)).unwrap();
    save(&mut s, &Uuid::new_v4().to_string(), None, draft(&[]));
    let preview = s.inspect_backup(&s.dataset_id, &path).unwrap();
    assert_eq!(preview.incoming, before);
    assert_eq!(preview.current.events, before.events + 1);
    s.storage_info().unwrap();
    assert!(s
        .root
        .join("datasets")
        .join(s.receipt(&preview.token).unwrap().candidate_id)
        .exists());
    let mut authorized = vec![];
    let result = s
        .restore_backup(request(&s, &preview), |paths| {
            authorized = paths.to_vec();
            Ok(())
        })
        .unwrap();
    assert_ne!(result.dataset_id, old);
    assert!(!result.cleanup_pending);
    assert!(!old_dir.exists());
    assert!(authorized[0].starts_with(s.dataset_dir()));
    assert_eq!(counts(&s.conn).unwrap(), before);
    assert_eq!(
        fs::read(s.dataset_dir().join(&image.relative_path)).unwrap(),
        bytes
    );
    assert!(!s.preferences().unwrap().sound_enabled);
    assert_eq!(s.event(&event.id).unwrap().tags, vec!["日常", "成果"]);
    assert!(s.require_dataset(&old).is_err());
    assert!(s.load_date("2026-10-17", TODAY).unwrap().summary.reviewed);
    assert_eq!(
        s.load_date("2026-10-17", TODAY).unwrap().summary.progressed,
        1
    );
    let new = s.dataset_id.clone();
    drop(s);
    let s = open(&dir);
    assert_eq!(s.dataset_id, new);
    assert_eq!(s.event(&event.id).unwrap().images.len(), 1);
}
#[test]
fn damaged_missing_unsafe_and_wrong_version_backups_never_replace_current_data() {
    let dir = TestDir::new();
    let mut s = open(&dir);
    populated(&mut s, &dir);
    let path = export(&s, &dir);
    let image_name = assets(&s.conn).unwrap().remove(0).relative_path;
    let old_id = s.dataset_id.clone();
    let old_hash = hash_file(&s.dataset_dir().join("lifeisrpg.sqlite3")).unwrap();
    for case in [
        "missing", "corrupt", "path", "absolute", "unknown", "version", "size", "decode",
    ] {
        let bad = dir.0.join(format!("{case}.zip"));
        change_zip(&path, &bad, |files| match case {
            "missing" => {
                files.remove(&image_name);
            }
            "corrupt" => {
                files.insert(image_name.clone(), vec![1, 2, 3]);
            }
            "path" => {
                files.insert("../outside".into(), vec![1]);
            }
            "absolute" => {
                files.insert("/tmp/outside".into(), vec![1]);
            }
            "unknown" => {
                files.insert("unknown.txt".into(), vec![1]);
            }
            "version" => {
                let mut m: Manifest = serde_json::from_slice(&files["manifest.json"]).unwrap();
                m.schema_version = 999;
                files.insert("manifest.json".into(), serde_json::to_vec(&m).unwrap());
            }
            "size" => {
                let mut m: Manifest = serde_json::from_slice(&files["manifest.json"]).unwrap();
                m.files[0].bytes = MAX_TOTAL + 1;
                files.insert("manifest.json".into(), serde_json::to_vec(&m).unwrap());
            }
            "decode" => {
                files.insert(image_name.clone(), vec![1, 2, 3]);
                update_manifest(files);
            }
            _ => unreachable!(),
        });
        assert!(s.inspect_backup(&s.dataset_id, &bad).is_err(), "{case}");
        assert_eq!(s.dataset_id, old_id);
        assert_eq!(
            hash_file(&s.dataset_dir().join("lifeisrpg.sqlite3")).unwrap(),
            old_hash
        );
    }
    let symlink = dir.0.join("symlink.zip");
    let mut zip = ZipWriter::new(fs::File::create(&symlink).unwrap());
    zip.add_symlink(image_name, "../../outside", SimpleFileOptions::default())
        .unwrap();
    zip.finish().unwrap();
    assert!(s.inspect_backup(&s.dataset_id, &symlink).is_err());
    assert!(!dir.0.join("outside").exists());
}
#[test]
fn matching_checksums_do_not_allow_bad_sql_schema_dates_or_business_cycles() {
    let dir = TestDir::new();
    let mut s = open(&dir);
    populated(&mut s, &dir);
    let path = export(&s, &dir);
    for (name,sql) in [
        ("cycle", "INSERT INTO quests SELECT '00000000-0000-0000-0000-000000000000','cycle','',id,mode,status,archived_on,created_at,revision FROM quests LIMIT 1; UPDATE quests SET parent_id='00000000-0000-0000-0000-000000000000' WHERE id<>'00000000-0000-0000-0000-000000000000';"),("trigger","CREATE TRIGGER bad AFTER DELETE ON quests BEGIN SELECT 1; END;"),("date","UPDATE execution_records SET occurred_on='2026-02-30';"),("scope","UPDATE quests SET status='completed';"),("snapshot","UPDATE plans SET content_snapshot='{\"title\":\"\",\"description\":\"\",\"goals\":[]}';")]{
  let bad=dir.0.join(format!("{name}.zip"));change_zip(&path,&bad,|files|{let db=dir.0.join(format!("{name}.sqlite3"));fs::write(&db,&files["lifeisrpg.sqlite3"]).unwrap();let conn=Connection::open(&db).unwrap();conn.execute_batch(sql).unwrap();drop(conn);files.insert("lifeisrpg.sqlite3".into(),fs::read(db).unwrap());update_manifest(files);});assert!(s.inspect_backup(&s.dataset_id,&bad).is_err(),"{name}");
 }
}
#[test]
fn backup_refuses_missing_images_and_preserves_existing_export_on_failure() {
    let dir = TestDir::new();
    let mut s = open(&dir);
    populated(&mut s, &dir);
    let path = export(&s, &dir);
    let bytes = fs::read(&path).unwrap();
    let a = assets(&s.conn).unwrap().remove(0);
    fs::remove_file(s.dataset_dir().join(a.relative_path)).unwrap();
    assert!(s.export_backup(&s.dataset_id, &path, NOW).is_err());
    assert_eq!(fs::read(path).unwrap(), bytes);
}
#[test]
fn changed_source_current_state_and_prepared_candidate_expire_restore_preview() {
    let dir = TestDir::new();
    let mut s = open(&dir);
    populated(&mut s, &dir);
    let path = export(&s, &dir);
    let preview = s.inspect_backup(&s.dataset_id, &path).unwrap();
    save(&mut s, &Uuid::new_v4().to_string(), None, draft(&[]));
    assert!(s.restore_backup(request(&s, &preview), |_| Ok(())).is_err());
    s.discard_backup(&s.dataset_id, &preview.token).unwrap();
    let preview = s.inspect_backup(&s.dataset_id, &path).unwrap();
    fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"changed")
        .unwrap();
    assert!(s.restore_backup(request(&s, &preview), |_| Ok(())).is_err());
    s.discard_backup(&s.dataset_id, &preview.token).unwrap();
    let path = export(&s, &dir);
    let preview = s.inspect_backup(&s.dataset_id, &path).unwrap();
    let candidate = s
        .root
        .join("datasets")
        .join(s.receipt(&preview.token).unwrap().candidate_id);
    let conn = Connection::open(candidate.join("lifeisrpg.sqlite3")).unwrap();
    conn.execute("UPDATE life_events SET title='tampered'", [])
        .unwrap();
    drop(conn);
    assert!(s.restore_backup(request(&s, &preview), |_| Ok(())).is_err());
}
#[test]
fn pointer_and_activation_failures_roll_back_and_cleanup_failure_keeps_new_dataset() {
    let dir = TestDir::new();
    let mut s = open(&dir);
    populated(&mut s, &dir);
    let path = export(&s, &dir);
    let old_id = s.dataset_id.clone();
    let old_dir = s.dataset_dir();
    for step in ["before_pointer", "pointer_replaced", "activate"] {
        let p = s.inspect_backup(&s.dataset_id, &path).unwrap();
        FAIL_AT.set(Some(step));
        assert!(s.restore_backup(request(&s, &p), |_| Ok(())).is_err());
        FAIL_AT.set(None);
        assert_eq!(s.dataset_id, old_id);
        assert_eq!(
            serde_json::from_slice::<ActivePointer>(&fs::read(s.root.join("active.json")).unwrap())
                .unwrap()
                .dataset_id
                .to_string(),
            old_id
        );
        assert!(old_dir.exists());
        s.discard_backup(&s.dataset_id, &p.token).unwrap();
    }
    let p = s.inspect_backup(&s.dataset_id, &path).unwrap();
    assert!(s
        .restore_backup(request(&s, &p), |_| Err(invalid(
            "scope activation failure"
        )))
        .is_err());
    assert_eq!(s.dataset_id, old_id);
    s.discard_backup(&s.dataset_id, &p.token).unwrap();
    let p = s.inspect_backup(&s.dataset_id, &path).unwrap();
    FAIL_AT.set(Some("cleanup"));
    let result = s.restore_backup(request(&s, &p), |_| Ok(())).unwrap();
    FAIL_AT.set(None);
    assert!(result.cleanup_pending);
    assert!(old_dir.exists());
    let id = s.dataset_id.clone();
    drop(s);
    let s = open(&dir);
    assert_eq!(s.dataset_id, id);
    assert!(!old_dir.exists());
    assert_eq!(s.list_events().unwrap().len(), 1);
}
#[test]
fn restart_uses_only_the_atomically_committed_pointer_and_keeps_complete_data() {
    let dir = TestDir::new();
    let mut s = open(&dir);
    let event = populated(&mut s, &dir);
    let path = export(&s, &dir);
    let old = s.dataset_id.clone();
    let p = s.inspect_backup(&s.dataset_id, &path).unwrap();
    let candidate = s.receipt(&p.token).unwrap().candidate_id;
    fs::write(s.root.join("active-incomplete.tmp"), b"partial pointer").unwrap();
    drop(s);
    let s = open(&dir);
    assert_eq!(s.dataset_id, old);
    assert_eq!(s.event(&event.id).unwrap().images.len(), 1);
    assert!(!s.root.join("datasets").join(candidate).exists());
    let p = s.inspect_backup(&s.dataset_id, &path).unwrap();
    let candidate = s.receipt(&p.token).unwrap().candidate_id;
    let root = s.root.clone();
    switch_pointer(&root, &candidate).unwrap();
    drop(s);
    let s = open(&dir);
    assert_eq!(s.dataset_id, candidate);
    assert_eq!(s.event(&event.id).unwrap().images.len(), 1);
    assert!(!root.join("datasets").join(old).exists());
}

#[test]
fn rollback_failure_stops_old_session_writes_until_restart() {
    let dir = TestDir::new();
    let mut s = open(&dir);
    populated(&mut s, &dir);
    let path = export(&s, &dir);
    let p = s.inspect_backup(&s.dataset_id, &path).unwrap();
    let old = s.dataset_id.clone();
    FAIL_AT.set(Some("rollback"));
    assert!(s.restore_backup(request(&s, &p), |_| Ok(())).is_err());
    FAIL_AT.set(None);
    assert!(s.ensure_ready().is_err());
    assert!(s.require_dataset(&old).is_err());
    drop(s);
    let s = open(&dir);
    assert_ne!(s.dataset_id, old);
    assert_eq!(s.list_events().unwrap().len(), 1);
    assert_eq!(s.list_events().unwrap()[0].images.len(), 1);
}
