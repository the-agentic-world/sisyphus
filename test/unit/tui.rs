use crate::{
    cli::{DoctorArgs, InstallArgs, ScopeArg, TargetArg, UpdateArgs, UpdateScopeArg},
    tui::{
        doctor_tui_options, scripted_install_wizard, use_doctor_tui_for, use_install_tui_for,
        use_update_tui_for, TuiInput,
    },
};
use ratatui::{backend::TestBackend, Terminal};

fn install_args(scope: Option<ScopeArg>, target: Option<TargetArg>) -> InstallArgs {
    InstallArgs {
        scope,
        target,
        locale: None,
        dry_run: false,
        force: false,
        trust_project: false,
        json: false,
        no_interactive: false,
    }
}

fn doctor_args(scope: Option<ScopeArg>, target: Option<TargetArg>) -> DoctorArgs {
    DoctorArgs {
        scope,
        target,
        json: false,
        repair: false,
        no_interactive: false,
    }
}

fn update_args() -> UpdateArgs {
    UpdateArgs {
        scope: UpdateScopeArg::All,
        target: TargetArg::Codex,
        force: false,
        no_interactive: false,
    }
}

#[test]
fn install_tui_only_handles_missing_tty_inputs() {
    let missing = install_args(None, Some(TargetArg::Codex));
    assert!(use_install_tui_for(&missing, true, false));

    let complete = install_args(Some(ScopeArg::Project), Some(TargetArg::Codex));
    assert!(use_install_tui_for(&complete, true, false));

    let mut complete = complete;
    complete.locale = Some("ko-KR".to_string());
    assert!(!use_install_tui_for(&complete, true, false));
    assert!(!use_install_tui_for(&missing, false, false));
    assert!(!use_install_tui_for(&missing, true, true));
}

#[test]
fn install_tui_respects_json_and_no_interactive() {
    let mut args = install_args(None, None);
    args.json = true;
    assert!(!use_install_tui_for(&args, true, false));

    args.json = false;
    args.no_interactive = true;
    assert!(!use_install_tui_for(&args, true, false));
}

#[test]
fn update_tui_requires_tty_and_interactive_mode() {
    let args = update_args();
    assert!(use_update_tui_for(&args, true, false));
    assert!(!use_update_tui_for(&args, false, false));
    assert!(!use_update_tui_for(&args, true, true));

    let mut disabled = update_args();
    disabled.no_interactive = true;
    assert!(!use_update_tui_for(&disabled, true, false));
}

#[test]
fn doctor_tui_only_handles_bare_tty_non_json() {
    let args = doctor_args(None, None);
    assert!(use_doctor_tui_for(&args, true, false));
    assert!(!use_doctor_tui_for(&args, false, false));
    assert!(!use_doctor_tui_for(&args, true, true));

    let with_scope = doctor_args(Some(ScopeArg::Project), None);
    assert!(!use_doctor_tui_for(&with_scope, true, false));

    let mut json = doctor_args(None, None);
    json.json = true;
    assert!(!use_doctor_tui_for(&json, true, false));
}

#[test]
fn doctor_tui_defaults_to_project_codex() {
    let options = doctor_tui_options(doctor_args(None, None)).expect("doctor options");
    assert_eq!(options.scope.to_string(), "project");
    assert_eq!(options.target.to_string(), "codex");
    assert!(!options.json);
}

#[test]
fn scripted_install_wizard_collects_missing_values_and_confirms() {
    let args = install_args(None, None);
    let result = scripted_install_wizard(
        args,
        &[
            TuiInput::Select(1),
            TuiInput::Select(1),
            TuiInput::Confirm,
            TuiInput::Select(0),
        ],
    )
    .expect("wizard should succeed")
    .expect("wizard should confirm");

    assert_eq!(result.locale.as_deref(), Some("en-US"));
    assert_eq!(result.scope, Some(ScopeArg::Global));
    assert_eq!(result.target, Some(TargetArg::Codex));
}

#[test]
fn scripted_install_wizard_can_cancel_before_side_effects() {
    let args = install_args(None, None);
    let result = scripted_install_wizard(args, &[TuiInput::Cancel]).expect("wizard should return");
    assert!(result.is_none());
}

#[test]
fn scripted_install_wizard_preserves_existing_flags() {
    let mut args = install_args(Some(ScopeArg::Project), None);
    args.dry_run = true;
    args.force = true;
    let result = scripted_install_wizard(
        args,
        &[
            TuiInput::Select(0),
            TuiInput::Confirm,
            TuiInput::Confirm,
            TuiInput::Confirm,
        ],
    )
    .expect("wizard should succeed")
    .expect("wizard should confirm");

    assert_eq!(result.locale.as_deref(), Some("ko-KR"));
    assert_eq!(result.scope, Some(ScopeArg::Project));
    assert_eq!(result.target, Some(TargetArg::Codex));
    assert!(result.dry_run);
    assert!(result.force);
    assert!(result.trust_project);
}

#[test]
fn scripted_install_wizard_records_pi_project_trust() {
    let mut args = install_args(Some(ScopeArg::Project), Some(TargetArg::Pi));
    args.locale = Some("ko-KR".to_string());
    let result = scripted_install_wizard(args, &[TuiInput::Select(0), TuiInput::Select(0)])
        .expect("wizard should succeed")
        .expect("wizard should confirm");

    assert!(result.trust_project);
}

#[test]
fn narrow_menu_subtitles_are_truncated_without_losing_navigation_space() {
    assert_eq!(
        crate::tui::truncate_for_width("Choose the user-facing response locale.", 36),
        "Choose the user-facing response loc…"
    );
    assert_eq!(crate::tui::truncate_for_width("abc", 1), "…");
}

#[test]
fn narrow_locale_menu_renders_all_choices_selection_and_compact_navigation() {
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    terminal
        .draw(|frame| {
            crate::tui::render_menu(
                frame,
                "Megara Install",
                "Choose the user-facing response locale.",
                &crate::tui::locale_options(),
                2,
            )
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let snapshot = buffer
        .content
        .chunks(buffer.area.width as usize)
        .map(|cells| cells.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    for label in [
        "1. Korean (ko-KR)",
        "2. English (en-US)",
        "3. Japanese (ja-JP)",
        "4. Chinese (zh-CN)",
    ] {
        assert!(
            snapshot.contains(label),
            "missing {label:?} in:\n{snapshot}"
        );
    }
    assert!(snapshot.contains("> 3. Japanese (ja-JP)"), "{snapshot}");
    assert!(snapshot.contains("Enter select"), "{snapshot}");
    assert!(snapshot.contains("q cancel"), "{snapshot}");
    assert!(!snapshot.contains("Recommended default"), "{snapshot}");
}
