use super::*;

#[test]
fn installs_project_scope_codex_harness() {
    let dir = tempdir().unwrap();
    let codex_home = tempdir().unwrap();

    let output = megara_with_codex_home(codex_home.path())
        .args(["install", "--scope", "project", "--target", "codex"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Megara / Install"));
    assert!(!stdout.to_ascii_lowercase().contains("hook"));

    for path in [
        ".agents/megara.toml",
        ".agents/.gitignore",
        ".agents/bin/megara",
        ".agents/bin/insane-search",
        ".agents/skills/caveman/SKILL.md",
        ".agents/skills/insane-search/SKILL.md",
        ".agents/skills/agent-models/SKILL.md",
        ".agents/tools/insane-search/TOOL.md",
        ".agents/agents/executor.toml",
        ".agents/agents/researcher.toml",
        ".agents/agents/contrarian.toml",
        ".agents/agents/simplifier.toml",
        ".megara/.gitignore",
        ".codex/AGENTS.md",
        ".codex/config.toml",
        ".codex/agents/executor.toml",
        ".codex/agents/researcher.toml",
        ".codex/agents/contrarian.toml",
        ".codex/agents/simplifier.toml",
    ] {
        assert!(dir.path().join(path).exists(), "missing {path}");
    }
    for path in [
        ".codex/hooks.json",
        ".agents/skill-fragments",
        ".codex/skills",
        ".codex/skill-fragments",
    ] {
        assert!(!dir.path().join(path).exists(), "unexpected {path}");
    }

    let mut skills = fs::read_dir(dir.path().join(".agents/skills"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    skills.sort();
    assert_eq!(skills, ["agent-models", "caveman", "insane-search"]);

    let agents_gitignore = fs::read_to_string(dir.path().join(".agents/.gitignore")).unwrap();
    let runtime_gitignore = fs::read_to_string(dir.path().join(".megara/.gitignore")).unwrap();
    for ignored in [
        "state/",
        "artifacts/",
        "cache/",
        "planning/",
        "migration-backups/",
    ] {
        assert!(
            agents_gitignore.contains(ignored),
            "missing {ignored} in SSOT ignore"
        );
        assert!(
            runtime_gitignore.contains(ignored),
            "missing {ignored} in runtime ignore"
        );
    }

    let agents_md = fs::read_to_string(dir.path().join(".codex/AGENTS.md")).unwrap();
    assert!(agents_md.contains("## Planning Adapter"));
    assert!(agents_md.contains(".megara/planning"));
    assert!(!agents_md.contains("hook"));

    let megara_config = fs::read_to_string(dir.path().join(".agents/megara.toml")).unwrap();
    assert!(megara_config.contains("default_active_skills = [\"caveman\"]"));
}

#[cfg(unix)]
#[test]
fn codex_projection_uses_native_questions_only_when_runtime_advertises_them() {
    let supported_project = tempdir().unwrap();
    let supported_codex_home = tempdir().unwrap();
    let supported_runtime = tempdir().unwrap();
    write_codex_runtime(supported_runtime.path(), true);

    let install = megara_with_codex_home(supported_codex_home.path())
        .args(["install", "--scope", "project", "--target", "codex"])
        .env("PATH", supported_runtime.path())
        .current_dir(supported_project.path())
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&install.stderr)
    );
    let supported_config =
        fs::read_to_string(supported_project.path().join(".codex/config.toml")).unwrap();
    assert!(supported_config.contains("default_mode_request_user_input = true"));
    assert!(supported_config.contains("MEGARA:DEFAULT-MODE-REQUEST-USER-INPUT"));
    let supported_agents =
        fs::read_to_string(supported_project.path().join(".codex/AGENTS.md")).unwrap();
    assert!(supported_agents
        .contains("only when `default_mode_request_user_input` is enabled by this projection"));
    assert!(String::from_utf8_lossy(&install.stdout).contains("request_user_input=available"));

    let fallback_project = tempdir().unwrap();
    let fallback_codex_home = tempdir().unwrap();
    let fallback_runtime = tempdir().unwrap();
    write_codex_runtime(fallback_runtime.path(), false);
    let install = megara_with_codex_home(fallback_codex_home.path())
        .args(["install", "--scope", "project", "--target", "codex"])
        .env("PATH", fallback_runtime.path())
        .current_dir(fallback_project.path())
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&install.stderr)
    );
    let fallback_config =
        fs::read_to_string(fallback_project.path().join(".codex/config.toml")).unwrap();
    assert!(!fallback_config.contains("default_mode_request_user_input"));
    let fallback_agents =
        fs::read_to_string(fallback_project.path().join(".codex/AGENTS.md")).unwrap();
    assert!(fallback_agents.contains("If the feature is unavailable"));
    assert!(
        String::from_utf8_lossy(&install.stdout).contains("request_user_input=Markdown fallback")
    );

    let sync = megara_with_codex_home(fallback_codex_home.path())
        .args(["sync", "--scope", "project", "--target", "codex"])
        .env("PATH", supported_runtime.path())
        .current_dir(fallback_project.path())
        .output()
        .unwrap();
    assert!(
        sync.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&sync.stdout),
        String::from_utf8_lossy(&sync.stderr)
    );
    let synced_config =
        fs::read_to_string(fallback_project.path().join(".codex/config.toml")).unwrap();
    assert!(synced_config.contains("default_mode_request_user_input = true"));
    assert!(synced_config.contains("MEGARA:DEFAULT-MODE-REQUEST-USER-INPUT"));

    let downgrade = megara_with_codex_home(supported_codex_home.path())
        .args(["sync", "--scope", "project", "--target", "codex"])
        .env("PATH", fallback_runtime.path())
        .current_dir(supported_project.path())
        .output()
        .unwrap();
    assert!(
        downgrade.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&downgrade.stdout),
        String::from_utf8_lossy(&downgrade.stderr)
    );
    let downgraded_config =
        fs::read_to_string(supported_project.path().join(".codex/config.toml")).unwrap();
    assert!(!downgraded_config.contains("default_mode_request_user_input"));
    assert!(!downgraded_config.contains("MEGARA:DEFAULT-MODE-REQUEST-USER-INPUT"));
}

#[cfg(unix)]
#[test]
fn project_install_preserves_an_explicit_user_feature_disable() {
    let project = tempdir().unwrap();
    let codex_home = tempdir().unwrap();
    let runtime = tempdir().unwrap();
    write_codex_runtime(runtime.path(), true);
    let config = project.path().join(".codex/config.toml");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(
        &config,
        "[features]\ndefault_mode_request_user_input = false\n",
    )
    .unwrap();

    let install = megara_with_codex_home(codex_home.path())
        .args(["install", "--scope", "project", "--target", "codex"])
        .env("PATH", runtime.path())
        .current_dir(project.path())
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&install.stderr)
    );
    let updated = fs::read_to_string(config).unwrap();
    assert!(updated.contains("default_mode_request_user_input = false"));
    assert!(!updated.contains("MEGARA:DEFAULT-MODE-REQUEST-USER-INPUT"));
}
#[test]
fn install_migrates_legacy_project_runtime_state() {
    let dir = tempdir().unwrap();
    let codex_home = tempdir().unwrap();
    let legacy_state = dir.path().join(".agents/state/legacy-session.json");
    fs::create_dir_all(legacy_state.parent().unwrap()).unwrap();
    fs::write(&legacy_state, r#"{"session_id":"legacy-session"}"#).unwrap();

    let output = megara_with_codex_home(codex_home.path())
        .arg("install")
        .arg("--scope")
        .arg("project")
        .arg("--target")
        .arg("codex")
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Migration"));
    assert!(stdout.contains("moved=1"));
    assert!(!dir.path().join(".agents/state").exists());
    assert!(dir
        .path()
        .join(".megara/state/legacy-session.json")
        .exists());
}

#[test]
fn install_project_scope_honors_locale_arg() {
    let dir = tempdir().unwrap();
    let codex_home = tempdir().unwrap();

    let output = megara_with_codex_home(codex_home.path())
        .arg("install")
        .arg("--scope")
        .arg("project")
        .arg("--target")
        .arg("codex")
        .arg("--locale")
        .arg("en-US")
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let megara_config = fs::read_to_string(dir.path().join(".agents/megara.toml")).unwrap();
    assert!(megara_config.contains("locale = \"en-US\""));
    let agents_md = fs::read_to_string(dir.path().join(".codex/AGENTS.md")).unwrap();
    assert!(agents_md.contains("Locale: `en-US`"));
}

#[test]
fn install_keeps_conflicting_legacy_runtime_state_in_place() {
    let dir = tempdir().unwrap();
    let codex_home = tempdir().unwrap();
    let legacy_state = dir.path().join(".agents/state/session.json");
    let migrated_state = dir.path().join(".megara/state/session.json");
    fs::create_dir_all(legacy_state.parent().unwrap()).unwrap();
    fs::create_dir_all(migrated_state.parent().unwrap()).unwrap();
    fs::write(&legacy_state, r#"{"source":"legacy"}"#).unwrap();
    fs::write(&migrated_state, r#"{"source":"current"}"#).unwrap();

    let output = megara_with_codex_home(codex_home.path())
        .arg("install")
        .arg("--scope")
        .arg("project")
        .arg("--target")
        .arg("codex")
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("conflicts=1"));
    assert!(stdout.contains("legacy runtime state migration left 1 conflicting file"));
    assert!(legacy_state.exists());
    assert_eq!(
        fs::read_to_string(migrated_state).unwrap(),
        r#"{"source":"current"}"#
    );
}
