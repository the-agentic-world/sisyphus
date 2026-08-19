use super::*;

fn doctor_json(project: &Path, codex_home: &Path, repair: bool) -> serde_json::Value {
    let mut command = megara_with_codex_home(codex_home);
    command
        .args(["doctor", "--scope", "project", "--target", "codex"])
        .current_dir(project);
    if repair {
        command.arg("--repair");
    }
    let output = command.arg("--json").output().unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn start_doctor_session(
    project: &Path,
    command_id: &str,
) -> (String, std::path::PathBuf, Vec<String>) {
    let mut store = super::planning::store::PlanningStore::open_project(project).unwrap();
    let database_path = store.database_path().to_path_buf();
    let started = store
        .start(
            command_id,
            &format!("sha256:{command_id}"),
            super::planning::engine::StartCommand {
                session_id: None,
                project_id: store.project_id().to_string(),
                request: "doctor health test".to_string(),
                title: Some("Doctor health test".to_string()),
            },
        )
        .unwrap();
    let session_id = started.state.session_id.clone();
    let event_hashes = store
        .event_envelopes(&session_id)
        .unwrap()
        .into_iter()
        .map(|event| event.state_hash_after)
        .collect();
    (session_id, database_path, event_hashes)
}

fn doctor_warning(report: &serde_json::Value, code: &str) -> bool {
    report["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|warning| warning.as_str())
        .any(|warning| warning.contains(code))
}

fn doctor_issue(report: &serde_json::Value, code: &str) -> bool {
    report["issues"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|issue| issue["code"] == code)
}

#[test]
fn doctor_reports_missing_then_ok() {
    let dir = tempdir().unwrap();
    let codex_home = tempdir().unwrap();

    let missing = megara_with_codex_home(codex_home.path())
        .arg("doctor")
        .arg("--scope")
        .arg("project")
        .arg("--target")
        .arg("codex")
        .arg("--json")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(missing.status.success());
    let missing_stdout = String::from_utf8_lossy(&missing.stdout);
    assert!(missing_stdout.contains("\"ok\": false"));

    let install = megara_with_codex_home(codex_home.path())
        .arg("install")
        .arg("--scope")
        .arg("project")
        .arg("--target")
        .arg("codex")
        .arg("--trust-project")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(install.status.success());

    let agents_md = dir.path().join(".codex/AGENTS.md");
    fs::write(&agents_md, "# MEGARA:MANAGED\nstale").unwrap();

    let stale = megara_with_codex_home(codex_home.path())
        .arg("doctor")
        .arg("--scope")
        .arg("project")
        .arg("--target")
        .arg("codex")
        .arg("--json")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(stale.status.success());
    let stale_stdout = String::from_utf8_lossy(&stale.stdout);
    assert!(stale_stdout.contains("\"ok\": false"));
    assert!(stale_stdout.contains(".codex/AGENTS.md"));
    assert!(doctor_issue(
        &serde_json::from_str(&stale_stdout).unwrap(),
        "PROJECTION_STALE"
    ));

    let sync = megara_with_codex_home(codex_home.path())
        .arg("sync")
        .arg("--scope")
        .arg("project")
        .arg("--target")
        .arg("codex")
        .arg("--force")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(sync.status.success());

    let ok = megara_with_codex_home(codex_home.path())
        .arg("doctor")
        .arg("--scope")
        .arg("project")
        .arg("--target")
        .arg("codex")
        .arg("--json")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(ok.status.success());
    let ok_stdout = String::from_utf8_lossy(&ok.stdout);
    assert!(ok_stdout.contains("\"ok\": true"));
    assert!(ok_stdout.contains("\"warnings\": []"));

    let human = megara_with_codex_home(codex_home.path())
        .arg("doctor")
        .arg("--scope")
        .arg("project")
        .arg("--target")
        .arg("codex")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(human.status.success());
    let human_stdout = String::from_utf8_lossy(&human.stdout);
    assert!(human_stdout.contains("Megara / Doctor"));
    assert!(human_stdout.contains("megara doctor: scope=project, target=codex, ok=true"));
}

#[test]
fn doctor_repair_restores_stale_runtime_projection_without_touching_planning_db() {
    let dir = tempdir().unwrap();
    let codex_home = tempdir().unwrap();
    install_project_harness(dir.path(), codex_home.path());
    let (_session_id, database_path, _event_hashes) =
        start_doctor_session(dir.path(), "cmd-doctor-runtime-projection");
    let planner = dir.path().join(".codex/agents/planner.toml");
    let canonical = fs::read(&planner).unwrap();
    let database_before = fs::read(&database_path).unwrap();
    fs::write(
        &planner,
        [canonical.as_slice(), b"\n# UAT doctor drift sentinel\n"].concat(),
    )
    .unwrap();

    let read_only = doctor_json(dir.path(), codex_home.path(), false);
    assert!(doctor_issue(&read_only, "PROJECTION_STALE"));
    assert!(fs::read(&planner)
        .unwrap()
        .ends_with(b"# UAT doctor drift sentinel\n"));
    assert_eq!(fs::read(&database_path).unwrap(), database_before);

    let repaired = doctor_json(dir.path(), codex_home.path(), true);
    assert_eq!(repaired["ok"], true, "report={repaired}");
    assert_eq!(fs::read(&planner).unwrap(), canonical);
    assert_eq!(fs::read(&database_path).unwrap(), database_before);
    assert!(repaired["observations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| {
            item.as_str()
                .is_some_and(|item| item.contains("Managed projection repair"))
        }));

    let second = doctor_json(dir.path(), codex_home.path(), true);
    assert_eq!(second["ok"], true, "report={second}");
    assert!(second["observations"]
        .as_array()
        .unwrap()
        .iter()
        .all(|item| !item
            .as_str()
            .is_some_and(|item| item.contains("Managed projection repair"))));
    assert_eq!(fs::read(&database_path).unwrap(), database_before);
}

#[test]
fn doctor_reports_broken_project_wrapper() {
    let dir = tempdir().unwrap();
    let codex_home = tempdir().unwrap();
    install_project_harness(dir.path(), codex_home.path());

    fs::write(dir.path().join(".agents/bin/megara"), "not executable").unwrap();

    let output = megara_with_codex_home(codex_home.path())
        .arg("doctor")
        .arg("--scope")
        .arg("project")
        .arg("--target")
        .arg("codex")
        .arg("--json")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"ok\": false"));
    assert!(stdout.contains(".agents/bin/megara"));
}

#[test]
fn doctor_is_read_only_and_repairs_diverged_replay_cache_without_events() {
    let dir = tempdir().unwrap();
    let codex_home = tempdir().unwrap();
    install_project_harness(dir.path(), codex_home.path());
    let (session_id, database_path, event_hashes) =
        start_doctor_session(dir.path(), "cmd-doctor-cache");

    {
        let connection = rusqlite::Connection::open(&database_path).unwrap();
        connection
            .execute(
                "UPDATE sessions SET state_json=?1, normalized_state_hash=?2 WHERE session_id=?3",
                rusqlite::params!["{broken", "sha256:broken", session_id],
            )
            .unwrap();
    }

    let read_only = doctor_json(dir.path(), codex_home.path(), false);
    assert_eq!(read_only["ok"], false);
    assert!(doctor_warning(&read_only, "PROJECTION_DIVERGED"));
    let cached_after_read_only: String = rusqlite::Connection::open(&database_path)
        .unwrap()
        .query_row(
            "SELECT state_json FROM sessions WHERE session_id=?1",
            [&session_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(cached_after_read_only, "{broken");
    let unchanged_after_read_only = super::planning::store::PlanningStore::open_project(dir.path())
        .unwrap()
        .event_envelopes(&session_id)
        .unwrap()
        .into_iter()
        .map(|event| event.state_hash_after)
        .collect::<Vec<_>>();
    assert_eq!(unchanged_after_read_only, event_hashes);

    let repaired = doctor_json(dir.path(), codex_home.path(), true);
    assert_eq!(repaired["warnings"], serde_json::json!([]));
    assert!(repaired["observations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|observation| observation
            .as_str()
            .is_some_and(|observation| observation.contains("events unchanged"))));
    let repaired_store = super::planning::store::PlanningStore::open_project(dir.path()).unwrap();
    assert!(repaired_store.current(&session_id).is_ok());
    let unchanged_after_repair = repaired_store
        .event_envelopes(&session_id)
        .unwrap()
        .into_iter()
        .map(|event| event.state_hash_after)
        .collect::<Vec<_>>();
    assert_eq!(unchanged_after_repair, event_hashes);
}

#[test]
fn doctor_repairs_missing_projection_without_new_event() {
    let dir = tempdir().unwrap();
    let codex_home = tempdir().unwrap();
    install_project_harness(dir.path(), codex_home.path());
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    let (session_id, _database_path, _initial_event_hashes) =
        start_doctor_session(dir.path(), "cmd-doctor-projection");
    super::planning_cli_artifact_support::prepare_complete(dir.path(), &session_id);
    let event_hashes = super::planning::store::PlanningStore::open_project(dir.path())
        .unwrap()
        .event_envelopes(&session_id)
        .unwrap()
        .into_iter()
        .map(|event| event.state_hash_after)
        .collect::<Vec<_>>();

    let projection = dir
        .path()
        .join(".megara/planning/artifacts")
        .join(&session_id)
        .join("spec.md");
    assert!(projection.is_file());
    fs::remove_file(&projection).unwrap();

    let read_only = doctor_json(dir.path(), codex_home.path(), false);
    assert_eq!(read_only["ok"], false);
    assert!(doctor_warning(&read_only, "PROJECTION_MISSING"));
    assert!(doctor_issue(&read_only, "PROJECTION_MISSING"));
    assert!(!projection.exists());

    let repaired = doctor_json(dir.path(), codex_home.path(), true);
    assert_eq!(repaired["warnings"], serde_json::json!([]));
    assert!(projection.is_file());
    assert!(repaired["observations"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |observation| observation.as_str().is_some_and(|observation| {
                observation.contains(&format!("session={session_id}, kind=spec"))
            })
        ));
    let store = super::planning::store::PlanningStore::open_project(dir.path()).unwrap();
    let repaired_event_hashes = store
        .event_envelopes(&session_id)
        .unwrap()
        .into_iter()
        .map(|event| event.state_hash_after)
        .collect::<Vec<_>>();
    assert_eq!(repaired_event_hashes, event_hashes);
}

#[test]
fn doctor_repair_overwrites_stale_managed_projection() {
    let dir = tempdir().unwrap();
    let codex_home = tempdir().unwrap();
    install_project_harness(dir.path(), codex_home.path());
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    let (session_id, _database_path, _event_hashes) =
        start_doctor_session(dir.path(), "cmd-doctor-stale-projection");
    super::planning_cli_artifact_support::prepare_complete(dir.path(), &session_id);
    let projection = dir
        .path()
        .join(".megara/planning/artifacts")
        .join(&session_id)
        .join("spec.md");
    let original = fs::read(&projection).unwrap();
    fs::write(
        &projection,
        [original.as_slice(), b"\nUAT drift\n"].concat(),
    )
    .unwrap();

    let read_only = doctor_json(dir.path(), codex_home.path(), false);
    assert!(doctor_issue(&read_only, "PROJECTION_STALE"));
    let repaired = doctor_json(dir.path(), codex_home.path(), true);
    assert_eq!(repaired["warnings"], serde_json::json!([]));
    assert_eq!(fs::read(&projection).unwrap(), original);
}

#[test]
fn doctor_repairs_clean_tombstone_artifact_residue_without_warning() {
    let dir = tempdir().unwrap();
    let codex_home = tempdir().unwrap();
    install_project_harness(dir.path(), codex_home.path());
    let (session_id, _database_path, _event_hashes) =
        start_doctor_session(dir.path(), "cmd-doctor-residue");
    let mut store = super::planning::store::PlanningStore::open_project(dir.path()).unwrap();
    let receipt = store
        .purge(
            &session_id,
            "cmd-doctor-residue-purge",
            "sha256:doctor-residue-purge",
            1,
            &session_id,
        )
        .unwrap();
    assert_eq!(receipt.cleanup_state, "clean");
    drop(store);

    let residue = dir
        .path()
        .join(".megara/planning/artifacts")
        .join(&session_id);
    fs::create_dir_all(&residue).unwrap();
    fs::write(residue.join("leftover.md"), "residue\n").unwrap();

    let read_only = doctor_json(dir.path(), codex_home.path(), false);
    assert_eq!(read_only["ok"], false);
    assert!(doctor_warning(&read_only, "PURGE_RESIDUE"));
    assert!(doctor_issue(&read_only, "PURGE_RESIDUE"));
    assert!(residue.exists());

    let repaired = doctor_json(dir.path(), codex_home.path(), true);
    assert_eq!(repaired["warnings"], serde_json::json!([]));
    assert!(!residue.exists());
    assert!(repaired["observations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|observation| observation
            .as_str()
            .is_some_and(|observation| observation.contains("repaired=1, pending=0"))));
}

#[test]
fn doctor_reports_invalid_tombstone_without_rewriting_it() {
    let dir = tempdir().unwrap();
    let codex_home = tempdir().unwrap();
    install_project_harness(dir.path(), codex_home.path());
    let (session_id, database_path, _event_hashes) =
        start_doctor_session(dir.path(), "cmd-doctor-corrupt-tombstone");
    let mut store = super::planning::store::PlanningStore::open_project(dir.path()).unwrap();
    store
        .purge(
            &session_id,
            "cmd-doctor-corrupt-tombstone-purge",
            "sha256:doctor-corrupt-tombstone-purge",
            1,
            &session_id,
        )
        .unwrap();
    drop(store);

    let invalid_response = serde_json::json!({
        "purged": true,
        "session_id": session_id,
    })
    .to_string();
    let connection = rusqlite::Connection::open(&database_path).unwrap();
    connection
        .execute(
            "UPDATE purged_sessions SET core_response_json=?1 WHERE session_id=?2",
            rusqlite::params![invalid_response, session_id],
        )
        .unwrap();
    drop(connection);

    let read_only = doctor_json(dir.path(), codex_home.path(), false);
    assert_eq!(read_only["ok"], false);
    assert!(doctor_warning(&read_only, "TOMBSTONE_INVALID"));
    assert!(doctor_issue(&read_only, "TOMBSTONE_INVALID"));

    let repaired = doctor_json(dir.path(), codex_home.path(), true);
    assert_eq!(repaired["ok"], false);
    assert!(doctor_warning(&repaired, "TOMBSTONE_INVALID"));
    let stored_response: String = rusqlite::Connection::open(&database_path)
        .unwrap()
        .query_row(
            "SELECT core_response_json FROM purged_sessions WHERE session_id=?1",
            [&session_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored_response, invalid_response);
}

#[test]
fn doctor_reports_unsupported_tombstone_schema_without_rewriting_it() {
    let dir = tempdir().unwrap();
    let codex_home = tempdir().unwrap();
    install_project_harness(dir.path(), codex_home.path());
    let (session_id, database_path, _event_hashes) =
        start_doctor_session(dir.path(), "cmd-doctor-unsupported-tombstone");
    let mut store = super::planning::store::PlanningStore::open_project(dir.path()).unwrap();
    store
        .purge(
            &session_id,
            "cmd-doctor-unsupported-tombstone-purge",
            "sha256:doctor-unsupported-tombstone-purge",
            1,
            &session_id,
        )
        .unwrap();
    drop(store);
    let connection = rusqlite::Connection::open(&database_path).unwrap();
    connection
        .execute(
            "UPDATE purged_sessions SET purge_schema_version=999 WHERE session_id=?1",
            [&session_id],
        )
        .unwrap();
    drop(connection);

    let read_only = doctor_json(dir.path(), codex_home.path(), false);
    assert!(doctor_issue(&read_only, "TOMBSTONE_INVALID"));
    let repaired = doctor_json(dir.path(), codex_home.path(), true);
    assert!(doctor_issue(&repaired, "TOMBSTONE_INVALID"));
    let version: i64 = rusqlite::Connection::open(&database_path)
        .unwrap()
        .query_row(
            "SELECT purge_schema_version FROM purged_sessions WHERE session_id=?1",
            [&session_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, 999);
}

#[test]
fn doctor_json_reports_corrupt_planning_database_without_resetting_it() {
    let dir = tempdir().unwrap();
    let codex_home = tempdir().unwrap();
    install_project_harness(dir.path(), codex_home.path());
    let (_session_id, database_path, _event_hashes) =
        start_doctor_session(dir.path(), "cmd-doctor-corrupt-db");
    let corrupt = b"not a sqlite database";
    fs::write(&database_path, corrupt).unwrap();

    for repair in [false, true] {
        let mut command = megara_with_codex_home(codex_home.path());
        command
            .args([
                "doctor", "--scope", "project", "--target", "codex", "--json",
            ])
            .current_dir(dir.path());
        if repair {
            command.arg("--repair");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["ok"], false);
        assert!(doctor_issue(&report, "DB_CORRUPT"));
        assert_eq!(fs::read(&database_path).unwrap(), corrupt);
    }
}

#[cfg(unix)]
#[test]
fn doctor_repair_retries_pending_planning_purge_cleanup() {
    use std::os::unix::fs::symlink;

    let dir = tempdir().unwrap();
    let codex_home = tempdir().unwrap();
    super::planning_migration_support::install(dir.path(), codex_home.path());
    super::planning_migration_support::write_legacy_file(dir.path(), b"pending-cleanup");
    let applied =
        super::planning_migration_support::report(&super::planning_migration_support::run(
            dir.path(),
            codex_home.path(),
            &["--apply", "--json"],
        ));
    let migration_id = applied["migration_id"].as_str().unwrap();
    let session_id = applied["session_id"].as_str().unwrap();
    let backup_root = dir
        .path()
        .join(format!(".megara/migration-backups/{migration_id}"));
    let held_root = dir
        .path()
        .join(format!(".megara/migration-backups/{migration_id}-held"));
    fs::rename(&backup_root, &held_root).unwrap();
    symlink(&held_root, &backup_root).unwrap();

    let mut store = super::planning::store::PlanningStore::open_project(dir.path()).unwrap();
    let receipt = store
        .purge(
            session_id,
            "cmd-doctor-pending-cleanup",
            "sha256:doctor-pending-cleanup",
            1,
            session_id,
        )
        .unwrap();
    assert_eq!(receipt.cleanup_state, "pending");
    drop(store);

    let read_only = megara_with_codex_home(codex_home.path())
        .args([
            "doctor", "--scope", "project", "--target", "codex", "--json",
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(read_only.status.success());
    let read_only_stdout = String::from_utf8_lossy(&read_only.stdout);
    assert!(read_only_stdout.contains("\"ok\": false"));
    assert!(read_only_stdout.contains("pending Planning purge cleanup"));
    assert!(backup_root
        .symlink_metadata()
        .unwrap()
        .file_type()
        .is_symlink());

    let unresolved = doctor_json(dir.path(), codex_home.path(), true);
    assert_eq!(unresolved["ok"], false, "report={unresolved}");
    assert!(doctor_issue(&unresolved, "PURGE_RESIDUE"));
    assert!(doctor_warning(&unresolved, "PURGE_RESIDUE"));
    assert!(backup_root
        .symlink_metadata()
        .unwrap()
        .file_type()
        .is_symlink());

    fs::remove_file(&backup_root).unwrap();
    fs::rename(&held_root, &backup_root).unwrap();

    let repaired = megara_with_codex_home(codex_home.path())
        .args([
            "doctor", "--scope", "project", "--target", "codex", "--repair", "--json",
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        repaired.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&repaired.stdout),
        String::from_utf8_lossy(&repaired.stderr)
    );
    let repaired_stdout = String::from_utf8_lossy(&repaired.stdout);
    assert!(repaired_stdout.contains("\"warnings\": []"));
    assert!(!repaired_stdout.contains("pending Planning purge cleanup"));
    assert!(repaired_stdout.contains("repaired=1, pending=0"));
    assert!(!backup_root.exists());
    let store = super::planning::store::PlanningStore::open_project(dir.path()).unwrap();
    assert_eq!(store.pending_cleanup_count().unwrap(), 0);
}
