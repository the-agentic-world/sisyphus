use std::{env, fs, path::PathBuf};

use anyhow::{bail, Result};
use serde::Serialize;

use crate::{
    cli::{resolve_scope, resolve_target, DoctorArgs, InstallArgs, ScopeArg, SyncArgs},
    paths::{InstallPaths, InstallScope, TargetRuntime},
    targets::codex,
    writer::WriteSummary,
};

use super::marker::{add_marker, add_shell_marker};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallAction {
    Install,
    Sync,
}

#[derive(Clone, Debug, Serialize)]
pub struct InstallOptions {
    pub action: InstallAction,
    pub scope: InstallScope,
    pub target: TargetRuntime,
    pub locale: Option<String>,
    pub dry_run: bool,
    pub force: bool,
    pub trust_project: bool,
    pub json: bool,
}

impl InstallOptions {
    pub fn resolve(args: InstallArgs, interactive: bool, action: InstallAction) -> Result<Self> {
        Ok(Self {
            action,
            scope: resolve_scope(args.scope, interactive)?,
            target: resolve_target(args.target, interactive)?,
            locale: normalize_locale(args.locale)?,
            dry_run: args.dry_run,
            force: args.force,
            trust_project: args.trust_project,
            json: args.json,
        })
    }

    pub fn resolve_sync(args: SyncArgs) -> Result<Vec<Self>> {
        let scope = resolve_sync_scope(args.scope)?;
        let targets = match args.target {
            Some(target) => vec![target.into()],
            None => detected_sync_targets(scope)?,
        };
        if targets.is_empty() {
            bail!(
                "no managed runtime projection found; run megara install --scope {} --target <codex|pi> first",
                scope
            );
        }
        Ok(targets
            .into_iter()
            .map(|target| Self {
                action: InstallAction::Sync,
                scope,
                target,
                locale: None,
                dry_run: args.dry_run,
                force: args.force,
                trust_project: false,
                json: args.json,
            })
            .collect())
    }
}

fn resolve_sync_scope(scope: Option<ScopeArg>) -> Result<InstallScope> {
    if let Some(scope) = scope {
        return Ok(scope.into());
    }
    let cwd = env::current_dir()?;
    if cwd.join(".agents/megara.toml").exists() {
        return Ok(InstallScope::Project);
    }
    if crate::paths::home_dir()?
        .join(".megara/megara.toml")
        .exists()
    {
        return Ok(InstallScope::Global);
    }
    bail!("no Megara SSOT found; specify --scope or run megara install first")
}

fn detected_sync_targets(scope: InstallScope) -> Result<Vec<TargetRuntime>> {
    let mut targets = Vec::new();
    for target in [TargetRuntime::Codex, TargetRuntime::Pi] {
        if managed_projection_exists(scope, target)? {
            targets.push(target);
        }
    }
    Ok(targets)
}

fn managed_projection_exists(scope: InstallScope, target: TargetRuntime) -> Result<bool> {
    let paths = InstallPaths::resolve(scope, target)?;
    let marker_paths = match target {
        TargetRuntime::Codex => vec![
            paths.target_root.join("AGENTS.md"),
            paths.target_root.join("agents/executor.toml"),
        ],
        TargetRuntime::Pi => vec![
            paths.target_root.join("extensions/megara.ts"),
            paths.target_root.join("agents/executor.md"),
        ],
    };
    Ok(marker_paths.into_iter().any(|path| {
        fs::read_to_string(path)
            .map(|content| content.contains(crate::installer::MANAGED_MARKER))
            .unwrap_or(false)
    }))
}

fn normalize_locale(locale: Option<String>) -> Result<Option<String>> {
    let Some(locale) = locale else {
        return Ok(None);
    };
    let locale = locale.trim().to_string();
    if locale.is_empty() {
        bail!("--locale must not be empty");
    }
    if locale.chars().any(char::is_control) {
        bail!("--locale must not contain control characters");
    }
    Ok(Some(locale))
}

#[derive(Clone, Debug, Serialize)]
pub struct DoctorOptions {
    pub scope: InstallScope,
    pub target: TargetRuntime,
    pub json: bool,
    pub repair: bool,
}

impl DoctorArgs {
    pub fn resolve(self) -> Result<DoctorOptions> {
        Ok(DoctorOptions {
            scope: resolve_scope(self.scope, false)?,
            target: resolve_target(self.target, false)?,
            json: self.json,
            repair: self.repair,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlannedFile {
    pub path: PathBuf,
    pub content: String,
    pub executable: bool,
}

impl PlannedFile {
    pub fn new(path: PathBuf, content: impl Into<String>) -> Self {
        let content = add_marker(&path, content.into());
        Self {
            path,
            content,
            executable: false,
        }
    }

    pub fn new_executable_shell(path: PathBuf, content: impl Into<String>) -> Self {
        let content = add_shell_marker(content.into());
        Self {
            path,
            content,
            executable: true,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct InstallPlan {
    pub scope: InstallScope,
    pub target: TargetRuntime,
    pub ssot_root: PathBuf,
    pub runtime_root: PathBuf,
    pub target_root: PathBuf,
    pub codex_runtime_features: Option<codex::CodexRuntimeFeatures>,
    pub files: Vec<PlannedFile>,
    pub managed_toml_edits: Vec<super::managed_edit::ManagedTomlEdit>,
    pub obsolete_files: Vec<PathBuf>,
    #[serde(skip)]
    pub obsolete_managed_files: Vec<PlannedFile>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct StateMigrationSummary {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub moved: Vec<PathBuf>,
    pub conflicts: Vec<PathBuf>,
    pub removed_source: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct InstallResult {
    pub options: InstallOptions,
    pub plan: InstallPlan,
    pub summary: WriteSummary,
    pub migrations: Vec<StateMigrationSummary>,
    pub project_trust: Option<codex::ProjectTrustSummary>,
    pub warnings: Vec<String>,
}
