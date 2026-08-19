use std::{collections::BTreeSet, fs, path::PathBuf};

use anyhow::Result;
use serde::Serialize;

use crate::{
    cli::UninstallArgs,
    installer::{InstallAction, InstallOptions, Planner, MANAGED_MARKER},
    paths::{InstallPaths, InstallScope, TargetRuntime},
    targets::codex,
    templates::TemplateRegistry,
    ui::{self, Section},
    writer::remove_managed_files,
};

#[derive(Debug, Serialize)]
struct UninstallResult {
    scope: InstallScope,
    target: TargetRuntime,
    dry_run: bool,
    removed: Vec<PathBuf>,
    retained_runtime_data: bool,
}

pub fn run(args: UninstallArgs, registry: &TemplateRegistry) -> Result<()> {
    let scope: InstallScope = args.scope.into();
    let target: TargetRuntime = args.target.into();
    let plan = Planner::new(
        registry,
        InstallOptions {
            action: InstallAction::Install,
            scope,
            target,
            locale: None,
            dry_run: args.dry_run,
            force: false,
            trust_project: false,
            json: args.json,
        },
    )
    .plan_without_managed_edits()?;
    let managed_configs = if target == TargetRuntime::Codex {
        match scope {
            InstallScope::Project => codex::plan_remove_mcp_config(&plan.target_root, args.force)?
                .into_iter()
                .collect::<Vec<_>>(),
            InstallScope::Global => codex::plan_remove_global_config(&plan.target_root)?
                .into_iter()
                .collect::<Vec<_>>(),
        }
    } else {
        Vec::new()
    };
    let keep_shared_files = other_managed_projection_exists(scope, target, registry)?;
    let paths = if keep_shared_files {
        plan.files
            .iter()
            .filter(|file| file.path.starts_with(&plan.target_root))
            .map(|file| file.path.clone())
            .collect::<Vec<_>>()
    } else {
        plan.files.iter().map(|file| file.path.clone()).collect()
    };
    let paths = paths
        .into_iter()
        .chain(plan.obsolete_files)
        .chain(
            (target == TargetRuntime::Codex && scope == InstallScope::Global)
                .then(|| plan.target_root.join("config.toml")),
        )
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let planned_removed = remove_managed_files(&paths, true)?;
    let mut removed = if args.dry_run {
        planned_removed
    } else {
        for edit in &managed_configs {
            edit.apply(false)?;
        }
        remove_managed_files(&paths, false)?
    };
    for edit in &managed_configs {
        if edit.changed {
            removed.push(edit.path.clone());
        }
    }
    let runtime_paths = InstallPaths::resolve(scope, target)?;
    let retained_runtime_data = runtime_paths.runtime_root.join("state").exists()
        || runtime_paths.runtime_root.join("artifacts").exists()
        || runtime_paths.runtime_root.join("cache").exists()
        || runtime_paths.runtime_root.join("planning").exists();
    let result = UninstallResult {
        scope,
        target,
        dry_run: args.dry_run,
        removed,
        retained_runtime_data,
    };
    print(&result, keep_shared_files, args.json)
}

fn other_managed_projection_exists(
    scope: InstallScope,
    target: TargetRuntime,
    registry: &TemplateRegistry,
) -> Result<bool> {
    let target = other_target(target);
    let plan = Planner::new(
        registry,
        InstallOptions {
            action: InstallAction::Install,
            scope,
            target,
            locale: None,
            dry_run: true,
            force: false,
            trust_project: false,
            json: false,
        },
    )
    .plan_without_managed_edits()?;
    Ok(plan
        .files
        .iter()
        .map(|file| &file.path)
        .chain(plan.obsolete_files.iter())
        .filter(|path| path.starts_with(&plan.target_root))
        .any(is_managed_file))
}

fn is_managed_file(path: &PathBuf) -> bool {
    fs::read_to_string(path)
        .map(|content| content.contains(MANAGED_MARKER))
        .unwrap_or(false)
}

fn other_target(target: TargetRuntime) -> TargetRuntime {
    match target {
        TargetRuntime::Codex => TargetRuntime::Pi,
        TargetRuntime::Pi => TargetRuntime::Codex,
    }
}

fn print(result: &UninstallResult, kept_shared_files: bool, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(result)?);
        return Ok(());
    }
    let mut sections = vec![Section::new(
        "Run",
        vec![format!(
            "megara uninstall {}: scope={}, target={}, removed={}",
            if result.dry_run {
                "planned"
            } else {
                "complete"
            },
            result.scope,
            result.target,
            result.removed.len()
        )],
    )];
    if !result.removed.is_empty() {
        sections.push(Section::new("Removed", preview_paths(&result.removed)));
    }
    if kept_shared_files {
        sections.push(Section::new(
            "Retained",
            vec!["Shared Megara files remain because another runtime is installed.".to_string()],
        ));
    }
    if result.retained_runtime_data {
        sections.push(Section::new(
            "Retained",
            vec![
                "Runtime data remains for recovery and is never removed by uninstall.".to_string(),
            ],
        ));
    }
    ui::print_dashboard(
        "Uninstall",
        if result.dry_run {
            "planned"
        } else {
            "complete"
        },
        &[
            ("scope", result.scope.to_string()),
            ("target", result.target.to_string()),
        ],
        &sections,
    )
}

fn preview_paths(paths: &[PathBuf]) -> Vec<String> {
    const LIMIT: usize = 8;

    let mut preview = paths
        .iter()
        .take(LIMIT)
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    if paths.len() > preview.len() {
        preview.push(format!(
            "+{} more managed file(s)",
            paths.len() - preview.len()
        ));
    }
    preview
}
