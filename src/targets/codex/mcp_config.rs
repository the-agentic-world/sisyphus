use std::{
    fs,
    ops::Range,
    path::{Path, PathBuf},
};

use crate::installer::ManagedTomlEdit;
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use toml_edit::{value, Array, DocumentMut, ImDocument, Item, Table, Value as EditValue};

const MCP_HASH_PREFIX: &str = "# MEGARA:MCP-SHA256=";
pub(super) const DEFAULT_MODE_REQUEST_USER_INPUT: &str = "default_mode_request_user_input";
pub(super) const DEFAULT_MODE_REQUEST_USER_INPUT_MARKER: &str =
    "# MEGARA:DEFAULT-MODE-REQUEST-USER-INPUT";

#[derive(Clone, Copy)]
struct RenderOptions {
    force: bool,
    runtime_supports_default_mode_request_user_input: bool,
}

pub(super) fn plan(
    root: &Path,
    executable: &Path,
    force: bool,
    runtime_supports_default_mode_request_user_input: bool,
) -> Result<ManagedTomlEdit> {
    let path = root.join("config.toml");
    let (source, existing, permissions) = match read_existing(&path)? {
        Some((source, existing, permissions)) => (Some(source), Some(existing), Some(permissions)),
        None => (None, None, None),
    };
    let project_root = root
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.to_path_buf())
        .canonicalize()
        .unwrap_or_else(|_| root.to_path_buf());
    render(
        &path,
        source.as_deref(),
        existing.as_deref(),
        permissions,
        executable,
        &project_root,
        RenderOptions {
            force,
            runtime_supports_default_mode_request_user_input,
        },
    )
}

pub(super) fn plan_remove(root: &Path, force: bool) -> Result<Option<ManagedTomlEdit>> {
    let path = root.join("config.toml");
    let Some((source, existing, permissions)) = read_existing(&path)? else {
        return Ok(None);
    };
    let (without_hash, stored_hash) = remove_hash_line(&existing);
    let (without_feature_marker, feature_marker_present) =
        remove_marker_line(&without_hash, DEFAULT_MODE_REQUEST_USER_INPUT_MARKER);
    let mut document: DocumentMut = without_feature_marker
        .parse()
        .context("failed to parse Codex config TOML")?;
    let Some(servers) = document.get_mut("mcp_servers").and_then(Item::as_table_mut) else {
        return remove_stale_feature_setting(
            &path,
            source,
            permissions,
            document,
            stored_hash,
            feature_marker_present,
        );
    };
    let Some(current) = servers.get("megara_planning") else {
        return remove_stale_feature_setting(
            &path,
            source,
            permissions,
            document,
            stored_hash,
            feature_marker_present,
        );
    };
    let current_hash = item_hash(current)?;
    let managed = stored_hash.as_deref() == Some(current_hash.as_str());
    if !managed && !force {
        bail!("refusing to remove edited or unmanaged megara_planning MCP table");
    }
    let backup = (force && !managed)
        .then(|| table_backup(&existing))
        .transpose()?;
    servers.remove("megara_planning");
    configure_default_mode_request_user_input(&mut document, false, feature_marker_present)?;
    let desired = document.to_string();
    Ok(Some(ManagedTomlEdit {
        path: path.clone(),
        created: false,
        changed: desired.as_bytes() != source.as_slice(),
        backup_path: backup.as_ref().map(|_| backup_path(&path)),
        desired,
        backup,
        expected_source: Some(source),
        permissions: Some(permissions),
    }))
}

fn remove_stale_feature_setting(
    path: &Path,
    source: Vec<u8>,
    permissions: fs::Permissions,
    mut document: DocumentMut,
    stored_hash: Option<String>,
    feature_marker_present: bool,
) -> Result<Option<ManagedTomlEdit>> {
    if !feature_marker_present {
        return Ok(None);
    }
    configure_default_mode_request_user_input(&mut document, false, true)?;
    let mut desired = document.to_string();
    if let Some(stored_hash) = stored_hash {
        if !desired.ends_with('\n') {
            desired.push('\n');
        }
        desired.push_str(MCP_HASH_PREFIX);
        desired.push_str(&stored_hash);
        desired.push('\n');
    }
    Ok(Some(ManagedTomlEdit {
        path: path.to_path_buf(),
        created: false,
        changed: desired.as_bytes() != source.as_slice(),
        backup_path: None,
        desired,
        backup: None,
        expected_source: Some(source),
        permissions: Some(permissions),
    }))
}

fn render(
    path: &Path,
    source: Option<&[u8]>,
    existing: Option<&str>,
    permissions: Option<fs::Permissions>,
    executable: &Path,
    project_root: &Path,
    options: RenderOptions,
) -> Result<ManagedTomlEdit> {
    let base = existing.unwrap_or("# Megara Codex projection.\n");
    let (without_hash, stored_hash) = remove_hash_line(base);
    let (without_feature_marker, feature_marker_present) =
        remove_marker_line(&without_hash, DEFAULT_MODE_REQUEST_USER_INPUT_MARKER);
    let mut document: DocumentMut = without_feature_marker
        .parse()
        .context("failed to parse Codex config TOML")?;
    let current = document
        .get("mcp_servers")
        .and_then(Item::as_table)
        .and_then(|servers| servers.get("megara_planning"));
    let current_hash = current.map(item_hash).transpose()?;
    let unmanaged = current_hash.as_deref() != stored_hash.as_deref();
    if current.is_some() && unmanaged && !options.force {
        bail!("unmanaged or directly edited megara_planning MCP table; rerun with --force");
    }
    let backup = if current.is_some() && unmanaged && options.force {
        Some(table_backup(base)?)
    } else {
        None
    };
    let servers = document["mcp_servers"].or_insert(Item::Table(Table::new()));
    let servers = servers
        .as_table_mut()
        .context("mcp_servers must be a TOML table")?;
    servers.insert("megara_planning", desired_item(executable, project_root));
    let desired_hash = item_hash(
        servers
            .get("megara_planning")
            .expect("MCP table was inserted"),
    )?;
    let (_default_mode_request_user_input, feature_is_managed) =
        configure_default_mode_request_user_input(
            &mut document,
            options.runtime_supports_default_mode_request_user_input,
            feature_marker_present,
        )?;
    let mut desired = document.to_string();
    if !desired.ends_with('\n') {
        desired.push('\n');
    }
    if feature_is_managed {
        desired.push_str(DEFAULT_MODE_REQUEST_USER_INPUT_MARKER);
        desired.push('\n');
    }
    desired.push_str(MCP_HASH_PREFIX);
    desired.push_str(&desired_hash);
    desired.push('\n');
    let changed = desired.as_bytes() != base.as_bytes();
    Ok(ManagedTomlEdit {
        path: path.to_path_buf(),
        created: existing.is_none(),
        changed,
        backup_path: backup.as_ref().map(|_| backup_path(path)),
        desired,
        backup,
        expected_source: source.map(ToOwned::to_owned),
        permissions,
    })
}

fn desired_item(executable: &Path, project_root: &Path) -> Item {
    let mut table = Table::new();
    table.insert("command", value(executable.display().to_string()));
    let mut args = Array::default();
    args.push("planning");
    args.push("mcp");
    args.push("--project");
    args.push(project_root.display().to_string());
    table.insert("args", Item::Value(EditValue::Array(args)));
    table.insert("cwd", value(project_root.display().to_string()));
    table.insert("enabled", value(true));
    table.insert("startup_timeout_sec", value(10));
    table.insert("tool_timeout_sec", value(120));
    let mut tools = Table::new();
    for name in [
        "planning_spec_approve",
        "planning_plan_approve",
        "planning_purge",
    ] {
        let mut config = Table::new();
        config.insert("approval_mode", value("prompt"));
        tools.insert(name, Item::Table(config));
    }
    table.insert("tools", Item::Table(tools));
    Item::Table(table)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DefaultModeRequestUserInputSetting {
    Missing,
    Enabled,
    DisabledOrInvalid,
}

fn default_mode_request_user_input_setting(
    document: &DocumentMut,
) -> DefaultModeRequestUserInputSetting {
    let Some(features) = document.get("features").and_then(Item::as_table_like) else {
        return DefaultModeRequestUserInputSetting::Missing;
    };
    let Some(value) = features.get(DEFAULT_MODE_REQUEST_USER_INPUT) else {
        return DefaultModeRequestUserInputSetting::Missing;
    };
    if value
        .as_value()
        .and_then(EditValue::as_bool)
        .is_some_and(|enabled| enabled)
    {
        DefaultModeRequestUserInputSetting::Enabled
    } else {
        DefaultModeRequestUserInputSetting::DisabledOrInvalid
    }
}

pub(super) fn configure_default_mode_request_user_input(
    document: &mut DocumentMut,
    runtime_supports_default_mode_request_user_input: bool,
    feature_marker_present: bool,
) -> Result<(bool, bool)> {
    let setting = default_mode_request_user_input_setting(document);
    if !runtime_supports_default_mode_request_user_input {
        if feature_marker_present && setting == DefaultModeRequestUserInputSetting::Enabled {
            let features = document
                .get_mut("features")
                .and_then(Item::as_table_like_mut)
                .context("features must be a TOML table")?;
            features.remove(DEFAULT_MODE_REQUEST_USER_INPUT);
        }
        return Ok((false, false));
    }

    match setting {
        DefaultModeRequestUserInputSetting::Enabled => Ok((true, feature_marker_present)),
        DefaultModeRequestUserInputSetting::DisabledOrInvalid => Ok((false, false)),
        DefaultModeRequestUserInputSetting::Missing => {
            let features = document["features"].or_insert(Item::Table(Table::new()));
            let features = features
                .as_table_like_mut()
                .context("features must be a TOML table")?;
            features.insert(DEFAULT_MODE_REQUEST_USER_INPUT, value(true));
            Ok((true, true))
        }
    }
}

fn item_hash(item: &Item) -> Result<String> {
    let mut document = DocumentMut::new();
    let mut servers = Table::new();
    servers.insert("megara_planning", item.clone());
    document["mcp_servers"] = Item::Table(servers);
    let mut hasher = Sha256::new();
    hasher.update(document.to_string().as_bytes());
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn table_backup(content: &str) -> Result<Vec<u8>> {
    let document: ImDocument<&str> =
        ImDocument::parse(content).context("failed to parse Codex config TOML for table backup")?;
    let Some(servers) = document.get("mcp_servers").and_then(Item::as_table) else {
        bail!("cannot isolate managed megara_planning TOML table for backup")
    };
    let Some(target) = servers.get("megara_planning") else {
        bail!("cannot isolate managed megara_planning TOML table for backup")
    };

    let mut spans = Vec::new();
    if let Some(span) = servers.key("megara_planning").and_then(|key| key.span()) {
        spans.push(span);
    }
    collect_spans(target, &mut spans);
    let ranges = source_line_ranges(content, spans);
    if ranges.is_empty() {
        bail!("cannot isolate managed megara_planning TOML table for backup")
    }
    let mut backup = Vec::new();
    for range in ranges {
        backup.extend_from_slice(content.as_bytes().get(range).context(
            "toml_edit returned an invalid span for managed megara_planning TOML table",
        )?);
    }
    Ok(backup)
}

fn collect_spans(item: &Item, spans: &mut Vec<Range<usize>>) {
    if let Some(span) = item.span() {
        spans.push(span);
    }
    match item {
        Item::Table(table) => {
            for (key, value) in table.iter() {
                if let Some(span) = table.key(key).and_then(|key| key.span()) {
                    spans.push(span);
                }
                collect_spans(value, spans);
            }
        }
        Item::Value(value) => {
            if let Some(table) = value.as_inline_table() {
                for (key, value) in table.iter() {
                    if let Some(span) = table.key(key).and_then(|key| key.span()) {
                        spans.push(span);
                    }
                    if let Some(span) = value.span() {
                        spans.push(span);
                    }
                }
            }
        }
        Item::None | Item::ArrayOfTables(_) => {}
    }
}

fn source_line_ranges(source: &str, spans: Vec<Range<usize>>) -> Vec<Range<usize>> {
    let mut ranges = spans
        .into_iter()
        .filter(|span| span.start <= span.end && span.end <= source.len())
        .map(|span| {
            let start = source[..span.start]
                .rfind('\n')
                .map_or(0, |offset| offset + 1);
            let end = source[span.end..]
                .find('\n')
                .map_or(source.len(), |offset| span.end + offset + 1);
            start..end
        })
        .collect::<Vec<_>>();
    ranges.sort_by_key(|range| (range.start, range.end));

    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        if let Some(last) = merged.last_mut() {
            if range.start <= last.end {
                last.end = last.end.max(range.end);
                continue;
            }
        }
        merged.push(range);
    }
    merged
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_file_name("config.toml.megara.mcp.bak")
}

fn remove_hash_line(content: &str) -> (String, Option<String>) {
    let mut result = String::new();
    let mut hash = None;
    for segment in content.split_inclusive('\n') {
        let line = segment.trim_end_matches('\n').trim_end_matches('\r');
        if let Some(value) = line.strip_prefix(MCP_HASH_PREFIX) {
            hash = Some(value.trim().to_string());
        } else {
            result.push_str(segment);
        }
    }
    (result, hash)
}

pub(super) fn remove_marker_line(content: &str, marker: &str) -> (String, bool) {
    let mut result = String::new();
    let mut found = false;
    for segment in content.split_inclusive('\n') {
        let line = segment.trim_end_matches('\n').trim_end_matches('\r');
        if line == marker {
            found = true;
        } else {
            result.push_str(segment);
        }
    }
    (result, found)
}

fn read_existing(path: &Path) -> Result<Option<(Vec<u8>, String, fs::Permissions)>> {
    let source = match fs::read(path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to read Codex config {}", path.display()))
        }
    };
    let content = String::from_utf8(source.clone())
        .with_context(|| format!("Codex config is not valid UTF-8: {}", path.display()))?;
    let permissions = fs::metadata(path)
        .with_context(|| format!("failed to stat Codex config {}", path.display()))?
        .permissions();
    Ok(Some((source, content, permissions)))
}
