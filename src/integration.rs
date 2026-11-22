use std::path::{Path, PathBuf};

use anyhow::Result;

const OPENCODE_PLUGIN: &str = include_str!("../.opencode/plugins/truespec/index.ts");
const TRUESPEC_REVIEW_SKILL: &str = include_str!("../skills/truespec-review/SKILL.md");
const BINARY_PLACEHOLDER: &str = "__TRUESPEC_INSTALLED_BINARY__";

#[derive(Clone)]
pub struct InstallArtifact {
    pub destination: PathBuf,
    pub contents: Vec<u8>,
    pub label: String,
    pub protected: bool,
}

impl InstallArtifact {
    pub fn protected(
        destination: impl Into<PathBuf>,
        contents: impl Into<Vec<u8>>,
        label: impl Into<String>,
    ) -> Self {
        Self {
            destination: destination.into(),
            contents: contents.into(),
            label: label.into(),
            protected: true,
        }
    }

    pub fn managed(
        destination: impl Into<PathBuf>,
        contents: impl Into<Vec<u8>>,
        label: impl Into<String>,
    ) -> Self {
        Self {
            destination: destination.into(),
            contents: contents.into(),
            label: label.into(),
            protected: false,
        }
    }
}

pub struct InstallContext {
    pub binary: PathBuf,
    pub config_dir: PathBuf,
}

pub trait Integration: Sync {
    fn id(&self) -> &'static str;
    fn label(&self) -> &'static str;
    fn artifacts(&self, context: &InstallContext) -> Result<Vec<InstallArtifact>>;
}

struct OpenCodeIntegration;

static OPENCODE: OpenCodeIntegration = OpenCodeIntegration;
static INTEGRATIONS: [&'static dyn Integration; 1] = [&OPENCODE];

pub fn integrations() -> &'static [&'static dyn Integration] {
    &INTEGRATIONS
}

pub fn integration(id: &str) -> Option<&'static dyn Integration> {
    integrations()
        .iter()
        .copied()
        .find(|integration| integration.id() == id)
}

impl Integration for OpenCodeIntegration {
    fn id(&self) -> &'static str {
        "opencode"
    }

    fn label(&self) -> &'static str {
        "OpenCode"
    }

    fn artifacts(&self, context: &InstallContext) -> Result<Vec<InstallArtifact>> {
        let executable = context
            .binary
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("TrueSpec executable path is not UTF-8"))?;
        let encoded = serde_json::to_string(executable)?;
        let plugin = OPENCODE_PLUGIN.replace(BINARY_PLACEHOLDER, &encoded[1..encoded.len() - 1]);
        Ok(vec![
            InstallArtifact::protected(
                context
                    .config_dir
                    .join("opencode/plugins/truespec/index.ts"),
                plugin.into_bytes(),
                "OpenCode plugin",
            ),
            InstallArtifact::protected(
                context
                    .config_dir
                    .join("opencode/skills/truespec-review/SKILL.md"),
                TRUESPEC_REVIEW_SKILL.as_bytes().to_vec(),
                "OpenCode truespec-review skill",
            ),
        ])
    }
}

pub fn config_dir() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("XDG_CONFIG_HOME").filter(|path| !path.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    #[cfg(windows)]
    if let Some(path) = std::env::var_os("APPDATA").filter(|path| !path.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or_else(|| anyhow::anyhow!("Cannot find the user configuration directory"))?;
    Ok(Path::new(&home).join(".config"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_skill_has_portable_connector_only_triggers() {
        assert!(TRUESPEC_REVIEW_SKILL.starts_with(
            "---\nname: truespec-review\ndescription: Use whenever a coding-harness message"
        ));
        assert!(TRUESPEC_REVIEW_SKILL.contains("@truespec-review"));
        assert!(TRUESPEC_REVIEW_SKILL.contains("batch_id and comments payload"));
        assert!(TRUESPEC_REVIEW_SKILL.contains("Read the current files"));
        assert!(TRUESPEC_REVIEW_SKILL.contains("Answer questions"));
        assert!(!TRUESPEC_REVIEW_SKILL.contains("truespec listen"));
        assert!(!TRUESPEC_REVIEW_SKILL.contains("truespec ack"));
        assert!(!TRUESPEC_REVIEW_SKILL.contains("truespec reply"));
    }
}
