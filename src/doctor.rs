use std::{fs, path::Path, process::Command};

use crate::{
    installer::{runtime_support_files, DoctorOptions, PlannedFile, MANAGED_MARKER},
    paths::{InstallPaths, TargetRuntime},
    planning::{
        service::{inspect_candidate_projection, repair_candidate_projection, ProjectionStatus},
        store::PlanningStore,
    },
    targets::{codex, pi},
    templates::TemplateRegistry,
    ui::{self, Section},
    writer::write_files,
};
use anyhow::Result;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct DoctorIssue {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub repairable: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct DoctorReport {
    pub scope: String,
    pub target: String,
    pub ok: bool,
    pub missing: Vec<String>,
    pub unmanaged: Vec<String>,
    pub stale: Vec<String>,
    pub warnings: Vec<String>,
    pub observations: Vec<String>,
    pub issues: Vec<DoctorIssue>,
    #[serde(skip)]
    pub json: bool,
}

pub fn run(_registry: &TemplateRegistry, options: DoctorOptions) -> Result<DoctorReport> {
    let paths = InstallPaths::resolve(options.scope, options.target)?;
    let mut missing = Vec::new();
    let mut unmanaged = Vec::new();
    let mut stale = Vec::new();
    let mut warnings = runtime_dependency_issues(options.target);
    let mut observations = Vec::new();
    let mut issues = Vec::new();

    for path in TemplateRegistry::missing_paths(&paths.ssot_root) {
        let path = path.display().to_string();
        missing.push(path.clone());
        issues.push(projection_issue("PROJECTION_MISSING", path, false));
    }

    if missing.is_empty() {
        for file in runtime_support_files(paths.ssot_root.clone(), paths.runtime_root.clone())? {
            inspect_managed_file(
                &file,
                &mut missing,
                &mut unmanaged,
                &mut stale,
                &mut issues,
                options.repair,
                &mut observations,
            )?;
            if file
                .path
                .file_name()
                .and_then(|file_name| file_name.to_str())
                == Some("megara")
            {
                inspect_wrapper_invocation(&file.path, &mut warnings);
            }
        }

        let ssot_registry = TemplateRegistry::from_ssot_root(&paths.ssot_root)?;
        let codex_runtime_features =
            (options.target == TargetRuntime::Codex).then(codex::detect_runtime_features);
        let projection_files = match options.target {
            TargetRuntime::Codex => codex::projection_files(
                paths.target_root.clone(),
                options.scope,
                &ssot_registry,
                codex_runtime_features
                    .as_ref()
                    .expect("Codex projection has runtime features"),
            )?,
            TargetRuntime::Pi => {
                pi::projection_files(paths.target_root.clone(), options.scope, &ssot_registry)?
            }
        };

        for file in projection_files {
            inspect_managed_file(
                &file,
                &mut missing,
                &mut unmanaged,
                &mut stale,
                &mut issues,
                options.repair,
                &mut observations,
            )?;
        }
        if options.target == TargetRuntime::Pi
            && options.scope == crate::paths::InstallScope::Project
        {
            pi::inspect_trust(
                &paths.runtime_root,
                paths
                    .target_root
                    .parent()
                    .expect("project Pi target root has a parent"),
                &ssot_registry,
                &mut warnings,
            )?;
        }
        if options.target == TargetRuntime::Codex
            && options.scope == crate::paths::InstallScope::Project
            && !codex::is_project_trusted(
                paths
                    .target_root
                    .parent()
                    .expect("project Codex target root has a parent"),
            )?
        {
            warnings.push(
                "Codex project is not explicitly trusted; its .codex config may stay inactive. Rerun install with --trust-project after reviewing the project."
                    .to_string(),
            );
        }
        if let Some(runtime_features) = &codex_runtime_features {
            observations.push(format!(
                "Codex runtime version: {}; request_user_input={}",
                runtime_features
                    .version
                    .as_deref()
                    .unwrap_or("not detected"),
                if runtime_features.default_mode_request_user_input {
                    "available"
                } else {
                    "Markdown fallback"
                }
            ));
        }
    }

    inspect_planning_health(
        options.scope,
        &paths.runtime_root,
        options.repair,
        &mut warnings,
        &mut observations,
        &mut issues,
    )?;

    Ok(DoctorReport {
        scope: options.scope.to_string(),
        target: options.target.to_string(),
        ok: missing.is_empty()
            && unmanaged.is_empty()
            && stale.is_empty()
            && warnings.is_empty()
            && issues.is_empty(),
        missing,
        unmanaged,
        stale,
        warnings,
        observations,
        issues,
        json: options.json,
    })
}

fn inspect_planning_health(
    scope: crate::paths::InstallScope,
    runtime_root: &Path,
    repair: bool,
    warnings: &mut Vec<String>,
    observations: &mut Vec<String>,
    issues: &mut Vec<DoctorIssue>,
) -> Result<()> {
    if scope != crate::paths::InstallScope::Project {
        return Ok(());
    }
    let Some(project_root) = runtime_root.parent() else {
        return Ok(());
    };
    let opened = if repair {
        PlanningStore::open_existing_project_for_repair(project_root)
    } else {
        PlanningStore::open_existing_project(project_root)
    };
    let mut store = match opened {
        Ok(store) => store,
        Err(error) => {
            let message = format!("Planning DB_CORRUPT: {error}");
            warnings.push(message.clone());
            issues.push(DoctorIssue {
                code: "DB_CORRUPT",
                message,
                path: Some(
                    project_root
                        .join(".megara/planning/planning.db")
                        .display()
                        .to_string(),
                ),
                repairable: false,
            });
            return Ok(());
        }
    };
    let Some(mut store) = store.take() else {
        return Ok(());
    };
    let pending_before = store.pending_cleanup_count()?;
    if pending_before > 0 && !repair {
        let message = format!(
            "pending Planning purge cleanup: {pending_before}; run `megara doctor --repair`"
        );
        warnings.push(message.clone());
        issues.push(DoctorIssue {
            code: "PURGE_RESIDUE",
            message,
            path: None,
            repairable: true,
        });
    }

    let inspection = match store.inspect_health() {
        Ok(inspection) => inspection,
        Err(error) => {
            let message = format!("Planning DB_CORRUPT: {error}");
            warnings.push(message.clone());
            issues.push(DoctorIssue {
                code: "DB_CORRUPT",
                message,
                path: Some(
                    project_root
                        .join(".megara/planning/planning.db")
                        .display()
                        .to_string(),
                ),
                repairable: false,
            });
            return Ok(());
        }
    };
    for issue in &inspection.issues {
        if repair && issue.repairable {
            continue;
        }
        if issue.code == "PURGE_RESIDUE" && pending_before > 0 {
            continue;
        }
        warnings.push(format!(
            "Planning {}: {}{}",
            issue.code,
            issue.message,
            if issue.repairable {
                "; run `megara doctor --repair`"
            } else {
                ""
            }
        ));
        issues.push(DoctorIssue {
            code: issue.code,
            message: issue.message.clone(),
            path: None,
            repairable: issue.repairable,
        });
    }
    if !inspection.tombstones.is_empty() {
        let pending_tombstones = inspection
            .tombstones
            .iter()
            .filter(|tombstone| tombstone.cleanup_state == "pending")
            .count();
        let residue_tombstones = inspection
            .tombstones
            .iter()
            .filter(|tombstone| {
                tombstone.artifact_residue
                    || tombstone.backup_residue
                    || tombstone.pending_backup_id.is_some()
            })
            .count();
        let sample = inspection
            .tombstones
            .first()
            .map(|tombstone| tombstone.session_id.as_str())
            .unwrap_or("none");
        observations.push(format!(
            "Planning tombstones inspected: count={}, pending={}, residue={}, sample_session={sample}",
            inspection.tombstones.len(),
            pending_tombstones,
            residue_tombstones,
        ));
    }

    if repair {
        let mut cache_repairs = 0;
        for state in &inspection.cache_repairs {
            match store.repair_cached_state(state) {
                Ok(()) => cache_repairs += 1,
                Err(error) => warnings.push(format!(
                    "Planning PROJECTION_DIVERGED repair failed for {}: {error}",
                    state.session_id
                )),
            }
        }
        if cache_repairs > 0 {
            observations.push(format!(
                "Planning replay cache repair: repaired={cache_repairs}, events unchanged"
            ));
        }
    }

    for state in &inspection.replayed_states {
        inspect_projections(project_root, state, repair, warnings, observations, issues);
    }

    if repair {
        let repaired = store.repair_pending_cleanup()?;
        let pending_after = store.pending_cleanup_count()?;
        observations.push(format!(
            "Planning purge cleanup repair: repaired={repaired}, pending={pending_after}"
        ));
        if pending_after > 0 {
            warnings.push(format!(
                "pending Planning purge cleanup remains: {pending_after}; retry `megara doctor --repair`"
            ));
        }
        let post_repair = match store.inspect_health() {
            Ok(inspection) => inspection,
            Err(error) => {
                warnings.push(format!("Planning DB_CORRUPT after repair: {error}"));
                return Ok(());
            }
        };
        for issue in post_repair.issues {
            if !issue.repairable {
                continue;
            }
            warnings.push(format!("Planning {}: {}", issue.code, issue.message));
            issues.push(DoctorIssue {
                code: issue.code,
                message: issue.message,
                path: None,
                repairable: true,
            });
        }
    }
    Ok(())
}

fn inspect_projections(
    project_root: &Path,
    state: &crate::planning::domain::PlanningState,
    repair: bool,
    warnings: &mut Vec<String>,
    observations: &mut Vec<String>,
    issues: &mut Vec<DoctorIssue>,
) {
    inspect_one_projection(
        project_root,
        &state.session_id,
        "spec",
        state.spec.current_candidate.as_ref(),
        repair,
        warnings,
        observations,
        issues,
    );
    inspect_one_projection(
        project_root,
        &state.session_id,
        "plan",
        state.plan.current_candidate.as_ref(),
        repair,
        warnings,
        observations,
        issues,
    );
}

#[allow(clippy::too_many_arguments)]
fn inspect_one_projection<T: serde::Serialize>(
    project_root: &Path,
    session_id: &str,
    kind: &str,
    candidate: Option<&T>,
    repair: bool,
    warnings: &mut Vec<String>,
    observations: &mut Vec<String>,
    issues: &mut Vec<DoctorIssue>,
) {
    let Some(candidate) = candidate else {
        return;
    };
    let candidate = match serde_json::to_value(candidate) {
        Ok(candidate) => candidate,
        Err(error) => {
            warnings.push(format!(
                "Planning PROJECTION_IO: cannot encode {kind} candidate for {session_id}: {error}"
            ));
            issues.push(DoctorIssue {
                code: "PROJECTION_IO",
                message: format!("cannot encode {kind} candidate for {session_id}: {error}"),
                path: None,
                repairable: false,
            });
            return;
        }
    };
    let status = inspect_candidate_projection(project_root, session_id, kind, &candidate);
    match status {
        ProjectionStatus::Unchanged => {}
        ProjectionStatus::Missing | ProjectionStatus::Stale if repair => {
            let repaired_status =
                repair_candidate_projection(project_root, session_id, kind, &candidate);
            if matches!(
                repaired_status,
                ProjectionStatus::Written | ProjectionStatus::Unchanged
            ) {
                observations.push(format!(
                    "Planning projection repair: session={session_id}, kind={kind}, status={}",
                    repaired_status.as_str()
                ));
            } else {
                let message = format!(
                    "Planning PROJECTION_{}: session={session_id}, kind={kind}, repair_status={}",
                    repaired_status.as_str().to_ascii_uppercase(),
                    repaired_status.as_str()
                );
                warnings.push(message.clone());
                issues.push(DoctorIssue {
                    code: projection_code(repaired_status),
                    message,
                    path: None,
                    repairable: false,
                });
            }
        }
        ProjectionStatus::Missing | ProjectionStatus::Stale => {
            let message = format!(
                "Planning PROJECTION_{}: session={session_id}, kind={kind}; run `megara doctor --repair`",
                status.as_str().to_ascii_uppercase(),
            );
            warnings.push(message.clone());
            issues.push(DoctorIssue {
                code: projection_code(status),
                message,
                path: None,
                repairable: true,
            });
        }
        ProjectionStatus::Conflict | ProjectionStatus::IoError | ProjectionStatus::Written => {
            let message = format!(
                "Planning PROJECTION_{}: session={session_id}, kind={kind}",
                status.as_str().to_ascii_uppercase(),
            );
            warnings.push(message.clone());
            issues.push(DoctorIssue {
                code: projection_code(status),
                message,
                path: None,
                repairable: false,
            });
        }
    }
}

impl DoctorReport {
    pub fn print(&self) -> Result<()> {
        if self.json {
            println!("{}", serde_json::to_string_pretty(self)?);
            return Ok(());
        }

        let rows = [
            ("scope", self.scope.clone()),
            ("target", self.target.clone()),
            ("ok", self.ok.to_string()),
        ];
        let mut sections = vec![Section::new(
            "Run",
            vec![format!(
                "megara doctor: scope={}, target={}, ok={}",
                self.scope, self.target, self.ok
            )],
        )];
        push_group(&mut sections, "Missing", &self.missing);
        push_group(&mut sections, "Unmanaged", &self.unmanaged);
        push_group(&mut sections, "Stale", &self.stale);
        push_group(&mut sections, "Warnings", &self.warnings);
        push_group(&mut sections, "Observations", &self.observations);

        let status = if self.ok { "OK" } else { "issues found" };
        ui::print_dashboard("Doctor", status, &rows, &sections)?;
        Ok(())
    }
}

fn runtime_dependency_issues(target: TargetRuntime) -> Vec<String> {
    match target {
        TargetRuntime::Codex => codex::runtime_dependency_issues(),
        TargetRuntime::Pi => pi::runtime_dependency_issues(),
    }
}

fn push_group(sections: &mut Vec<Section>, label: &str, paths: &[String]) {
    if !paths.is_empty() {
        sections.push(Section::new(label, paths.to_vec()));
    }
}

fn inspect_managed_file(
    file: &PlannedFile,
    missing: &mut Vec<String>,
    unmanaged: &mut Vec<String>,
    stale: &mut Vec<String>,
    issues: &mut Vec<DoctorIssue>,
    repair: bool,
    observations: &mut Vec<String>,
) -> Result<()> {
    let path = &file.path;
    let desired = &file.content;
    if !path.exists() {
        if repair {
            repair_managed_file(file, observations)?;
            return Ok(());
        }
        let path = path.display().to_string();
        missing.push(path.clone());
        issues.push(projection_issue("PROJECTION_MISSING", path, true));
        return Ok(());
    }

    let current = fs::read_to_string(path)?;
    if !current.contains(MANAGED_MARKER) {
        let path = path.display().to_string();
        unmanaged.push(path.clone());
        issues.push(projection_issue("PROJECTION_DIVERGED", path, false));
    } else if current != desired.as_str() {
        if repair {
            repair_managed_file(file, observations)?;
            return Ok(());
        }
        let path = path.display().to_string();
        stale.push(path.clone());
        issues.push(projection_issue("PROJECTION_STALE", path, true));
    }
    Ok(())
}

fn repair_managed_file(file: &PlannedFile, observations: &mut Vec<String>) -> Result<()> {
    write_files(std::slice::from_ref(file), false, true)?;
    observations.push(format!(
        "Managed projection repair: {}",
        file.path.display()
    ));
    Ok(())
}

fn projection_issue(code: &'static str, path: String, repairable: bool) -> DoctorIssue {
    DoctorIssue {
        code,
        message: format!("managed projection requires attention: {path}"),
        path: Some(path),
        repairable,
    }
}

fn projection_code(status: ProjectionStatus) -> &'static str {
    match status {
        ProjectionStatus::Missing => "PROJECTION_MISSING",
        ProjectionStatus::Stale => "PROJECTION_STALE",
        ProjectionStatus::Conflict => "PROJECTION_DIVERGED",
        ProjectionStatus::IoError => "PROJECTION_IO",
        ProjectionStatus::Written | ProjectionStatus::Unchanged => "PROJECTION_STALE",
    }
}

fn inspect_wrapper_invocation(path: &Path, warnings: &mut Vec<String>) {
    if !path.exists() {
        return;
    }
    match Command::new(path).arg("--version").output() {
        Ok(output) if output.status.success() => {}
        Ok(output) => warnings.push(format!(
            "Megara wrapper is not invocable: {} exited with {}",
            path.display(),
            output.status
        )),
        Err(error) => warnings.push(format!(
            "Megara wrapper is not invocable: {} ({error})",
            path.display()
        )),
    }
}
