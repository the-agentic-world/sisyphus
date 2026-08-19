use super::*;

#[test]
fn installs_global_scope_codex_harness() {
    let cwd = tempdir().unwrap();
    let home = tempdir().unwrap();
    let codex_home = home.path().join(".codex");

    let output = megara_with_codex_home(&codex_home)
        .arg("install")
        .arg("--scope")
        .arg("global")
        .arg("--target")
        .arg("codex")
        .env("HOME", home.path())
        .current_dir(cwd.path())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(home.path().join(".megara/megara.toml").exists());
    assert!(home.path().join(".megara/.gitignore").exists());
    assert!(home.path().join(".megara/bin/megara").exists());
    assert!(home.path().join(".megara/bin/insane-search").exists());
    let wrapper = fs::read_to_string(home.path().join(".megara/bin/insane-search")).unwrap();
    assert!(wrapper.contains("state/tools/insane-search"));
    assert!(wrapper.contains("Python 3.10 or newer"));
    assert!(wrapper.contains("root_dir=$(CDPATH= cd \"$bin_dir/..\" && pwd -P)"));
    assert!(home
        .path()
        .join(".megara/tools/insane-search/TOOL.md")
        .exists());
    assert!(home
        .path()
        .join(".megara/skills/insane-search/SKILL.md")
        .exists());
    assert!(home
        .path()
        .join(".codex/skills/insane-search/SKILL.md")
        .exists());
    assert!(home.path().join(".codex/AGENTS.md").exists());
    let agents_md = fs::read_to_string(home.path().join(".codex/AGENTS.md")).unwrap();
    assert!(agents_md.contains("~/.megara/bin/<tool-name>"));
}

#[cfg(unix)]
#[test]
fn global_install_enables_and_removes_native_question_feature_with_runtime_support() {
    let cwd = tempdir().unwrap();
    let home = tempdir().unwrap();
    let codex_home = home.path().join(".codex");
    let supported_runtime = tempdir().unwrap();
    let fallback_runtime = tempdir().unwrap();
    write_codex_runtime(supported_runtime.path(), true);
    write_codex_runtime(fallback_runtime.path(), false);

    let install = megara_with_codex_home(&codex_home)
        .args(["install", "--scope", "global", "--target", "codex"])
        .env("HOME", home.path())
        .env("PATH", supported_runtime.path())
        .current_dir(cwd.path())
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&install.stderr)
    );
    let config = codex_home.join("config.toml");
    let supported_config = fs::read_to_string(&config).unwrap();
    assert!(supported_config.contains("default_mode_request_user_input = true"));
    assert!(supported_config.contains("MEGARA:DEFAULT-MODE-REQUEST-USER-INPUT"));

    let fallback = megara_with_codex_home(&codex_home)
        .args(["install", "--scope", "global", "--target", "codex"])
        .env("HOME", home.path())
        .env("PATH", fallback_runtime.path())
        .current_dir(cwd.path())
        .output()
        .unwrap();
    assert!(
        fallback.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&fallback.stderr)
    );
    let fallback_config = fs::read_to_string(config).unwrap();
    assert!(!fallback_config.contains("default_mode_request_user_input"));
    assert!(!fallback_config.contains("MEGARA:DEFAULT-MODE-REQUEST-USER-INPUT"));

    let uninstall = megara_with_codex_home(&codex_home)
        .args(["uninstall", "--scope", "global", "--target", "codex"])
        .env("HOME", home.path())
        .env("PATH", fallback_runtime.path())
        .current_dir(cwd.path())
        .output()
        .unwrap();
    assert!(
        uninstall.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&uninstall.stderr)
    );
    assert!(!codex_home.join("config.toml").exists());
}

#[cfg(unix)]
#[test]
fn global_install_protects_an_unmanaged_codex_config_until_forced() {
    let cwd = tempdir().unwrap();
    let home = tempdir().unwrap();
    let codex_home = home.path().join(".codex");
    let runtime = tempdir().unwrap();
    write_codex_runtime(runtime.path(), true);
    fs::create_dir_all(&codex_home).unwrap();
    let config = codex_home.join("config.toml");
    let original = "model = \"gpt-5\"\n";
    fs::write(&config, original).unwrap();

    let protected = megara_with_codex_home(&codex_home)
        .args(["install", "--scope", "global", "--target", "codex"])
        .env("HOME", home.path())
        .env("PATH", runtime.path())
        .current_dir(cwd.path())
        .output()
        .unwrap();
    assert!(!protected.status.success());
    assert!(String::from_utf8_lossy(&protected.stderr).contains("unmanaged Codex config"));
    assert_eq!(fs::read_to_string(&config).unwrap(), original);
    assert!(!codex_home.join("AGENTS.md").exists());

    let forced = megara_with_codex_home(&codex_home)
        .args([
            "install", "--scope", "global", "--target", "codex", "--force",
        ])
        .env("HOME", home.path())
        .env("PATH", runtime.path())
        .current_dir(cwd.path())
        .output()
        .unwrap();
    assert!(
        forced.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&forced.stderr)
    );
    let updated = fs::read_to_string(&config).unwrap();
    assert!(updated.contains("model = \"gpt-5\""));
    assert!(updated.contains("default_mode_request_user_input = true"));
    assert!(updated.contains("MEGARA:DEFAULT-MODE-REQUEST-USER-INPUT"));
}

#[cfg(unix)]
#[test]
fn global_install_preserves_an_explicit_user_feature_disable() {
    let cwd = tempdir().unwrap();
    let home = tempdir().unwrap();
    let codex_home = home.path().join(".codex");
    let runtime = tempdir().unwrap();
    write_codex_runtime(runtime.path(), true);
    fs::create_dir_all(&codex_home).unwrap();
    let config = codex_home.join("config.toml");
    let original = "[features]\ndefault_mode_request_user_input = false\n";
    fs::write(&config, original).unwrap();

    let install = megara_with_codex_home(&codex_home)
        .args(["install", "--scope", "global", "--target", "codex"])
        .env("HOME", home.path())
        .env("PATH", runtime.path())
        .current_dir(cwd.path())
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&install.stderr)
    );
    assert_eq!(fs::read_to_string(config).unwrap(), original);
}
