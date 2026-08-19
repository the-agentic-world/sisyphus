use super::*;

#[test]
fn project_install_trusts_codex_only_when_requested() {
    let project = tempdir().unwrap();
    let codex_home = tempdir().unwrap();

    let untrusted = megara_with_codex_home(codex_home.path())
        .args(["install", "--scope", "project", "--target", "codex"])
        .current_dir(project.path())
        .output()
        .unwrap();
    assert!(
        untrusted.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&untrusted.stderr)
    );
    assert!(
        String::from_utf8_lossy(&untrusted.stdout)
            .contains("Codex project config remains inactive"),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&untrusted.stdout),
        String::from_utf8_lossy(&untrusted.stderr)
    );
    let global_config = codex_home.path().join("config.toml");
    assert!(!global_config.exists());

    let doctor = megara_with_codex_home(codex_home.path())
        .args([
            "doctor", "--scope", "project", "--target", "codex", "--json",
        ])
        .current_dir(project.path())
        .output()
        .unwrap();
    assert!(doctor.status.success());
    let report: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
    assert_eq!(report["ok"], false);
    assert!(report["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|warning| warning.as_str())
        .any(|warning| warning.contains("not explicitly trusted")));

    let trusted = megara_with_codex_home(codex_home.path())
        .args([
            "install",
            "--scope",
            "project",
            "--target",
            "codex",
            "--trust-project",
        ])
        .current_dir(project.path())
        .output()
        .unwrap();
    assert!(
        trusted.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&trusted.stderr)
    );
    let content = fs::read_to_string(&global_config).unwrap();
    let parsed: toml::Value = content.parse().unwrap();
    let project_root = fs::canonicalize(project.path()).unwrap();
    assert_eq!(
        parsed["projects"][project_root.to_str().unwrap()]["trust_level"].as_str(),
        Some("trusted")
    );
    assert!(String::from_utf8_lossy(&trusted.stdout).contains("Codex project trust: registered=1"));

    let sync = megara_with_codex_home(codex_home.path())
        .args(["sync", "--scope", "project", "--target", "codex"])
        .current_dir(project.path())
        .output()
        .unwrap();
    assert!(sync.status.success());
    assert!(
        !String::from_utf8_lossy(&sync.stdout).contains("Codex project config remains inactive")
    );
}
