use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::{
    installer::{ManagedTomlEdit, PlannedFile},
    paths::InstallScope,
    templates::TemplateRegistry,
};

#[path = "codex/agent.rs"]
mod agent;
#[path = "codex/agents_md.rs"]
mod agents_md;
#[path = "codex/config.rs"]
mod config;
#[path = "codex/global_config.rs"]
mod global_config;
#[path = "codex/mcp_config.rs"]
mod mcp_config;
#[path = "codex/projection.rs"]
mod projection;
#[path = "codex/runtime.rs"]
mod runtime;
#[path = "codex/trust.rs"]
mod trust;
const DEFAULT_LOCALE: &str = "ko-KR";

pub use runtime::CodexRuntimeFeatures;
pub use trust::ProjectTrustSummary;

pub fn projection_files(
    root: PathBuf,
    scope: InstallScope,
    registry: &TemplateRegistry,
    runtime_features: &CodexRuntimeFeatures,
) -> Result<Vec<PlannedFile>> {
    projection::projection_plan(root, scope, registry, false, false, runtime_features)
        .map(|(files, _)| files)
}

pub fn projection_files_with_force(
    root: PathBuf,
    scope: InstallScope,
    registry: &TemplateRegistry,
    force: bool,
    runtime_features: &CodexRuntimeFeatures,
) -> Result<Vec<PlannedFile>> {
    projection::projection_plan(root, scope, registry, force, false, runtime_features)
        .map(|(files, _)| files)
}

pub(crate) fn projection_plan_with_force(
    root: PathBuf,
    scope: InstallScope,
    registry: &TemplateRegistry,
    force: bool,
    runtime_features: &CodexRuntimeFeatures,
) -> Result<(Vec<PlannedFile>, Option<ManagedTomlEdit>)> {
    projection::projection_plan(root, scope, registry, force, true, runtime_features)
}

pub fn detect_runtime_features() -> CodexRuntimeFeatures {
    runtime::detect_runtime_features()
}

pub fn ensure_project_trust(project_root: &Path, dry_run: bool) -> Result<ProjectTrustSummary> {
    trust::ensure_project_trust(project_root, dry_run)
}

pub fn is_project_trusted(project_root: &Path) -> Result<bool> {
    trust::is_project_trusted(project_root)
}

pub(crate) fn plan_remove_mcp_config(root: &Path, force: bool) -> Result<Option<ManagedTomlEdit>> {
    mcp_config::plan_remove(root, force)
}

pub(crate) fn plan_remove_global_config(root: &Path) -> Result<Option<ManagedTomlEdit>> {
    global_config::plan_remove(root)
}

pub fn obsolete_projection_files(
    root: PathBuf,
    scope: InstallScope,
    registry: &TemplateRegistry,
) -> Vec<PathBuf> {
    projection::obsolete_projection_files(root, scope, registry)
}

pub fn runtime_dependency_issues() -> Vec<String> {
    Vec::new()
}
