use std::{
    fs::File,
    io::Read,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};

const MAX_DOCUMENT_BYTES: u64 = 2 * 1024 * 1024;
const STATE: &str = "truespec/.state";
const CONFIG: &str = "truespec/config";

#[derive(Debug, Serialize)]
pub struct Document {
    pub path: String,
    pub label: String,
    pub group: String,
    pub kind: &'static str,
}

pub struct Snapshot {
    pub text: String,
    pub revision: String,
}

pub trait DocumentSource: Send + Sync {
    fn list(&self) -> Result<Vec<Document>>;
    fn read(&self, locator: &str) -> Result<Option<Snapshot>>;
}

pub trait SourceAdapter: Sync {
    fn id(&self) -> &'static str;
    fn label(&self) -> &'static str;
    fn default_path(&self) -> &'static Path;
    fn detect(&self, root: &Path) -> bool;
    fn open(&self, root: &Path, path: &Path) -> Result<Box<dyn DocumentSource>>;
}

struct OpenSpecAdapter;
struct MarkdownAdapter;

static OPEN_SPEC_ADAPTER: OpenSpecAdapter = OpenSpecAdapter;
static MARKDOWN_ADAPTER: MarkdownAdapter = MarkdownAdapter;
static SOURCE_ADAPTERS: [&'static dyn SourceAdapter; 2] = [&OPEN_SPEC_ADAPTER, &MARKDOWN_ADAPTER];

pub fn adapters() -> &'static [&'static dyn SourceAdapter] {
    &SOURCE_ADAPTERS
}

pub fn adapter(id: &str) -> Option<&'static dyn SourceAdapter> {
    adapters()
        .iter()
        .copied()
        .find(|adapter| adapter.id() == id)
}

pub fn detect(root: &Path) -> &'static dyn SourceAdapter {
    adapters()
        .iter()
        .copied()
        .find(|adapter| adapter.detect(root))
        .expect("the Markdown adapter is the fallback")
}

impl SourceAdapter for OpenSpecAdapter {
    fn id(&self) -> &'static str {
        "openspec"
    }

    fn label(&self) -> &'static str {
        "OpenSpec"
    }

    fn default_path(&self) -> &'static Path {
        Path::new("openspec")
    }

    fn detect(&self, root: &Path) -> bool {
        root.join(self.default_path()).is_dir()
    }

    fn open(&self, root: &Path, path: &Path) -> Result<Box<dyn DocumentSource>> {
        Ok(Box::new(OpenSpecSource(Tree::new(root, path)?)))
    }
}

impl SourceAdapter for MarkdownAdapter {
    fn id(&self) -> &'static str {
        "markdown"
    }

    fn label(&self) -> &'static str {
        "Markdown"
    }

    fn default_path(&self) -> &'static Path {
        Path::new(".")
    }

    fn detect(&self, _root: &Path) -> bool {
        true
    }

    fn open(&self, root: &Path, path: &Path) -> Result<Box<dyn DocumentSource>> {
        Ok(Box::new(MarkdownFolderSource(Tree::new(root, path)?)))
    }
}

pub fn validate_source_path(root: &Path, path: &Path) -> Result<String> {
    if path != Path::new(".") {
        check_relative(path)?;
    }
    let candidate = root.join(path);
    let canonical = candidate
        .canonicalize()
        .with_context(|| format!("Cannot find source directory {}", candidate.display()))?;
    let relative = canonical
        .strip_prefix(root)
        .context("Source path must stay inside the repository")?;
    ensure!(
        canonical.is_dir() && !excluded(relative),
        "Source path must be a readable directory outside TrueSpec state"
    );
    ensure!(
        candidate == canonical,
        "Source path must not contain symlinks or non-canonical components"
    );
    if relative.as_os_str().is_empty() {
        Ok(".".into())
    } else {
        as_locator(relative)
    }
}

pub fn check_relative(path: &Path) -> Result<()> {
    ensure!(
        !path.as_os_str().is_empty()
            && path.components().all(|c| matches!(c, Component::Normal(_))),
        "Expected a workspace-relative file path"
    );
    Ok(())
}

fn excluded(relative: &Path) -> bool {
    relative.starts_with(STATE)
        || relative.starts_with(CONFIG)
        || relative.components().any(|component| {
            let name = component.as_os_str().to_string_lossy();
            name.starts_with('.') || name == "node_modules" || name == "target"
        })
}

fn markdown(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
}

pub(crate) fn as_locator(path: &Path) -> Result<String> {
    path.components()
        .map(|c| c.as_os_str().to_str().context("Path is not UTF-8"))
        .collect::<Result<Vec<_>>>()
        .map(|parts| parts.join("/"))
}

struct Tree {
    root: PathBuf,
    base: PathBuf,
}

impl Tree {
    fn new(root: &Path, docs: &Path) -> Result<Self> {
        let base = root.join(docs).canonicalize().with_context(|| {
            format!("Cannot find document folder {}", root.join(docs).display())
        })?;
        let relative = base
            .strip_prefix(root)
            .context("Document folder must be inside the workspace")?;
        ensure!(
            base.is_dir() && !excluded(relative),
            "Document folder must be a readable source directory"
        );
        Ok(Self {
            root: root.to_owned(),
            base,
        })
    }

    fn files(&self) -> Result<Vec<PathBuf>> {
        let mut dirs = vec![self.base.clone()];
        let mut files = Vec::new();
        while let Some(dir) = dirs.pop() {
            let entries = match dir.read_dir() {
                Ok(entries) => entries,
                Err(error) if dir != self.base && error.kind() == std::io::ErrorKind::NotFound => {
                    continue; // Editor or VCS moved this folder during discovery.
                }
                Err(error) => return Err(error.into()),
            };
            for entry in entries {
                let entry = entry?;
                let path = entry.path();
                let relative = path.strip_prefix(&self.root)?;
                if excluded(relative) || relative.to_str().is_none() {
                    continue;
                }
                let kind = entry.file_type()?;
                if kind.is_dir() {
                    dirs.push(path);
                } else if kind.is_file() && markdown(&path) {
                    files.push(path);
                }
            }
        }
        files.sort();
        Ok(files)
    }

    fn read(&self, locator: &str) -> Result<Option<Snapshot>> {
        let relative = Path::new(locator);
        check_relative(relative)?;
        let path = self.root.join(relative);
        if !path.starts_with(&self.base) || excluded(relative) || !markdown(relative) {
            return Ok(None);
        }
        let canonical = match path.canonicalize() {
            Ok(canonical) => canonical,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        // In-place identities never follow symlinks, including a swapped parent directory.
        if canonical != path || !canonical.is_file() {
            return Ok(None);
        }
        let file = match File::open(&canonical) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let mut bytes = Vec::new();
        file.take(MAX_DOCUMENT_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
            bail!("Markdown documents larger than 2 MiB cannot be opened");
        }
        let revision = format!("{:x}", Sha256::digest(&bytes));
        let text = String::from_utf8(bytes).context("Document is not UTF-8 Markdown")?;
        Ok(Some(Snapshot { text, revision }))
    }

    fn document(
        &self,
        path: &Path,
        label: String,
        group: String,
        kind: &'static str,
    ) -> Result<Document> {
        Ok(Document {
            path: as_locator(path.strip_prefix(&self.root)?)?,
            label,
            group,
            kind,
        })
    }
}

struct OpenSpecSource(Tree);

impl DocumentSource for OpenSpecSource {
    fn list(&self) -> Result<Vec<Document>> {
        self.0
            .files()?
            .iter()
            .map(|path| {
                let relative = as_locator(path.strip_prefix(&self.0.base)?)?;
                let parts: Vec<_> = relative.split('/').collect();
                let (group, kind, label) = match parts.as_slice() {
                    ["specs", rest @ ..] if !rest.is_empty() => {
                        ("Current specifications".into(), "spec", spec_label(rest))
                    }
                    ["changes", "archive", change, "specs", rest @ ..] if !rest.is_empty() => {
                        (format!("archive / {change}"), "delta", spec_label(rest))
                    }
                    ["changes", "archive", change, rest @ ..] if !rest.is_empty() => {
                        (format!("archive / {change}"), "archived", rest.join("/"))
                    }
                    ["changes", change, "specs", rest @ ..] if !rest.is_empty() => {
                        (format!("change / {change}"), "delta", spec_label(rest))
                    }
                    ["changes", change, rest @ ..] if !rest.is_empty() => {
                        (format!("change / {change}"), "change", rest.join("/"))
                    }
                    _ => ("OpenSpec workspace".into(), "markdown", relative),
                };
                self.0.document(path, label, group, kind)
            })
            .collect()
    }

    fn read(&self, locator: &str) -> Result<Option<Snapshot>> {
        self.0.read(locator)
    }
}

fn spec_label(parts: &[&str]) -> String {
    match parts {
        [parents @ .., "spec.md"] if !parents.is_empty() => parents.join("/"),
        _ => parts.join("/"),
    }
}

struct MarkdownFolderSource(Tree);

impl DocumentSource for MarkdownFolderSource {
    fn list(&self) -> Result<Vec<Document>> {
        self.0
            .files()?
            .iter()
            .map(|path| {
                let relative = path.strip_prefix(&self.0.base)?;
                let group = relative
                    .parent()
                    .filter(|path| !path.as_os_str().is_empty());
                self.0.document(
                    path,
                    relative
                        .file_name()
                        .context("Expected a filename")?
                        .to_string_lossy()
                        .into(),
                    group
                        .map(as_locator)
                        .transpose()?
                        .unwrap_or_else(|| "Documents".into()),
                    "markdown",
                )
            })
            .collect()
    }

    fn read(&self, locator: &str) -> Result<Option<Snapshot>> {
        self.0.read(locator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(root: &Path, relative: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "# Example\n").unwrap();
    }

    #[test]
    fn openspec_keeps_specs_changes_deltas_and_archive_separate() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let paths = [
            (
                "openspec/specs/auth/spec.md",
                "spec",
                "Current specifications",
            ),
            ("openspec/changes/new/proposal.md", "change", "change / new"),
            (
                "openspec/changes/new/notes/sketch.md",
                "change",
                "change / new",
            ),
            (
                "openspec/changes/new/specs/auth/spec.md",
                "delta",
                "change / new",
            ),
            (
                "openspec/changes/archive/old/tasks.md",
                "archived",
                "archive / old",
            ),
            (
                "openspec/changes/archive/old/specs/auth/spec.md",
                "delta",
                "archive / old",
            ),
        ];
        for (path, _, _) in paths {
            write(&root, path);
        }
        let src = adapter("openspec")
            .unwrap()
            .open(&root, Path::new("openspec"))
            .unwrap();
        let entries = src.list().unwrap();
        assert_eq!(entries.len(), paths.len());
        for (path, kind, group) in paths {
            let entry = entries.iter().find(|entry| entry.path == path).unwrap();
            assert_eq!((entry.kind, entry.group.as_str()), (kind, group));
            if kind == "delta" {
                assert_eq!(entry.label, "auth");
            }
            assert!(src.read(path).unwrap().is_some());
        }
    }

    #[test]
    fn markdown_scope_omits_state_outside_paths_and_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        for path in [
            "docs/guide.md",
            "outside.md",
            "truespec/.state/secret.md",
            "truespec/config/sample.md",
        ] {
            write(&root, path);
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("outside.md"), root.join("docs/linked.md")).unwrap();
        let src = adapter("markdown")
            .unwrap()
            .open(&root, Path::new("docs"))
            .unwrap();
        assert_eq!(
            src.list()
                .unwrap()
                .iter()
                .map(|d| d.path.as_str())
                .collect::<Vec<_>>(),
            ["docs/guide.md"]
        );
        assert!(src.read("outside.md").unwrap().is_none());
        assert!(src.read("docs/linked.md").unwrap().is_none());
        assert!(src.read("../outside.md").is_err());
        let all = adapter("markdown")
            .unwrap()
            .open(&root, Path::new("."))
            .unwrap();
        assert_eq!(all.list().unwrap().len(), 2);
        assert!(all.read("truespec/.state/secret.md").unwrap().is_none());
    }

    #[test]
    fn source_registry_detects_and_preserves_adapter_independent_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        write(&root, "guide.md");
        assert_eq!(detect(&root).id(), "markdown");
        write(&root, "openspec/specs/guide/spec.md");
        assert_eq!(detect(&root).id(), "openspec");
        assert!(adapter("missing").is_none());

        let openspec = adapter("openspec")
            .unwrap()
            .open(&root, Path::new("openspec"))
            .unwrap();
        let markdown = adapter("markdown")
            .unwrap()
            .open(&root, Path::new("."))
            .unwrap();
        let expected = "openspec/specs/guide/spec.md";
        assert!(
            openspec
                .list()
                .unwrap()
                .iter()
                .any(|doc| doc.path == expected)
        );
        assert!(
            markdown
                .list()
                .unwrap()
                .iter()
                .any(|doc| doc.path == expected)
        );
    }
}
