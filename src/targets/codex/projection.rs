use std::{env, path::PathBuf};

use anyhow::{Context, Result};

use crate::{
    agents,
    installer::{ManagedTomlEdit, PlannedFile},
    paths::{InstallScope, TargetRuntime},
    templates::TemplateRegistry,
};

use super::{agent::agent_toml, agents_md::codex_agents_md, runtime::CodexRuntimeFeatures};

pub(super) fn projection_plan(
    root: PathBuf,
    scope: InstallScope,
    registry: &TemplateRegistry,
    force: bool,
    include_managed_edit: bool,
    runtime_features: &CodexRuntimeFeatures,
) -> Result<(Vec<PlannedFile>, Option<ManagedTomlEdit>)> {
    let megara_bin = env::current_exe()
        .context("failed to resolve current megara executable")?
        .canonicalize()
        .context("failed to canonicalize current megara executable")?;
    let managed_config = if include_managed_edit {
        match scope {
            InstallScope::Project => Some(super::mcp_config::plan(
                &root,
                &megara_bin,
                force,
                runtime_features.default_mode_request_user_input,
            )?),
            InstallScope::Global => super::global_config::plan(
                &root,
                runtime_features.default_mode_request_user_input,
                force,
            )?,
        }
    } else {
        None
    };
    let mut files = vec![PlannedFile::new(
        root.join("AGENTS.md"),
        codex_agents_md(registry)?,
    )];

    if scope == InstallScope::Global {
        for skill in registry.skills() {
            files.push(PlannedFile::new(
                root.join("skills").join(&skill.name).join("SKILL.md"),
                skill.content.clone(),
            ));
        }
    }
    for agent in registry.agents() {
        let policy = registry
            .config()
            .map(|config| {
                agents::effective_policy(scope, TargetRuntime::Codex, &agent.name, &config.content)
            })
            .transpose()?
            .unwrap_or_default();
        let (agent_id, agent_content) = agent_toml(agent, policy)?;
        files.push(PlannedFile::new(
            root.join("agents").join(format!("{agent_id}.toml")),
            agent_content,
        ));
    }

    Ok((files, managed_config))
}

pub(super) fn obsolete_projection_files(
    root: PathBuf,
    scope: InstallScope,
    _registry: &TemplateRegistry,
) -> Vec<PathBuf> {
    if scope != InstallScope::Project {
        return Vec::new();
    }
    let Some(project_root) = root.parent() else {
        return Vec::new();
    };
    crate::planning::migration::inventory::managed_projection_paths()
        .iter()
        .map(|relative| project_root.join(relative))
        .collect()
}
