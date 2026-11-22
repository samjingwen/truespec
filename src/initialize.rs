use std::{
    collections::BTreeMap,
    fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use dialoguer::{MultiSelect, theme::ColorfulTheme};
use uuid::Uuid;

use crate::{
    integration::{self, InstallArtifact, InstallContext, Integration},
    project::ProjectConfig,
};

pub struct InitPlan {
    artifacts: Vec<InstallArtifact>,
    force: bool,
}

pub fn select_integrations(
    selected: &[String],
    all: bool,
) -> Result<Vec<&'static dyn Integration>> {
    select_integrations_with_terminal(
        selected,
        all,
        io::stdin().is_terminal() && io::stdout().is_terminal(),
    )
}

fn select_integrations_with_terminal(
    selected: &[String],
    all: bool,
    interactive: bool,
) -> Result<Vec<&'static dyn Integration>> {
    if all {
        return Ok(integration::integrations().to_vec());
    }
    if !selected.is_empty() {
        let mut resolved = BTreeMap::new();
        for id in selected {
            let integration = integration::integration(id)
                .with_context(|| format!("Unknown coding integration: {id}"))?;
            resolved.insert(integration.id(), integration);
        }
        return Ok(resolved.into_values().collect());
    }
    if !interactive {
        let available = integration::integrations()
            .iter()
            .map(|integration| integration.id())
            .collect::<Vec<_>>()
            .join(", ");
        bail!(
            "No integration selected. Available integrations: {available}\nRun `truespec init --integration opencode`."
        );
    }
    let integrations = integration::integrations();
    let labels: Vec<_> = integrations
        .iter()
        .map(|integration| integration.label())
        .collect();
    let selected = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Select coding integrations")
        .items(&labels)
        .defaults(&vec![true; labels.len()])
        .interact()?;
    ensure!(!selected.is_empty(), "Select at least one integration.");
    Ok(selected
        .into_iter()
        .map(|index| integrations[index])
        .collect())
}

impl InitPlan {
    pub fn build(
        root: &Path,
        config: &ProjectConfig,
        integrations: &[&dyn Integration],
        context: &InstallContext,
        force: bool,
    ) -> Result<Self> {
        let mut generated = vec![InstallArtifact::managed(
            ProjectConfig::path(root),
            config.contents()?,
            "TrueSpec project configuration",
        )];
        for integration in integrations {
            generated.extend(integration.artifacts(context)?);
        }
        let mut deduplicated: BTreeMap<PathBuf, InstallArtifact> = BTreeMap::new();
        for artifact in generated {
            if let Some(existing) = deduplicated.get(&artifact.destination) {
                ensure!(
                    existing.contents == artifact.contents
                        && existing.protected == artifact.protected,
                    "Generated artifacts conflict at {}",
                    artifact.destination.display()
                );
                continue;
            }
            deduplicated.insert(artifact.destination.clone(), artifact);
        }
        let artifacts: Vec<_> = deduplicated.into_values().collect();
        let mut conflicts = Vec::new();
        let mut pending = Vec::new();
        for artifact in &artifacts {
            match fs::read(&artifact.destination) {
                Ok(existing) if existing == artifact.contents => continue,
                Ok(_) if artifact.protected && !force => {
                    conflicts.push(format!(
                        "{}: {}",
                        artifact.label,
                        artifact.destination.display()
                    ));
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("Cannot inspect {}", artifact.destination.display())
                    });
                }
            }
            pending.push(artifact.clone());
        }
        if !conflicts.is_empty() {
            bail!(
                "Refusing to overwrite differing generated files:\n{}\nRe-run with --force only if you want to replace them.",
                conflicts.join("\n")
            );
        }
        Ok(Self {
            artifacts: pending,
            force,
        })
    }

    pub fn apply(self) -> Result<Vec<PathBuf>> {
        let mut changed = Vec::new();
        for artifact in self.artifacts {
            if fs::read(&artifact.destination).is_ok_and(|existing| existing == artifact.contents) {
                continue;
            }
            if artifact.protected && !self.force && artifact.destination.exists() {
                bail!(
                    "Refusing to overwrite a generated file that changed after preflight: {}",
                    artifact.destination.display()
                );
            }
            let directory = artifact
                .destination
                .parent()
                .context("Generated file has no parent directory")?;
            fs::create_dir_all(directory)
                .with_context(|| format!("Cannot create {}", directory.display()))?;
            let filename = artifact
                .destination
                .file_name()
                .context("Generated file has no filename")?
                .to_string_lossy();
            let temporary = directory.join(format!(".{filename}.{}.tmp", Uuid::new_v4()));
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .with_context(|| format!("Cannot stage {}", temporary.display()))?;
            file.write_all(&artifact.contents)?;
            file.sync_all()?;
            #[cfg(windows)]
            if artifact.destination.exists() {
                fs::remove_file(&artifact.destination)?;
            }
            fs::rename(&temporary, &artifact.destination)
                .with_context(|| format!("Cannot install {}", artifact.destination.display()))?;
            changed.push(artifact.destination);
        }
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{integration::Integration, project::ProjectConfig};

    struct FakeIntegration {
        id: &'static str,
        artifacts: Vec<InstallArtifact>,
    }

    impl Integration for FakeIntegration {
        fn id(&self) -> &'static str {
            self.id
        }

        fn label(&self) -> &'static str {
            self.id
        }

        fn artifacts(&self, _context: &InstallContext) -> Result<Vec<InstallArtifact>> {
            Ok(self.artifacts.clone())
        }
    }

    fn context(root: &Path) -> InstallContext {
        InstallContext {
            binary: root.join("truespec"),
            config_dir: root.join("user"),
        }
    }

    #[test]
    fn preflights_the_complete_plan_and_force_repairs_it() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().canonicalize().unwrap();
        fs::create_dir(root.join("docs")).unwrap();
        let config = ProjectConfig::new(&root, "markdown", Path::new("docs")).unwrap();
        let one = root.join("user/one");
        let two = root.join("user/two");
        fs::create_dir_all(one.parent().unwrap()).unwrap();
        fs::write(&one, "user one").unwrap();
        fs::write(&two, "user two").unwrap();
        let integration = FakeIntegration {
            id: "fake",
            artifacts: vec![
                InstallArtifact::protected(&one, b"generated one".to_vec(), "one"),
                InstallArtifact::protected(&two, b"generated two".to_vec(), "two"),
            ],
        };
        let error = InitPlan::build(&root, &config, &[&integration], &context(&root), false)
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains(one.to_str().unwrap()));
        assert!(error.contains(two.to_str().unwrap()));
        assert!(!ProjectConfig::path(&root).exists());
        assert_eq!(fs::read_to_string(&one).unwrap(), "user one");
        assert_eq!(fs::read_to_string(&two).unwrap(), "user two");

        let changed = InitPlan::build(&root, &config, &[&integration], &context(&root), true)
            .unwrap()
            .apply()
            .unwrap();
        assert_eq!(changed.len(), 3);
        assert_eq!(fs::read_to_string(&one).unwrap(), "generated one");
        assert_eq!(fs::read_to_string(&two).unwrap(), "generated two");
        assert_eq!(ProjectConfig::load(&root).unwrap(), config);
        assert!(
            InitPlan::build(&root, &config, &[&integration], &context(&root), false,)
                .unwrap()
                .apply()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn deduplicates_matching_artifacts_and_repairs_missing_files() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().canonicalize().unwrap();
        let config = ProjectConfig::new(&root, "markdown", Path::new(".")).unwrap();
        let destination = root.join("user/shared");
        let artifact = InstallArtifact::protected(&destination, b"generated".to_vec(), "shared");
        let first = FakeIntegration {
            id: "first",
            artifacts: vec![artifact.clone()],
        };
        let second = FakeIntegration {
            id: "second",
            artifacts: vec![artifact],
        };
        let changed = InitPlan::build(&root, &config, &[&first, &second], &context(&root), false)
            .unwrap()
            .apply()
            .unwrap();
        assert_eq!(changed.len(), 2);
        assert_eq!(fs::read_to_string(&destination).unwrap(), "generated");

        fs::remove_file(&destination).unwrap();
        let repaired = InitPlan::build(&root, &config, &[&first], &context(&root), false)
            .unwrap()
            .apply()
            .unwrap();
        assert_eq!(repaired, [destination]);
    }

    #[test]
    fn rejects_two_generated_bodies_for_one_destination() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().canonicalize().unwrap();
        let config = ProjectConfig::new(&root, "markdown", Path::new(".")).unwrap();
        let destination = root.join("shared");
        let one = FakeIntegration {
            id: "one",
            artifacts: vec![InstallArtifact::protected(
                &destination,
                b"one".to_vec(),
                "one",
            )],
        };
        let two = FakeIntegration {
            id: "two",
            artifacts: vec![InstallArtifact::protected(
                &destination,
                b"two".to_vec(),
                "two",
            )],
        };
        assert!(InitPlan::build(&root, &config, &[&one, &two], &context(&root), false).is_err());
        assert!(!destination.exists());
    }

    #[test]
    fn refuses_a_protected_file_created_after_preflight() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().canonicalize().unwrap();
        let config = ProjectConfig::new(&root, "markdown", Path::new(".")).unwrap();
        let destination = root.join("user/generated");
        let integration = FakeIntegration {
            id: "fake",
            artifacts: vec![InstallArtifact::protected(
                &destination,
                b"generated".to_vec(),
                "generated",
            )],
        };
        let plan =
            InitPlan::build(&root, &config, &[&integration], &context(&root), false).unwrap();
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(&destination, "user file").unwrap();
        assert!(plan.apply().is_err());
        assert_eq!(fs::read_to_string(destination).unwrap(), "user file");
    }

    #[test]
    fn resolves_duplicate_all_unknown_and_missing_integration_selection() {
        let selected = select_integrations_with_terminal(
            &["opencode".into(), "opencode".into()],
            false,
            false,
        )
        .unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id(), "opencode");
        assert_eq!(
            select_integrations_with_terminal(&[], true, false)
                .unwrap()
                .len(),
            integration::integrations().len()
        );
        assert!(select_integrations_with_terminal(&["missing".into()], false, false).is_err());
        assert!(select_integrations_with_terminal(&[], false, false).is_err());
    }
}
