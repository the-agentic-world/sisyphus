use clap::Args;

use super::{ScopeArg, TargetArg};

#[derive(Debug, Args)]
pub struct InstallArgs {
    #[arg(long, value_enum)]
    pub scope: Option<ScopeArg>,
    #[arg(long, value_enum)]
    pub target: Option<TargetArg>,
    #[arg(long, value_name = "LOCALE", help = "Set user-facing response locale")]
    pub locale: Option<String>,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub force: bool,
    #[arg(
        long,
        help = "Trust this project in Codex or allow project-local Pi role agents after installation"
    )]
    pub trust_project: bool,
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub no_interactive: bool,
}

#[derive(Debug, Args)]
pub struct SyncArgs {
    #[arg(long, value_enum)]
    pub scope: Option<ScopeArg>,
    #[arg(long, value_enum)]
    pub target: Option<TargetArg>,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub no_interactive: bool,
}

impl From<SyncArgs> for InstallArgs {
    fn from(args: SyncArgs) -> Self {
        Self {
            scope: args.scope,
            target: args.target,
            locale: None,
            dry_run: args.dry_run,
            force: args.force,
            trust_project: false,
            json: args.json,
            no_interactive: args.no_interactive,
        }
    }
}

#[derive(Debug, Args)]
pub struct DoctorArgs {
    #[arg(long, value_enum)]
    pub scope: Option<ScopeArg>,
    #[arg(long, value_enum)]
    pub target: Option<TargetArg>,
    #[arg(long)]
    pub json: bool,
    #[arg(long, help = "Retry pending Planning Core cleanup residue")]
    pub repair: bool,
    #[arg(long)]
    pub no_interactive: bool,
}
