use std::{env, fs, path::PathBuf};

use anyhow::{bail, Context, Result};
use toml::Value;

use crate::{
    paths::{InstallPaths, InstallScope, TargetRuntime},
    targets::{codex, pi},
    templates::TemplateRegistry,
    writer::{remove_managed_files, remove_obsolete_managed_files, write_files},
};

use super::migration;
use super::model::*;

pub struct Planner<'a> {
    registry: &'a TemplateRegistry,
    options: InstallOptions,
}

impl<'a> Planner<'a> {
    pub fn new(registry: &'a TemplateRegistry, options: InstallOptions) -> Self {
        Self { registry, options }
    }

    pub fn plan(&self) -> Result<InstallPlan> {
        self.plan_with_managed_edits(true)
    }

    pub fn plan_without_managed_edits(&self) -> Result<InstallPlan> {
        self.plan_with_managed_edits(false)
    }

    fn plan_with_managed_edits(&self, include_managed_edits: bool) -> Result<InstallPlan> {
        let paths = InstallPaths::resolve(self.options.scope, self.options.target)?;
        let codex_runtime_features =
            (self.options.target == TargetRuntime::Codex).then(codex::detect_runtime_features);
        let mut files = Vec::new();
        let mut managed_toml_edits = Vec::new();
        files.extend(runtime_support_files(
            paths.ssot_root.clone(),
            paths.runtime_root.clone(),
        )?);
        let projection_registry = match self.options.action {
            InstallAction::Install => {
                let registry = registry_with_locale(self.registry, self.options.locale.as_deref())?;
                let registry = if self.options.locale.is_none() {
                    registry_with_existing_config(&paths.ssot_root, registry)?
                } else {
                    registry
                };
                files.extend(ssot_files(paths.ssot_root.clone(), &registry));
                registry
            }
            InstallAction::Sync => TemplateRegistry::from_ssot_root(&paths.ssot_root)?,
        };

        match self.options.target {
            TargetRuntime::Codex => {
                let (projection_files, managed_edit) = if include_managed_edits {
                    codex::projection_plan_with_force(
                        paths.target_root.clone(),
                        self.options.scope,
                        &projection_registry,
                        self.options.force,
                        codex_runtime_features
                            .as_ref()
                            .expect("Codex projection has runtime features"),
                    )?
                } else {
                    (
                        codex::projection_files_with_force(
                            paths.target_root.clone(),
                            self.options.scope,
                            &projection_registry,
                            self.options.force,
                            codex_runtime_features
                                .as_ref()
                                .expect("Codex projection has runtime features"),
                        )?,
                        None,
                    )
                };
                files.extend(projection_files);
                if let Some(edit) = managed_edit {
                    managed_toml_edits.push(edit);
                }
            }
            TargetRuntime::Pi => files.extend(pi::projection_files(
                paths.target_root.clone(),
                self.options.scope,
                &projection_registry,
            )?),
        };
        let (obsolete_files, obsolete_managed_files) = match self.options.target {
            TargetRuntime::Codex => (
                codex::obsolete_projection_files(
                    paths.target_root.clone(),
                    self.options.scope,
                    &projection_registry,
                ),
                Vec::new(),
            ),
            TargetRuntime::Pi => (
                Vec::new(),
                pi::obsolete_projection_files(
                    paths.target_root.clone(),
                    self.options.scope,
                    &projection_registry,
                )?,
            ),
        };

        Ok(InstallPlan {
            scope: self.options.scope,
            target: self.options.target,
            ssot_root: paths.ssot_root,
            runtime_root: paths.runtime_root,
            target_root: paths.target_root,
            codex_runtime_features,
            files,
            managed_toml_edits,
            obsolete_files,
            obsolete_managed_files,
        })
    }

    pub fn execute(&self) -> Result<InstallResult> {
        let plan = self.plan()?;
        let mut preflight = write_files(&plan.files, true, self.options.force)?;
        let obsolete_preflight =
            remove_obsolete_managed_files(&plan.obsolete_managed_files, true, self.options.force)?;
        preflight.conflicts.extend(obsolete_preflight.conflicts);
        preflight.removed.extend(obsolete_preflight.removed);
        if !self.options.dry_run && !preflight.conflicts.is_empty() {
            bail!(
                "refusing to overwrite {} unmanaged file(s); rerun with --force",
                preflight.conflicts.len()
            );
        }
        let mut summary = if self.options.dry_run {
            preflight
        } else {
            for edit in &plan.managed_toml_edits {
                edit.apply(false)?;
            }
            write_files(&plan.files, false, self.options.force)?
        };
        if !self.options.dry_run {
            summary.removed.extend(
                remove_obsolete_managed_files(
                    &plan.obsolete_managed_files,
                    false,
                    self.options.force,
                )?
                .removed,
            );
        }
        summary.removed.extend(remove_managed_files(
            &plan.obsolete_files,
            self.options.dry_run,
        )?);
        let migrations = migration::migrate_legacy_project_state(
            &plan.ssot_root,
            &plan.runtime_root,
            self.options.dry_run,
        )?
        .into_iter()
        .collect::<Vec<_>>();
        let project_trust = if self.options.target == TargetRuntime::Codex
            && self.options.scope == InstallScope::Project
            && self.options.trust_project
        {
            Some(codex::ensure_project_trust(
                plan.target_root
                    .parent()
                    .context("Codex project root has no parent")?,
                self.options.dry_run,
            )?)
        } else {
            None
        };
        let mut warnings = runtime_dependency_issues(self.options.target);
        if self.options.target == TargetRuntime::Codex
            && self.options.scope == InstallScope::Project
            && !self.options.trust_project
            && !codex::is_project_trusted(
                plan.target_root
                    .parent()
                    .context("Codex project root has no parent")?,
            )?
        {
            warnings.push(
                "Codex project config remains inactive until you trust the project in Codex or rerun install with --trust-project."
                    .to_string(),
            );
        }
        if self.options.target == TargetRuntime::Pi && self.options.scope == InstallScope::Project {
            if self.options.trust_project {
                let projection_registry = match self.options.action {
                    InstallAction::Install => {
                        registry_with_locale(self.registry, self.options.locale.as_deref())?
                    }
                    InstallAction::Sync => TemplateRegistry::from_ssot_root(&plan.ssot_root)?,
                };
                pi::ensure_project_trust(
                    &plan.runtime_root,
                    plan.target_root
                        .parent()
                        .context("Pi project root has no parent")?,
                    &projection_registry,
                    self.options.dry_run,
                )?;
            } else {
                warnings.push(
                    "Pi project role agents remain disabled until you rerun install with --trust-project."
                        .to_string(),
                );
            }
        }
        for migration in &migrations {
            if !migration.conflicts.is_empty() {
                warnings.push(format!(
                    "legacy runtime state migration left {} conflicting file(s) under {}; review them before removing the legacy state directory",
                    migration.conflicts.len(),
                    migration.source.display()
                ));
            }
        }
        Ok(InstallResult {
            options: self.options.clone(),
            plan,
            summary,
            migrations,
            project_trust,
            warnings,
        })
    }
}

fn runtime_dependency_issues(target: TargetRuntime) -> Vec<String> {
    match target {
        TargetRuntime::Codex => codex::runtime_dependency_issues(),
        TargetRuntime::Pi => pi::runtime_dependency_issues(),
    }
}

fn ssot_files(root: PathBuf, registry: &TemplateRegistry) -> Vec<PlannedFile> {
    registry
        .ssot_files()
        .iter()
        .map(|template| {
            PlannedFile::new(root.join(&template.relative_path), template.content.clone())
        })
        .collect()
}

fn registry_with_locale(
    registry: &TemplateRegistry,
    locale: Option<&str>,
) -> Result<TemplateRegistry> {
    let Some(locale) = locale else {
        return Ok(registry.clone());
    };
    let Some(config) = registry.config() else {
        return Ok(registry.clone());
    };
    let content = render_config_template(&config.content, Some(locale))?;
    Ok(registry.with_config_content(content))
}

fn registry_with_existing_config(
    root: &std::path::Path,
    registry: TemplateRegistry,
) -> Result<TemplateRegistry> {
    let Some(config) = registry.config() else {
        return Ok(registry);
    };
    let path = root.join(&config.relative_path);
    if !path.exists() {
        return Ok(registry);
    }
    let content = fs::read_to_string(&path)
        .with_context(|| format!("failed to read existing Megara config {}", path.display()))?;
    Ok(registry.with_config_content(crate::installer::strip_managed_marker(&content)))
}

fn render_config_template(content: &str, locale: Option<&str>) -> Result<String> {
    let Some(locale) = locale else {
        return Ok(content.to_string());
    };
    let mut value: Value = content
        .parse()
        .context("failed to parse bundled Megara config template")?;
    if let Some(table) = value.as_table_mut() {
        table.insert("locale".to_string(), Value::String(locale.to_string()));
    }
    toml::to_string_pretty(&value).context("failed to render Megara config template")
}

pub(crate) fn runtime_support_files(
    root: PathBuf,
    runtime_root: PathBuf,
) -> Result<Vec<PlannedFile>> {
    let megara_bin = env::current_exe().context("failed to resolve current megara executable")?;
    let mut files = vec![
        PlannedFile::new_executable_shell(
            root.join("bin").join("megara"),
            format!(
                "#!/bin/sh\nexec {} \"$@\"\n",
                shell_quote(&megara_bin.display().to_string())
            ),
        ),
        PlannedFile::new_executable_shell(
            root.join("bin").join("insane-search"),
            r#"#!/bin/sh
set -eu
bin_dir=$(CDPATH= cd "$(dirname "$0")" && pwd -P)
root_dir=$(CDPATH= cd "$bin_dir/.." && pwd -P)
tool_dir="$bin_dir/../tools/insane-search"
if [ "$(basename "$root_dir")" = ".agents" ]; then
  runtime_root="$root_dir/../.megara"
else
  runtime_root="$root_dir"
fi
state_dir="$runtime_root/state/tools/insane-search"
venv_dir="$state_dir/venv"
python_bin="$venv_dir/bin/python"
requirements="$tool_dir/requirements.txt"
requirements_stamp="$state_dir/requirements.stamp"
if [ ! -d "$tool_dir" ]; then
  echo "insane-search tool directory not found: $tool_dir" >&2
  exit 2
fi
if [ ! -x "$python_bin" ]; then
  mkdir -p "$state_dir"
  echo "insane-search: bootstrapping Python dependencies into $venv_dir" >&2
  bootstrap_python=""
  for candidate in python3.13 python3.12 python3.11 python3.10 python3; do
    if command -v "$candidate" >/dev/null 2>&1 \
      && "$candidate" -c 'import sys; raise SystemExit(0 if sys.version_info >= (3, 10) else 1)' >/dev/null 2>&1; then
      bootstrap_python=$(command -v "$candidate")
      break
    fi
  done
  if [ -z "$bootstrap_python" ]; then
    echo "insane-search requires Python 3.10 or newer; install one and rerun." >&2
    exit 2
  fi
  "$bootstrap_python" -m venv "$venv_dir"
fi
needs_install=0
if [ ! -f "$requirements_stamp" ] || [ "$requirements" -nt "$requirements_stamp" ]; then
  needs_install=1
fi
if ! "$python_bin" - <<'PY' >/dev/null 2>&1
import importlib.util
missing = [
    package
    for package in ("curl_cffi", "bs4", "yaml", "yt_dlp")
    if importlib.util.find_spec(package) is None
]
raise SystemExit(1 if missing else 0)
PY
then
  needs_install=1
fi
if [ "$needs_install" = "1" ]; then
  "$python_bin" -m ensurepip --upgrade >/dev/null 2>&1 || true
  PIP_DISABLE_PIP_VERSION_CHECK=1 "$python_bin" -m pip install -r "$requirements" >&2
  touch "$requirements_stamp"
fi
cd "$tool_dir"
exec "$python_bin" -m engine "$@"
"#,
        ),
    ];
    if runtime_root != root {
        files.push(PlannedFile::new(
            runtime_root.join(".gitignore"),
            "state/\nartifacts/\ncache/\nplanning/\nmigration-backups/\n",
        ));
    }
    Ok(files)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
