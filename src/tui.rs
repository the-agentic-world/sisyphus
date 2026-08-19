use std::{
    env,
    io::{self, IsTerminal},
};

use anyhow::{bail, Result};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Terminal,
};

use crate::{
    cli::{
        ConfigureAgentsArgs, DoctorArgs, InstallArgs, ScopeArg, TargetArg, UpdateArgs,
        UpdateScopeArg,
    },
    doctor::DoctorReport,
    installer::DoctorOptions,
    paths::{InstallScope, TargetRuntime},
};

pub(crate) fn prepare_install(args: InstallArgs) -> Result<Option<InstallArgs>> {
    if use_install_tui(&args) {
        run_install_wizard(args)
    } else {
        Ok(Some(args))
    }
}

pub(crate) fn use_install_tui(args: &InstallArgs) -> bool {
    use_install_tui_for(args, interactive_terminal(), is_ci())
}

pub(crate) fn use_update_tui(args: &UpdateArgs) -> bool {
    use_update_tui_for(args, interactive_terminal(), is_ci())
}

pub(crate) fn use_doctor_tui(args: &DoctorArgs) -> bool {
    use_doctor_tui_for(args, interactive_terminal(), is_ci())
}

pub(crate) fn confirm_update(args: &UpdateArgs) -> Result<bool> {
    let subtitle = format!(
        "scope={}, target={}, force={}, version={}",
        update_scope_label(args.scope),
        target_label(args.target),
        args.force,
        env!("CARGO_PKG_VERSION")
    );
    let choice = run_menu(
        "Megara Update",
        &subtitle,
        &[
            MenuOption::new(
                "Run update",
                "Install the latest binary and refresh managed files.",
            ),
            MenuOption::new("Cancel", "Leave the current installation unchanged."),
        ],
    )?;
    Ok(matches!(choice, Some(0)))
}

pub(crate) fn configure_agents_wizard(
    mut args: ConfigureAgentsArgs,
    _roles: &[String],
) -> Result<Option<ConfigureAgentsArgs>> {
    if args.scope.is_none() {
        args.scope = match run_menu(
            "Agent Policies",
            "Choose the policy scope.",
            &[
                MenuOption::new("Project", "Override policies for this project."),
                MenuOption::new("Global", "Set defaults for every project."),
            ],
        )? {
            Some(0) => Some(ScopeArg::Project),
            Some(1) => Some(ScopeArg::Global),
            _ => return Ok(None),
        };
    }
    if args.target.is_none() {
        args.target = match run_menu(
            "Agent Policies",
            "Choose the target runtime.",
            &[
                MenuOption::new("Codex", "Configure Codex model and reasoning effort."),
                MenuOption::new("Pi", "Configure Pi model and thinking level."),
            ],
        )? {
            Some(0) => Some(TargetArg::Codex),
            Some(1) => Some(TargetArg::Pi),
            _ => return Ok(None),
        };
    }
    if args.role.is_empty() && !args.all {
        let selected = run_menu(
            "Agent Policies",
            "Choose which roles to change.",
            &[
                MenuOption::new("All roles", "Apply one policy to every Megara role."),
                MenuOption::new("One role", "Configure one selected role."),
            ],
        )?;
        match selected {
            Some(0) => args.all = true,
            Some(1) => {
                let roles = [
                    "executor",
                    "planner",
                    "architect",
                    "critic",
                    "researcher",
                    "contrarian",
                    "simplifier",
                ];
                let options = roles
                    .iter()
                    .map(|role| MenuOption::new(role, "Configure this role."))
                    .collect::<Vec<_>>();
                let Some(index) = run_menu("Agent Policies", "Choose a role.", &options)? else {
                    return Ok(None);
                };
                args.role.push(roles[index].to_string());
            }
            _ => return Ok(None),
        }
    }
    let target = args.target.expect("target selected");
    if args.model.is_none() {
        let options = match target {
            TargetArg::Codex => vec![
                MenuOption::new("gpt-5.6-terra", "Balanced implementation model."),
                MenuOption::new("gpt-5.6-sol", "Higher reasoning for design and critique."),
                MenuOption::new("gpt-5.6-luna", "Lower-cost simplification model."),
            ],
            TargetArg::Pi => vec![
                MenuOption::new("openai/gpt-5.5", "Use the configured OpenAI Codex model."),
                MenuOption::new("openai/gpt-5.4", "Use a lower-capability compatible model."),
                MenuOption::new("anthropic/claude-sonnet-4", "Use a Claude Sonnet model."),
            ],
        };
        let Some(index) = run_menu("Agent Policies", "Choose the role model.", &options)? else {
            return Ok(None);
        };
        args.model = Some(options[index].label.to_string());
    }
    let levels = ["minimal", "low", "medium", "high", "xhigh"];
    let selected = run_menu(
        "Agent Policies",
        "Choose the reasoning level.",
        &levels
            .iter()
            .map(|level| MenuOption::new(level, "Apply this level."))
            .collect::<Vec<_>>(),
    )?;
    let Some(index) = selected else {
        return Ok(None);
    };
    match target {
        TargetArg::Codex if args.reasoning_effort.is_none() => {
            args.reasoning_effort = Some(levels[index].to_string())
        }
        TargetArg::Pi if args.thinking_level.is_none() => {
            args.thinking_level = Some(levels[index].to_string())
        }
        _ => {}
    }
    Ok(Some(args))
}

pub(crate) fn doctor_tui_options(args: DoctorArgs) -> Result<DoctorOptions> {
    Ok(DoctorOptions {
        scope: args.scope.map(Into::into).unwrap_or(InstallScope::Project),
        target: args.target.map(Into::into).unwrap_or(TargetRuntime::Codex),
        json: args.json,
        repair: args.repair,
    })
}

pub(crate) fn show_doctor_report(report: &DoctorReport) -> Result<()> {
    let mut lines = vec![
        Line::from(vec![
            Span::styled("Status: ", Style::default().fg(Color::Gray)),
            Span::styled(
                if report.ok { "OK" } else { "issues found" },
                Style::default()
                    .fg(if report.ok {
                        Color::Green
                    } else {
                        Color::Yellow
                    })
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(format!("Scope: {}", report.scope)),
        Line::from(format!("Target: {}", report.target)),
        Line::from(""),
    ];
    push_report_group(&mut lines, "Missing", &report.missing);
    push_report_group(&mut lines, "Unmanaged", &report.unmanaged);
    push_report_group(&mut lines, "Stale", &report.stale);
    push_report_group(&mut lines, "Warnings", &report.warnings);
    push_report_group(&mut lines, "Observations", &report.observations);
    if report.ok {
        lines.push(Line::from("No installation drift detected."));
    }
    run_read_only("Megara Doctor", "Press Enter, Esc, or q to close.", lines)
}

pub(crate) fn use_install_tui_for(args: &InstallArgs, terminal: bool, ci: bool) -> bool {
    terminal
        && !ci
        && !args.no_interactive
        && !args.json
        && (args.locale.is_none() || args.scope.is_none() || args.target.is_none())
}

pub(crate) fn use_update_tui_for(args: &UpdateArgs, terminal: bool, ci: bool) -> bool {
    terminal && !ci && !args.no_interactive
}

pub(crate) fn use_doctor_tui_for(args: &DoctorArgs, terminal: bool, ci: bool) -> bool {
    terminal
        && !ci
        && !args.no_interactive
        && !args.json
        && !args.repair
        && args.scope.is_none()
        && args.target.is_none()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TuiInput {
    Up,
    Down,
    Select(usize),
    Confirm,
    Cancel,
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn scripted_install_wizard(
    args: InstallArgs,
    input: &[TuiInput],
) -> Result<Option<InstallArgs>> {
    let mut input = input.iter().copied();
    install_wizard_with(args, |_, _, options| {
        scripted_menu_choice(options.len(), &mut input)
    })
}

fn run_install_wizard(args: InstallArgs) -> Result<Option<InstallArgs>> {
    let _guard = TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.clear()?;
    let result = install_wizard_with(args, |title, subtitle, options| {
        run_menu_with_terminal(&mut terminal, title, subtitle, options)
    });
    terminal.clear()?;
    result
}

fn install_wizard_with<F>(mut args: InstallArgs, mut choose: F) -> Result<Option<InstallArgs>>
where
    F: FnMut(&str, &str, &[MenuOption]) -> Result<Option<usize>>,
{
    if args.locale.is_none() {
        let selected = choose(
            "Megara Install",
            "Choose the user-facing response locale.",
            &locale_options(),
        )?;
        args.locale = match selected {
            Some(0) => Some("ko-KR".to_string()),
            Some(1) => Some("en-US".to_string()),
            Some(2) => Some("ja-JP".to_string()),
            Some(3) => Some("zh-CN".to_string()),
            Some(_) => unreachable!("menu returned an out-of-range option"),
            None => return Ok(None),
        };
    }

    if args.scope.is_none() {
        let selected = choose(
            "Megara Install",
            "Choose where Megara should manage the harness.",
            &[
                MenuOption::new("Project", "Install into the current project."),
                MenuOption::new("Global", "Install into the user profile."),
            ],
        )?;
        args.scope = match selected {
            Some(0) => Some(ScopeArg::Project),
            Some(1) => Some(ScopeArg::Global),
            Some(_) => unreachable!("menu returned an out-of-range option"),
            None => return Ok(None),
        };
    }

    if args.target.is_none() {
        let selected = choose(
            "Megara Install",
            "Choose the agent runtime projection.",
            &[
                MenuOption::new("Codex", "Generate Codex harness files."),
                MenuOption::new("Pi", "Generate Pi Coding Agent harness files."),
            ],
        )?;
        args.target = match selected {
            Some(0) => Some(TargetArg::Codex),
            Some(1) => Some(TargetArg::Pi),
            Some(_) => unreachable!("menu returned an out-of-range option"),
            None => return Ok(None),
        };
    }

    if args.scope == Some(ScopeArg::Project) && !args.trust_project {
        match args.target {
            Some(TargetArg::Codex) => {
                let selected = choose(
                    "Megara Install",
                    "Trust this project in Codex?",
                    &[
                        MenuOption::new(
                            "Trust project",
                            "Allow this project's .codex config, MCP, and rules to load.",
                        ),
                        MenuOption::new(
                            "Install without trust",
                            "Keep project config inactive until you rerun install with --trust-project.",
                        ),
                    ],
                )?;
                args.trust_project = matches!(selected, Some(0));
            }
            Some(TargetArg::Pi) => {
                let selected = choose(
                    "Megara Install",
                    "Allow the generated Pi extension to run this project's role agents?",
                    &[
                        MenuOption::new(
                            "Trust project agents",
                            "Required before Pi can run project-local role agents.",
                        ),
                        MenuOption::new(
                            "Install without trust",
                            "Install files now and enable role agents later with --trust-project.",
                        ),
                    ],
                )?;
                args.trust_project = matches!(selected, Some(0));
            }
            None => unreachable!("target selected"),
        }
    }

    let selected = choose(
        "Megara Install",
        &format!(
            "locale={}, scope={}, target={}, dry-run={}, force={}",
            args.locale.as_deref().unwrap_or("ko-KR"),
            scope_label(args.scope.expect("scope selected")),
            target_label(args.target.expect("target selected")),
            args.dry_run,
            args.force
        ),
        &[
            MenuOption::new("Install", "Write managed harness files."),
            MenuOption::new("Cancel", "Leave the project unchanged."),
        ],
    )?;
    if !matches!(selected, Some(0)) {
        return Ok(None);
    }

    Ok(Some(args))
}

fn run_menu(title: &str, subtitle: &str, options: &[MenuOption]) -> Result<Option<usize>> {
    let _guard = TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.clear()?;
    let result = run_menu_with_terminal(&mut terminal, title, subtitle, options);
    terminal.clear()?;
    result
}

fn run_menu_with_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    title: &str,
    subtitle: &str,
    options: &[MenuOption],
) -> Result<Option<usize>> {
    if options.is_empty() {
        bail!("TUI menu requires at least one option");
    }
    let mut selected: usize = 0;
    loop {
        terminal.draw(|frame| render_menu(frame, title, subtitle, options, selected))?;
        match read_menu_input()? {
            TuiInput::Up => selected = selected.saturating_sub(1),
            TuiInput::Down => selected = (selected + 1).min(options.len() - 1),
            TuiInput::Select(index) if index < options.len() => return Ok(Some(index)),
            TuiInput::Select(_) => continue,
            TuiInput::Confirm => return Ok(Some(selected)),
            TuiInput::Cancel => return Ok(None),
        }
    }
}

fn run_read_only(title: &str, footer: &str, lines: Vec<Line<'static>>) -> Result<()> {
    let _guard = TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.clear()?;
    loop {
        terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([Constraint::Min(1), Constraint::Length(1)])
                .split(area);
            let body = Paragraph::new(lines.clone())
                .block(Block::default().title(title).borders(Borders::ALL))
                .wrap(Wrap { trim: false });
            frame.render_widget(body, chunks[0]);
            frame.render_widget(Paragraph::new(footer), chunks[1]);
        })?;
        if matches!(read_menu_input()?, TuiInput::Confirm | TuiInput::Cancel) {
            break;
        }
    }
    terminal.clear()?;
    Ok(())
}

pub(crate) fn render_menu(
    frame: &mut ratatui::Frame<'_>,
    title: &str,
    subtitle: &str,
    options: &[MenuOption],
    selected: usize,
) {
    let area = frame.area();
    let compact = area.height < 18;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let mut lines = vec![
        Line::from(Span::styled(
            truncate_for_width(subtitle, area.width.saturating_sub(4) as usize),
            Style::default().fg(Color::Gray),
        )),
        Line::from(""),
    ];
    for (index, option) in options.iter().enumerate() {
        let marker = if index == selected { ">" } else { " " };
        let style = if index == selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::raw(format!("{marker} ")),
            Span::styled(format!("{}. {}", index + 1, option.label), style),
        ]));
        if !compact {
            lines.push(Line::from(Span::styled(
                format!("   {}", option.description),
                Style::default().fg(Color::DarkGray),
            )));
            lines.push(Line::from(""));
        }
    }

    let body = Paragraph::new(lines)
        .block(Block::default().title(title).borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(body, chunks[0]);
    frame.render_widget(
        Paragraph::new(if compact {
            "↑↓/jk move · Enter select · q cancel"
        } else {
            "Up/Down or j/k to move, Enter to select, Esc/q to cancel."
        })
        .wrap(Wrap { trim: true }),
        chunks[1],
    );
}

pub(crate) fn truncate_for_width(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_string();
    }
    if width <= 1 {
        return "…".to_string();
    }
    format!("{}…", value.chars().take(width - 1).collect::<String>())
}

fn read_menu_input() -> Result<TuiInput> {
    loop {
        if let Event::Key(key) = event::read()? {
            return match key.code {
                KeyCode::Up | KeyCode::Char('k') => Ok(TuiInput::Up),
                KeyCode::Down | KeyCode::Char('j') => Ok(TuiInput::Down),
                KeyCode::Enter => Ok(TuiInput::Confirm),
                KeyCode::Esc | KeyCode::Char('q') => Ok(TuiInput::Cancel),
                KeyCode::Char(value) if value.is_ascii_digit() && value != '0' => {
                    Ok(TuiInput::Select((value as usize) - ('1' as usize)))
                }
                _ => continue,
            };
        }
    }
}

#[cfg(test)]
#[allow(dead_code)]
fn scripted_menu_choice<I>(len: usize, input: &mut I) -> Result<Option<usize>>
where
    I: Iterator<Item = TuiInput>,
{
    if len == 0 {
        bail!("TUI menu requires at least one option");
    }
    let mut selected: usize = 0;
    loop {
        let Some(input) = input.next() else {
            bail!("scripted TUI input ended before a menu decision");
        };
        match input {
            TuiInput::Up => selected = selected.saturating_sub(1),
            TuiInput::Down => selected = (selected + 1).min(len - 1),
            TuiInput::Select(index) if index < len => return Ok(Some(index)),
            TuiInput::Select(_) => continue,
            TuiInput::Confirm => return Ok(Some(selected)),
            TuiInput::Cancel => return Ok(None),
        }
    }
}

fn push_report_group(lines: &mut Vec<Line<'static>>, label: &'static str, values: &[String]) {
    if values.is_empty() {
        return;
    }
    lines.push(Line::from(Span::styled(
        label,
        Style::default().add_modifier(Modifier::BOLD),
    )));
    for value in values {
        lines.push(Line::from(format!("- {value}")));
    }
    lines.push(Line::from(""));
}

pub(crate) fn interactive_terminal() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

fn is_ci() -> bool {
    env::var_os("CI").is_some()
}

fn scope_label(scope: ScopeArg) -> &'static str {
    match scope {
        ScopeArg::Global => "global",
        ScopeArg::Project => "project",
    }
}

fn target_label(target: TargetArg) -> &'static str {
    match target {
        TargetArg::Codex => "codex",
        TargetArg::Pi => "pi",
    }
}

fn update_scope_label(scope: UpdateScopeArg) -> &'static str {
    match scope {
        UpdateScopeArg::All => "all",
        UpdateScopeArg::Global => "global",
        UpdateScopeArg::Project => "project",
    }
}

pub(crate) struct MenuOption {
    label: &'static str,
    description: &'static str,
}

pub(crate) fn locale_options() -> [MenuOption; 4] {
    [
        MenuOption::new("Korean (ko-KR)", "Recommended default for Korean teams."),
        MenuOption::new("English (en-US)", "Use English for user-facing responses."),
        MenuOption::new(
            "Japanese (ja-JP)",
            "Use Japanese for user-facing responses.",
        ),
        MenuOption::new(
            "Chinese (zh-CN)",
            "Use Simplified Chinese for user-facing responses.",
        ),
    ]
}

impl MenuOption {
    fn new(label: &'static str, description: &'static str) -> Self {
        Self { label, description }
    }
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(io::stdout(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error.into());
        }
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}
