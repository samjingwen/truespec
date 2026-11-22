use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::source;

const CONFIG_VERSION: u8 = 1;
const CONFIG_PATH: &str = "truespec/config/project.json";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectConfig {
    version: u8,
    source: SourceConfig,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceConfig {
    adapter: String,
    path: String,
}

impl ProjectConfig {
    pub fn initialize(
        root: &Path,
        requested_adapter: Option<&str>,
        requested_path: Option<&Path>,
    ) -> Result<Self> {
        if requested_adapter.is_none() && requested_path.is_none() && Self::path(root).is_file() {
            return Self::load(root);
        }
        let existing = Self::load(root).ok();
        let adapter = requested_adapter
            .and_then(source::adapter)
            .or_else(|| {
                requested_adapter
                    .is_none()
                    .then(|| existing.as_ref().map(Self::adapter))
                    .flatten()
            })
            .unwrap_or_else(|| source::detect(root));
        if let Some(id) = requested_adapter {
            ensure!(adapter.id() == id, "Unknown document source adapter: {id}");
        }
        let path = requested_path
            .or_else(|| {
                (requested_adapter.is_none())
                    .then(|| existing.as_ref().map(Self::source_path))
                    .flatten()
            })
            .unwrap_or_else(|| adapter.default_path());
        Self::new(root, adapter.id(), path)
    }

    pub fn repository_root() -> Result<PathBuf> {
        let root = std::env::current_dir()?.canonicalize()?;
        ensure!(
            root.join(".git").symlink_metadata().is_ok(),
            "Run `truespec init` from the repository root."
        );
        Ok(root)
    }

    pub fn new(root: &Path, adapter: &str, path: &Path) -> Result<Self> {
        ensure!(
            source::adapter(adapter).is_some(),
            "Unknown document source adapter: {adapter}"
        );
        let path = source::validate_source_path(root, path)?;
        Ok(Self {
            version: CONFIG_VERSION,
            source: SourceConfig {
                adapter: adapter.into(),
                path,
            },
        })
    }

    pub fn load(root: &Path) -> Result<Self> {
        let path = Self::path(root);
        let bytes = fs::read(&path).with_context(|| format!("Cannot read {}", path.display()))?;
        let config: Self = serde_json::from_slice(&bytes).with_context(|| {
            format!("Invalid TrueSpec project configuration {}", path.display())
        })?;
        config.validate(root)?;
        Ok(config)
    }

    #[cfg(test)]
    pub fn save(&self, root: &Path) -> Result<()> {
        self.validate(root)?;
        let path = Self::path(root);
        fs::create_dir_all(
            path.parent()
                .context("Project configuration path has no parent")?,
        )?;
        fs::write(&path, self.contents()?)
            .with_context(|| format!("Cannot write {}", path.display()))
    }

    pub fn contents(&self) -> Result<Vec<u8>> {
        let mut bytes = serde_json::to_vec_pretty(self)?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    pub fn path(root: &Path) -> PathBuf {
        root.join(CONFIG_PATH)
    }

    pub fn adapter(&self) -> &'static dyn source::SourceAdapter {
        source::adapter(&self.source.adapter).expect("validated project source adapter")
    }

    pub fn source_path(&self) -> &Path {
        Path::new(&self.source.path)
    }

    fn validate(&self, root: &Path) -> Result<()> {
        ensure!(
            self.version == CONFIG_VERSION,
            "Unsupported TrueSpec project configuration version {}",
            self.version
        );
        ensure!(
            source::adapter(&self.source.adapter).is_some(),
            "Unknown document source adapter: {}",
            self.source.adapter
        );
        let validated = source::validate_source_path(root, Path::new(&self.source.path))?;
        ensure!(
            validated == self.source.path,
            "Project source path must be canonical"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn saves_and_loads_versioned_source_configuration() {
        let fixture = root();
        let root = fixture.path().canonicalize().unwrap();
        fs::create_dir(root.join("openspec")).unwrap();
        let config = ProjectConfig::new(&root, "openspec", Path::new("openspec")).unwrap();
        config.save(&root).unwrap();
        assert_eq!(ProjectConfig::load(&root).unwrap(), config);
        assert_eq!(config.adapter().id(), "openspec");
        assert_eq!(config.source_path(), Path::new("openspec"));
        assert!(
            String::from_utf8(config.contents().unwrap())
                .unwrap()
                .ends_with('\n')
        );
    }

    #[test]
    fn rejects_invalid_versions_adapters_and_paths_without_rewriting() {
        let fixture = root();
        let root = fixture.path().canonicalize().unwrap();
        fs::create_dir_all(root.join("truespec/config")).unwrap();
        fs::write(
            ProjectConfig::path(&root),
            r#"{"version":2,"source":{"adapter":"markdown","path":"."}}"#,
        )
        .unwrap();
        assert!(ProjectConfig::load(&root).is_err());
        assert!(ProjectConfig::new(&root, "missing", Path::new(".")).is_err());
        assert!(ProjectConfig::new(&root, "markdown", Path::new("missing")).is_err());
        fs::create_dir_all(root.join("truespec/.state")).unwrap();
        assert!(ProjectConfig::new(&root, "markdown", Path::new("truespec/.state")).is_err());
        assert_eq!(
            fs::read_to_string(ProjectConfig::path(&root)).unwrap(),
            r#"{"version":2,"source":{"adapter":"markdown","path":"."}}"#
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_source_symlinks_that_escape_the_repository() {
        let fixture = root();
        let outside = root();
        let root = fixture.path().canonicalize().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("linked")).unwrap();
        assert!(ProjectConfig::new(&root, "markdown", Path::new("linked")).is_err());
    }

    #[test]
    fn detects_sources_and_keeps_existing_configuration() {
        let fixture = root();
        let root = fixture.path().canonicalize().unwrap();
        assert_eq!(
            ProjectConfig::initialize(&root, None, None)
                .unwrap()
                .adapter()
                .id(),
            "markdown"
        );
        fs::create_dir(root.join("openspec")).unwrap();
        let detected = ProjectConfig::initialize(&root, None, None).unwrap();
        assert_eq!(detected.adapter().id(), "openspec");
        detected.save(&root).unwrap();
        fs::remove_dir(root.join("openspec")).unwrap();
        assert!(ProjectConfig::initialize(&root, None, None).is_err());
    }
}
