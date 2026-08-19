use std::process::Command;

use serde::Serialize;

pub(super) const DEFAULT_MODE_REQUEST_USER_INPUT_FEATURE: &str = "default_mode_request_user_input";

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct CodexRuntimeFeatures {
    pub version: Option<String>,
    pub default_mode_request_user_input: bool,
}

pub(super) fn detect_runtime_features() -> CodexRuntimeFeatures {
    let Some(version_output) = command_output(["--version"]) else {
        return CodexRuntimeFeatures::default();
    };
    let Some(version) = first_output_line(&version_output) else {
        return CodexRuntimeFeatures::default();
    };
    let default_mode_request_user_input =
        command_output(["features", "list"]).is_some_and(|features| {
            feature_is_listed(&features, DEFAULT_MODE_REQUEST_USER_INPUT_FEATURE)
        });

    CodexRuntimeFeatures {
        version: Some(version),
        default_mode_request_user_input,
    }
}

fn command_output<const N: usize>(args: [&str; N]) -> Option<String> {
    let output = Command::new("codex").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let output = String::from_utf8_lossy(&output.stdout);
    (!output.trim().is_empty()).then(|| output.into_owned())
}

fn first_output_line(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToString::to_string)
}

fn feature_is_listed(features: &str, feature: &str) -> bool {
    features
        .lines()
        .any(|line| line.split_whitespace().next() == Some(feature))
}

#[cfg(test)]
mod tests {
    use super::feature_is_listed;

    #[test]
    fn finds_an_advertised_feature_after_other_rows() {
        assert!(feature_is_listed(
            "multi_agent stable true\ndefault_mode_request_user_input under development false\n",
            "default_mode_request_user_input"
        ));
    }

    #[test]
    fn ignores_partial_feature_names() {
        assert!(!feature_is_listed(
            "default_mode_request_user_input_preview under development false\n",
            "default_mode_request_user_input"
        ));
    }
}
