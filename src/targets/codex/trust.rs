use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::Serialize;
use toml_edit::{value, DocumentMut, Item, Table, Value as EditValue};

use crate::installer::ManagedTomlEdit;

#[derive(Clone, Debug, Serialize)]
pub struct ProjectTrustSummary {
    pub config_path: PathBuf,
    pub project_root: PathBuf,
    pub registered: usize,
    pub unchanged: usize,
    pub skipped: bool,
}

pub(super) fn ensure_project_trust(
    project_root: &Path,
    dry_run: bool,
) -> Result<ProjectTrustSummary> {
    let (edit, project_root) = plan_project_trust(project_root)?;
    let changed = edit.changed;
    edit.apply(dry_run)?;
    Ok(ProjectTrustSummary {
        config_path: edit.path,
        project_root,
        registered: usize::from(changed && !dry_run),
        unchanged: usize::from(!changed && !dry_run),
        skipped: dry_run,
    })
}

pub(super) fn is_project_trusted(project_root: &Path) -> Result<bool> {
    let config_path = codex_home_dir()?.join("config.toml");
    let content = match fs::read_to_string(&config_path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to read Codex config {}", config_path.display()))
        }
    };
    let document: DocumentMut = content.parse().with_context(|| {
        format!(
            "failed to parse Codex config TOML: {}",
            config_path.display()
        )
    })?;
    let project_root = canonical_project_root(project_root);
    Ok(document
        .get("projects")
        .and_then(Item::as_table_like)
        .and_then(|projects| projects.get(&project_root.display().to_string()))
        .and_then(Item::as_table_like)
        .and_then(|project| project.get("trust_level"))
        .and_then(Item::as_value)
        .and_then(EditValue::as_str)
        == Some("trusted"))
}

fn plan_project_trust(project_root: &Path) -> Result<(ManagedTomlEdit, PathBuf)> {
    let config_path = codex_home_dir()?.join("config.toml");
    let project_root = canonical_project_root(project_root);
    let (source, existing, permissions) = match read_existing(&config_path)? {
        Some((source, existing, permissions)) => (Some(source), Some(existing), Some(permissions)),
        None => (None, None, None),
    };
    let mut document: DocumentMut = existing
        .as_deref()
        .unwrap_or_default()
        .parse()
        .with_context(|| {
            format!(
                "failed to parse Codex config TOML: {}",
                config_path.display()
            )
        })?;
    let projects = document["projects"].or_insert(Item::Table(Table::new()));
    let projects = projects
        .as_table_like_mut()
        .context("projects must be a TOML table")?;
    let project_key = project_root.display().to_string();
    let project = projects
        .entry(&project_key)
        .or_insert(Item::Table(Table::new()));
    let project = project
        .as_table_like_mut()
        .context("project trust settings must be a TOML table")?;
    project.insert("trust_level", value("trusted"));

    let desired = document.to_string();
    let changed = source
        .as_deref()
        .is_none_or(|source| source != desired.as_bytes());
    Ok((
        ManagedTomlEdit {
            path: config_path,
            created: source.is_none(),
            changed,
            backup_path: None,
            desired,
            backup: None,
            expected_source: source,
            permissions,
        },
        project_root,
    ))
}

fn canonical_project_root(project_root: &Path) -> PathBuf {
    fs::canonicalize(project_root).unwrap_or_else(|_| project_root.to_path_buf())
}

fn codex_home_dir() -> Result<PathBuf> {
    if let Some(value) = env::var_os("CODEX_HOME") {
        if !value.is_empty() {
            return Ok(PathBuf::from(value));
        }
    }
    crate::paths::home_dir().map(|home| home.join(".codex"))
}

fn read_existing(path: &Path) -> Result<Option<(Vec<u8>, String, fs::Permissions)>> {
    let source = match fs::read(path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to read Codex config {}", path.display()))
        }
    };
    let content = String::from_utf8(source.clone())
        .with_context(|| format!("Codex config is not valid UTF-8: {}", path.display()))?;
    let permissions = fs::metadata(path)
        .with_context(|| format!("failed to stat Codex config {}", path.display()))?
        .permissions();
    Ok(Some((source, content, permissions)))
}
