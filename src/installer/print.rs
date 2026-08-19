use anyhow::Result;

use crate::ui::{self, Section};

use super::model::{InstallAction, InstallResult};

impl InstallResult {
    pub fn print(&self) -> Result<()> {
        if self.options.json {
            println!("{}", serde_json::to_string_pretty(self)?);
            return Ok(());
        }

        let verb = match (self.options.action, self.options.dry_run) {
            (InstallAction::Install, true) => "install planned",
            (InstallAction::Install, false) => "installed",
            (InstallAction::Sync, true) => "sync planned",
            (InstallAction::Sync, false) => "synced",
        };
        let rows = [
            ("scope", self.plan.scope.to_string()),
            ("target", self.plan.target.to_string()),
            ("ssot", self.plan.ssot_root.display().to_string()),
            ("runtime", self.plan.runtime_root.display().to_string()),
            ("projection", self.plan.target_root.display().to_string()),
        ];
        let mut sections = vec![Section::new(
            "Run",
            vec![
                format!(
                    "megara {verb}: scope={}, target={}, ssot={}, runtime={}, projection={}",
                    self.plan.scope,
                    self.plan.target,
                    self.plan.ssot_root.display(),
                    self.plan.runtime_root.display(),
                    self.plan.target_root.display()
                ),
                format!(
                    "created={}, updated={}, unchanged={}, conflicts={}, removed={}",
                    self.summary.created.len(),
                    self.summary.updated.len(),
                    self.summary.unchanged.len(),
                    self.summary.conflicts.len(),
                    self.summary.removed.len()
                ),
            ],
        )];

        if !self.summary.conflicts.is_empty() {
            sections.push(Section::new(
                "Conflicts",
                self.summary
                    .conflicts
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect(),
            ));
        }

        if !self.summary.removed.is_empty() {
            sections.push(Section::new(
                "Removed",
                self.summary
                    .removed
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect(),
            ));
        }

        if !self.migrations.is_empty() {
            sections.push(Section::new(
                "Migration",
                self.migrations
                    .iter()
                    .map(|migration| {
                        format!(
                            "legacy runtime state: source={}, destination={}, moved={}, conflicts={}, removed_source={}",
                            migration.source.display(),
                            migration.destination.display(),
                            migration.moved.len(),
                            migration.conflicts.len(),
                            migration.removed_source
                        )
                    })
                    .collect(),
            ));
        }

        if let Some(project_trust) = &self.project_trust {
            sections.push(Section::new(
                "Project Trust",
                vec![format!(
                    "Codex project trust: registered={}, unchanged={}, skipped={}, project={}, config={}",
                    project_trust.registered,
                    project_trust.unchanged,
                    project_trust.skipped,
                    project_trust.project_root.display(),
                    project_trust.config_path.display()
                )],
            ));
        }

        if let Some(runtime_features) = &self.plan.codex_runtime_features {
            sections.push(Section::new(
                "Codex Runtime",
                vec![format!(
                    "version={}, request_user_input={}",
                    runtime_features
                        .version
                        .as_deref()
                        .unwrap_or("not detected"),
                    if runtime_features.default_mode_request_user_input {
                        "available"
                    } else {
                        "Markdown fallback"
                    }
                )],
            ));
        }

        if !self.warnings.is_empty() {
            sections.push(Section::new("Warnings", self.warnings.clone()));
        }

        ui::print_dashboard("Install", verb, &rows, &sections)?;
        Ok(())
    }
}
