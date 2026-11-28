use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, RwLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior::Immediate, params};
use sha2::{Digest, Sha256};

use crate::{
    project::ProjectConfig,
    review::Store,
    service::{OwnerLock, ServicePaths},
    source::DocumentSource,
};

const REGISTRY_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS workspaces (
    id TEXT PRIMARY KEY NOT NULL,
    canonical_root TEXT UNIQUE NOT NULL,
    config_fingerprint TEXT NOT NULL,
    available INTEGER NOT NULL DEFAULT 1 CHECK (available IN (0, 1)),
    registered_at INTEGER NOT NULL,
    checked_at INTEGER NOT NULL
);
PRAGMA user_version = 1;
";

#[derive(Debug)]
pub enum ProjectProblem {
    Missing,
    Unavailable,
}

impl std::fmt::Display for ProjectProblem {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing => formatter.write_str("Workspace is not registered"),
            Self::Unavailable => formatter.write_str("Registered workspace is unavailable"),
        }
    }
}

impl std::error::Error for ProjectProblem {}

#[derive(Debug)]
pub struct DuplicateWorkspaceIdentity;

impl std::fmt::Display for DuplicateWorkspaceIdentity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(
            "Workspace state is already registered at another available repository root. Remove or reset the copied local state before retrying.",
        )
    }
}

impl std::error::Error for DuplicateWorkspaceIdentity {}

pub struct ProjectContext {
    pub id: String,
    pub root: PathBuf,
    pub config_fingerprint: String,
    pub store: Store,
    pub source: Arc<dyn DocumentSource>,
    pub source_name: &'static str,
}

impl ProjectContext {
    fn open(root: &Path, expected_id: Option<&str>, create: bool) -> Result<Self> {
        let root = canonical_repository(root)?;
        let config = ProjectConfig::load(&root)?;
        let source = config.adapter().open(&root, config.source_path())?;
        let store = Store::open(&root, create)?;
        let id = store.workspace_id()?;
        if let Some(expected) = expected_id {
            ensure!(
                id == expected,
                "Registered workspace identity does not match repository state"
            );
        }
        Ok(Self {
            id,
            root,
            config_fingerprint: config_fingerprint(&config)?,
            store,
            source: source.into(),
            source_name: config.adapter().label(),
        })
    }
}

#[derive(Clone)]
pub struct ProjectRegistry {
    routing: Arc<Mutex<()>>,
    routing_generation: Arc<AtomicU64>,
    connection: Arc<Mutex<Connection>>,
    stores: Arc<Mutex<HashMap<String, Store>>>,
    contexts: Arc<RwLock<HashMap<String, Arc<ProjectContext>>>>,
}

impl ProjectRegistry {
    pub fn open(paths: &ServicePaths) -> Result<Self> {
        paths.ensure()?;
        let database = paths.registry();
        if std::fs::symlink_metadata(&database).is_ok() {
            ensure!(
                database.canonicalize()? == database && database.is_file(),
                "Invalid TrueSpec service registry path"
            );
        }
        let connection = open_connection(&database, true)?;
        let version =
            connection.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?;
        ensure!(
            version == 0 || version == 1,
            "Unsupported TrueSpec registry version {version}"
        );
        connection.execute_batch(REGISTRY_SCHEMA)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&database, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(Self {
            routing: Arc::new(Mutex::new(())),
            routing_generation: Arc::new(AtomicU64::new(0)),
            connection: Arc::new(Mutex::new(connection)),
            stores: Arc::new(Mutex::new(HashMap::new())),
            contexts: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub fn register(&self, root: &Path) -> Result<Arc<ProjectContext>> {
        let generation = self.routing_generation.load(Ordering::Acquire);
        let _routing = self.routing()?;
        self.ensure_routing_generation(generation)?;
        let root = canonical_repository(root)?;
        let root_text = path_text(&root)?;
        let registered_root_id = self
            .connection()?
            .query_row(
                "SELECT id FROM workspaces WHERE canonical_root = ?1",
                [root_text],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let mut context = ProjectContext::open(
            &root,
            registered_root_id.as_deref(),
            registered_root_id.is_none(),
        )?;
        let root_text = path_text(&context.root)?;
        let now = crate::review::now();
        let mut connection = self.connection()?;
        let tx = connection.transaction_with_behavior(Immediate)?;
        let by_id = tx
            .query_row(
                "SELECT canonical_root FROM workspaces WHERE id = ?1",
                [&context.id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if let Some(existing) = by_id.as_deref()
            && existing != root_text
            && registered_root_available(Path::new(existing), &context.id)
        {
            return Err(DuplicateWorkspaceIdentity.into());
        }
        let root_owner = tx
            .query_row(
                "SELECT id FROM workspaces WHERE canonical_root = ?1",
                [root_text],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if root_owner
            .as_deref()
            .is_some_and(|owner| owner != context.id)
        {
            anyhow::bail!("Repository root is registered with a different workspace identity");
        }
        if by_id.is_some() {
            tx.execute(
                "UPDATE workspaces
                 SET canonical_root = ?2, config_fingerprint = ?3, available = 1, checked_at = ?4
                 WHERE id = ?1",
                params![context.id, root_text, context.config_fingerprint, now],
            )?;
        } else {
            tx.execute(
                "INSERT INTO workspaces
                 (id, canonical_root, config_fingerprint, available, registered_at, checked_at)
                 VALUES (?1, ?2, ?3, 1, ?4, ?4)",
                params![context.id, root_text, context.config_fingerprint, now],
            )?;
        }
        tx.commit()?;
        drop(connection);
        self.publish_store(&mut context)?;
        let context = Arc::new(context);
        self.contexts
            .write()
            .map_err(|_| anyhow::anyhow!("Project context cache is unavailable"))?
            .insert(context.id.clone(), context.clone());
        Ok(context)
    }

    pub fn resolve_registered(&self, root: &Path) -> Result<String> {
        let generation = self.routing_generation.load(Ordering::Acquire);
        let _routing = self.routing()?;
        self.ensure_routing_generation(generation)?;
        let root = canonical_repository(root)?;
        let root_text = path_text(&root)?;
        let connection = self.connection()?;
        let id = connection
            .query_row(
                "SELECT id FROM workspaces WHERE canonical_root = ?1",
                [root_text],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .context("Workspace root is not registered")?;
        drop(connection);
        self.store_at(&id, &root)?;
        Ok(id)
    }

    pub(crate) fn forget(&self, root: &Path, _owner: &OwnerLock) -> Result<Option<String>> {
        let _routing = self.routing()?;
        let root = canonical_repository(root)?;
        let root_text = path_text(&root)?;
        let connection = self.connection()?;
        let id = connection
            .query_row(
                "SELECT id FROM workspaces WHERE canonical_root = ?1",
                [root_text],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if id.is_some() {
            connection.execute(
                "DELETE FROM workspaces WHERE canonical_root = ?1",
                [root_text],
            )?;
        }
        drop(connection);
        if let Some(id) = &id {
            self.stores
                .lock()
                .map_err(|_| anyhow::anyhow!("Project store cache is unavailable"))?
                .remove(id);
            self.evict(id)?;
        }
        self.routing_generation.fetch_add(1, Ordering::AcqRel);
        Ok(id)
    }

    pub fn store(&self, id: &str) -> Result<Store> {
        let generation = self.routing_generation.load(Ordering::Acquire);
        let _routing = self.routing()?;
        self.ensure_routing_generation(generation)?;
        let root = {
            let connection = self.connection()?;
            connection
                .query_row(
                    "SELECT canonical_root FROM workspaces WHERE id = ?1",
                    [id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .ok_or(ProjectProblem::Missing)?
        };
        let root = canonical_repository(Path::new(&root))?;
        self.store_at(id, &root)
    }

    pub fn project(&self, id: &str) -> Result<Arc<ProjectContext>> {
        let generation = self.routing_generation.load(Ordering::Acquire);
        let _routing = self.routing()?;
        self.ensure_routing_generation(generation)?;
        if let Some(cached) = self
            .contexts
            .read()
            .map_err(|_| anyhow::anyhow!("Project context cache is unavailable"))?
            .get(id)
            .cloned()
            && current_fingerprint(&cached.root).as_deref()
                == Some(cached.config_fingerprint.as_str())
            && cached.store.workspace_id().ok().as_deref() == Some(id)
            && self.mapping_matches(id, &cached.root)?
        {
            return Ok(cached);
        }
        let record = self
            .connection()?
            .query_row(
                "SELECT canonical_root, config_fingerprint FROM workspaces WHERE id = ?1",
                [id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
            .ok_or(ProjectProblem::Missing)?;
        let mut loaded = match ProjectContext::open(Path::new(&record.0), Some(id), false) {
            Ok(context) => context,
            Err(error) => {
                self.mark_unavailable(id)?;
                return Err(error.context(ProjectProblem::Unavailable));
            }
        };
        let updated = self.connection()?.execute(
            "UPDATE workspaces
             SET config_fingerprint = ?2, available = 1, checked_at = ?3 WHERE id = ?1",
            params![id, loaded.config_fingerprint, crate::review::now()],
        )?;
        ensure!(
            updated == 1,
            "Workspace registration disappeared while loading"
        );
        self.publish_store(&mut loaded)?;
        let loaded = Arc::new(loaded);
        self.contexts
            .write()
            .map_err(|_| anyhow::anyhow!("Project context cache is unavailable"))?
            .insert(id.to_owned(), loaded.clone());
        Ok(loaded)
    }

    pub fn counts(&self) -> Result<(u64, Option<u64>)> {
        let connection = self.connection()?;
        let projects: i64 =
            connection.query_row("SELECT COUNT(*) FROM workspaces", [], |row| row.get(0))?;
        drop(connection);
        let stores = self
            .stores
            .lock()
            .map_err(|_| anyhow::anyhow!("Project store cache is unavailable"))?;
        let open_reviews = stores
            .values()
            .filter(|store| store.has_open_review().unwrap_or(false))
            .count() as u64;
        let projects = u64::try_from(projects)?;
        let complete = usize::try_from(projects).ok() == Some(stores.len());
        Ok((projects, complete.then_some(open_reviews)))
    }

    pub fn evict(&self, id: &str) -> Result<()> {
        self.contexts
            .write()
            .map_err(|_| anyhow::anyhow!("Project context cache is unavailable"))?
            .remove(id);
        Ok(())
    }

    #[cfg(test)]
    pub fn contains_review_state(&self) -> Result<bool> {
        let connection = self.connection()?;
        let mut statement =
            connection.prepare("SELECT name FROM sqlite_master WHERE type = 'table'")?;
        let tables = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(tables.iter().any(|table| {
            matches!(
                table.as_str(),
                "documents"
                    | "sessions"
                    | "comments"
                    | "batches"
                    | "batch_items"
                    | "review_connectors"
                    | "agent_replies"
                    | "review_feed"
            )
        }))
    }

    fn mark_unavailable(&self, id: &str) -> Result<()> {
        self.connection()?.execute(
            "UPDATE workspaces SET available = 0, checked_at = ?2 WHERE id = ?1",
            params![id, crate::review::now()],
        )?;
        self.evict(id)
    }

    fn store_at(&self, id: &str, root: &Path) -> Result<Store> {
        let mut stores = self
            .stores
            .lock()
            .map_err(|_| anyhow::anyhow!("Project store cache is unavailable"))?;
        if let Some(store) = stores.get(id)
            && store.workspace_id().ok().as_deref() == Some(id)
        {
            return Ok(store.clone());
        }
        stores.remove(id);
        let store = Store::open(root, false)?;
        ensure!(
            store.workspace_id()? == id,
            "Registered workspace identity does not match repository state"
        );
        stores.insert(id.to_owned(), store.clone());
        Ok(store)
    }

    fn publish_store(&self, context: &mut ProjectContext) -> Result<()> {
        let mut stores = self
            .stores
            .lock()
            .map_err(|_| anyhow::anyhow!("Project store cache is unavailable"))?;
        if let Some(store) = stores.get(&context.id)
            && store.is_for_root(&context.root)
            && store.workspace_id().ok().as_deref() == Some(context.id.as_str())
        {
            context.store = store.clone();
        }
        stores.insert(context.id.clone(), context.store.clone());
        Ok(())
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| anyhow::anyhow!("TrueSpec registry connection is unavailable"))
    }

    fn routing(&self) -> Result<MutexGuard<'_, ()>> {
        self.routing
            .lock()
            .map_err(|_| anyhow::anyhow!("TrueSpec routing state is unavailable"))
    }

    fn ensure_routing_generation(&self, expected: u64) -> Result<()> {
        ensure!(
            self.routing_generation.load(Ordering::Acquire) == expected,
            "Workspace routing changed while the request was waiting; retry"
        );
        Ok(())
    }

    fn mapping_matches(&self, id: &str, root: &Path) -> Result<bool> {
        let root = path_text(root)?;
        self.connection()?
            .query_row(
                "SELECT EXISTS (
                    SELECT 1 FROM workspaces WHERE id = ?1 AND canonical_root = ?2
                 )",
                params![id, root],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }
}

fn open_connection(database: &Path, create: bool) -> Result<Connection> {
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_FULL_MUTEX;
    let flags = if create {
        flags | OpenFlags::SQLITE_OPEN_CREATE
    } else {
        flags
    };
    let connection = Connection::open_with_flags(database, flags)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    if create {
        connection.pragma_update(None, "journal_mode", "WAL")?;
    }
    Ok(connection)
}

fn canonical_repository(root: &Path) -> Result<PathBuf> {
    let root = root
        .canonicalize()
        .with_context(|| format!("Cannot find workspace {}", root.display()))?;
    ensure!(root.is_dir(), "Workspace must be a directory");
    let marker = root.join(".git");
    let metadata =
        std::fs::symlink_metadata(&marker).context("Workspace must be a Git repository root")?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "Git repository marker must not be a symlink"
    );
    Ok(root)
}

fn config_fingerprint(config: &ProjectConfig) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(config.contents()?)))
}

fn current_fingerprint(root: &Path) -> Option<String> {
    ProjectConfig::load(root)
        .and_then(|config| config_fingerprint(&config))
        .ok()
}

fn path_text(path: &Path) -> Result<&str> {
    path.to_str()
        .context("Workspace path must be valid UTF-8 for service registration")
}

fn registered_root_available(root: &Path, expected_id: &str) -> bool {
    canonical_repository(root)
        .and_then(|root| Store::open(&root, false))
        .and_then(|store| store.workspace_id())
        .is_ok_and(|id| id == expected_id)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn service_paths(root: &Path) -> ServicePaths {
        let root = root.canonicalize().unwrap();
        ServicePaths {
            config_dir: root.join("config/truespec"),
            state_dir: root.join("state/truespec"),
            data_dir: root.join("data/truespec"),
        }
    }

    fn project(parent: &Path, name: &str) -> PathBuf {
        let root = parent.join(name);
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::create_dir_all(root.join("truespec/config")).unwrap();
        fs::write(root.join("docs/guide.md"), "# Guide\n").unwrap();
        fs::write(
            root.join("truespec/config/project.json"),
            "{\n  \"version\": 1,\n  \"source\": { \"adapter\": \"markdown\", \"path\": \"docs\" }\n}\n",
        )
        .unwrap();
        root
    }

    #[test]
    fn registers_multiple_projects_without_review_state_in_global_registry() {
        let fixture = tempfile::tempdir().unwrap();
        let paths = service_paths(fixture.path());
        let registry = ProjectRegistry::open(&paths).unwrap();
        let first_root = project(fixture.path(), "one");
        let first = registry.register(&first_root).unwrap();
        let second = registry.register(&project(fixture.path(), "two")).unwrap();
        assert_ne!(first.id, second.id);
        assert_eq!(registry.counts().unwrap().0, 2);
        assert!(!registry.contains_review_state().unwrap());
        let connection = registry.connection().unwrap();
        let columns = connection
            .prepare("PRAGMA table_info(workspaces)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(
            columns,
            [
                "id",
                "canonical_root",
                "config_fingerprint",
                "available",
                "registered_at",
                "checked_at"
            ]
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(paths.registry()).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        drop(connection);

        let review = first.store.current_session().unwrap().id;
        let workspace = first.id.clone();
        drop(first);
        drop(second);
        drop(registry);
        fs::remove_file(paths.registry()).unwrap();
        let rebuilt = ProjectRegistry::open(&paths).unwrap();
        let restored = rebuilt.register(&first_root).unwrap();
        assert_eq!(restored.id, workspace);
        assert_eq!(restored.store.current_session().unwrap().id, review);
        assert!(!rebuilt.contains_review_state().unwrap());
    }

    #[test]
    fn refuses_duplicate_live_state_and_rebinds_a_verified_move() {
        let fixture = tempfile::tempdir().unwrap();
        let paths = service_paths(fixture.path());
        let registry = ProjectRegistry::open(&paths).unwrap();
        let first_root = project(fixture.path(), "first");
        let first = registry.register(&first_root).unwrap();
        let copied_root = project(fixture.path(), "copy");
        let copied = Store::open(&copied_root.canonicalize().unwrap(), true).unwrap();
        copied
            .write(|tx| {
                tx.execute(
                    "UPDATE workspace_identity SET id = ?1 WHERE singleton = 1",
                    [&first.id],
                )?;
                Ok(())
            })
            .unwrap();
        let duplicate = registry.register(&copied_root).err().unwrap();
        assert!(
            duplicate
                .downcast_ref::<DuplicateWorkspaceIdentity>()
                .is_some()
        );
        assert!(!duplicate.to_string().contains(first_root.to_str().unwrap()));
        fs::rename(first_root.join("docs"), first_root.join("docs-offline")).unwrap();
        assert!(registry.register(&copied_root).is_err());
        fs::rename(first_root.join("docs-offline"), first_root.join("docs")).unwrap();

        fs::remove_dir_all(&copied_root).unwrap();
        let moved = fixture.path().join("moved");
        fs::rename(&first_root, &moved).unwrap();
        let rebound = registry.register(&moved).unwrap();
        assert_eq!(rebound.id, first.id);
        assert_eq!(rebound.root, moved.canonicalize().unwrap());
    }

    #[test]
    fn forgetting_a_workspace_allows_an_explicit_fresh_identity() {
        let fixture = tempfile::tempdir().unwrap();
        let paths = service_paths(fixture.path());
        let owner = OwnerLock::acquire(&paths).unwrap().unwrap();
        let registry = ProjectRegistry::open(&paths).unwrap();
        let root = project(fixture.path(), "project");
        let first = registry.register(&root).unwrap();
        assert_eq!(
            registry.forget(&root, &owner).unwrap(),
            Some(first.id.clone())
        );
        assert!(registry.project(&first.id).is_err());
        assert!(crate::review::reset_state(&root).unwrap());
        let fresh = registry.register(&root).unwrap();
        assert_ne!(fresh.id, first.id);
    }

    #[test]
    fn one_unavailable_project_does_not_break_another() {
        let fixture = tempfile::tempdir().unwrap();
        let registry = ProjectRegistry::open(&service_paths(fixture.path())).unwrap();
        let missing_root = project(fixture.path(), "missing");
        let missing = registry.register(&missing_root).unwrap();
        let present = registry
            .register(&project(fixture.path(), "present"))
            .unwrap();
        registry.evict(&missing.id).unwrap();
        fs::remove_dir_all(missing_root).unwrap();
        assert!(registry.project(&missing.id).is_err());
        assert_eq!(registry.project(&present.id).unwrap().id, present.id);
    }

    #[test]
    fn cached_source_disappearance_marks_only_that_workspace_unavailable() {
        let fixture = tempfile::tempdir().unwrap();
        let registry = ProjectRegistry::open(&service_paths(fixture.path())).unwrap();
        let missing_root = project(fixture.path(), "missing-source");
        let missing = registry.register(&missing_root).unwrap();
        let present = registry
            .register(&project(fixture.path(), "present-source"))
            .unwrap();
        assert_eq!(registry.project(&missing.id).unwrap().id, missing.id);
        fs::remove_dir_all(missing_root.join("docs")).unwrap();
        let error = registry.project(&missing.id).err().unwrap();
        assert!(error.downcast_ref::<ProjectProblem>().is_some());
        assert!(!registry.contexts.read().unwrap().contains_key(&missing.id));
        assert_eq!(registry.project(&present.id).unwrap().id, present.id);
    }

    #[test]
    fn registered_identity_resolves_without_loading_the_source_tree() {
        let fixture = tempfile::tempdir().unwrap();
        let registry = ProjectRegistry::open(&service_paths(fixture.path())).unwrap();
        let root = project(fixture.path(), "project");
        let registered = registry.register(&root).unwrap();
        fs::remove_dir_all(root.join("docs")).unwrap();
        assert_eq!(registry.resolve_registered(&root).unwrap(), registered.id);
    }

    #[test]
    fn registration_requires_a_canonical_git_root_and_unique_root_identity() {
        let fixture = tempfile::tempdir().unwrap();
        let registry = ProjectRegistry::open(&service_paths(fixture.path())).unwrap();
        let not_git = fixture.path().join("not-git");
        fs::create_dir_all(not_git.join("docs")).unwrap();
        fs::create_dir_all(not_git.join("truespec/config")).unwrap();
        fs::write(not_git.join("docs/guide.md"), "# Guide\n").unwrap();
        fs::write(
            not_git.join("truespec/config/project.json"),
            "{\n  \"version\": 1,\n  \"source\": { \"adapter\": \"markdown\", \"path\": \"docs\" }\n}\n",
        )
        .unwrap();
        assert!(registry.register(&not_git).is_err());
        assert!(!not_git.join("truespec/.state/review.sqlite3").exists());

        let root = project(fixture.path(), "root");
        let registered = registry.register(&root).unwrap();
        registered
            .store
            .write(|tx| {
                tx.execute(
                    "UPDATE workspace_identity SET id = ?1 WHERE singleton = 1",
                    [crate::review::new_id()],
                )?;
                Ok(())
            })
            .unwrap();
        assert!(registry.register(&root).is_err());

        fs::write(
            root.join("truespec/config/project.json"),
            "{\n  \"version\": 1,\n  \"source\": { \"adapter\": \"markdown\", \"path\": \"docs/..\" }\n}\n",
        )
        .unwrap();
        assert!(registry.register(&root).is_err());
    }

    #[test]
    fn registration_failure_does_not_recreate_state_for_an_owned_root() {
        let fixture = tempfile::tempdir().unwrap();
        let paths = service_paths(fixture.path());
        let registry = ProjectRegistry::open(&paths).unwrap();
        let root = project(fixture.path(), "project");
        let registered = registry.register(&root).unwrap();
        drop(registered);
        drop(registry);
        let database = root.join("truespec/.state/review.sqlite3");
        fs::remove_file(&database).unwrap();
        let _ = fs::remove_file(root.join("truespec/.state/review.sqlite3-wal"));
        let _ = fs::remove_file(root.join("truespec/.state/review.sqlite3-shm"));

        let registry = ProjectRegistry::open(&paths).unwrap();
        assert!(registry.register(&root).is_err());
        assert!(!database.exists());
    }

    #[test]
    fn cached_contexts_invalidate_on_configuration_change_and_evict_without_state_loss() {
        let fixture = tempfile::tempdir().unwrap();
        let registry = ProjectRegistry::open(&service_paths(fixture.path())).unwrap();
        let root = project(fixture.path(), "project");
        let first = registry.register(&root).unwrap();
        let review = first.store.current_session().unwrap().id;
        let original_fingerprint = first.config_fingerprint.clone();
        registry.evict(&first.id).unwrap();
        let reloaded = registry.project(&first.id).unwrap();
        assert_eq!(reloaded.store.current_session().unwrap().id, review);

        fs::create_dir(root.join("other")).unwrap();
        fs::write(root.join("other/second.md"), "# Second\n").unwrap();
        fs::write(
            root.join("truespec/config/project.json"),
            "{\n  \"version\": 1,\n  \"source\": { \"adapter\": \"markdown\", \"path\": \"other\" }\n}\n",
        )
        .unwrap();
        let changed = registry.project(&first.id).unwrap();
        assert_ne!(changed.config_fingerprint, original_fingerprint);
        assert_eq!(changed.source.list().unwrap()[0].path, "other/second.md");
        assert_eq!(changed.store.current_session().unwrap().id, review);
        first.store.end_session(&review, true).unwrap();
        assert!(!changed.store.has_open_review().unwrap());
        assert_eq!(registry.counts().unwrap(), (1, Some(0)));
    }

    #[test]
    fn cached_contexts_reject_replaced_workspace_identity() {
        let fixture = tempfile::tempdir().unwrap();
        let registry = ProjectRegistry::open(&service_paths(fixture.path())).unwrap();
        let root = project(fixture.path(), "project");
        let cached = registry.register(&root).unwrap();
        cached
            .store
            .write(|tx| {
                tx.execute(
                    "UPDATE workspace_identity SET id = ?1 WHERE singleton = 1",
                    [crate::review::new_id()],
                )?;
                Ok(())
            })
            .unwrap();
        assert!(registry.project(&cached.id).is_err());
        assert!(!registry.contexts.read().unwrap().contains_key(&cached.id));
    }

    #[test]
    fn concurrent_cached_lookups_and_health_counts_do_not_contend_on_registry_connections() {
        let fixture = tempfile::tempdir().unwrap();
        let registry = ProjectRegistry::open(&service_paths(fixture.path())).unwrap();
        let project = registry
            .register(&project(fixture.path(), "project"))
            .unwrap();
        project.store.current_session().unwrap();
        let id = project.id.clone();
        let threads = (0..16)
            .map(|_| {
                let registry = registry.clone();
                let id = id.clone();
                std::thread::spawn(move || {
                    for _ in 0..100 {
                        assert_eq!(registry.project(&id).unwrap().id, id);
                        assert_eq!(registry.counts().unwrap(), (1, Some(1)));
                    }
                })
            })
            .collect::<Vec<_>>();
        for thread in threads {
            thread.join().unwrap();
        }
    }

    #[test]
    fn health_counts_do_not_open_uncached_project_databases() {
        let fixture = tempfile::tempdir().unwrap();
        let registry = ProjectRegistry::open(&service_paths(fixture.path())).unwrap();
        let project = registry
            .register(&project(fixture.path(), "project"))
            .unwrap();
        project.store.current_session().unwrap();
        registry.stores.lock().unwrap().clear();
        assert_eq!(registry.counts().unwrap(), (1, None));
        assert!(registry.stores.lock().unwrap().is_empty());
    }

    #[test]
    fn repeated_registration_shares_open_review_health_state() {
        let fixture = tempfile::tempdir().unwrap();
        let registry = ProjectRegistry::open(&service_paths(fixture.path())).unwrap();
        let root = project(fixture.path(), "project");
        let first = registry.register(&root).unwrap();
        let review = first.store.current_session().unwrap();
        let second = registry.register(&root).unwrap();
        assert!(second.store.has_open_review().unwrap());
        first.store.end_session(&review.id, true).unwrap();
        assert!(!second.store.has_open_review().unwrap());
        assert_eq!(registry.counts().unwrap(), (1, Some(0)));
    }

    #[test]
    fn stale_routing_generation_is_rejected_before_cache_publication() {
        let fixture = tempfile::tempdir().unwrap();
        let registry = ProjectRegistry::open(&service_paths(fixture.path())).unwrap();
        let generation = registry.routing_generation.load(Ordering::Acquire);
        registry.routing_generation.fetch_add(1, Ordering::AcqRel);
        let _routing = registry.routing().unwrap();
        assert!(registry.ensure_routing_generation(generation).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn registration_rejects_symlinked_git_markers_and_escaped_sources() {
        use std::os::unix::fs::symlink;

        let fixture = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let registry = ProjectRegistry::open(&service_paths(fixture.path())).unwrap();
        let root = fixture.path().join("linked");
        fs::create_dir_all(root.join("truespec/config")).unwrap();
        symlink(outside.path(), root.join(".git")).unwrap();
        symlink(outside.path(), root.join("docs")).unwrap();
        fs::write(
            root.join("truespec/config/project.json"),
            "{\n  \"version\": 1,\n  \"source\": { \"adapter\": \"markdown\", \"path\": \"docs\" }\n}\n",
        )
        .unwrap();
        assert!(registry.register(&root).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn refused_reset_validation_preserves_the_registered_route() {
        use std::os::unix::fs::symlink;

        let fixture = tempfile::tempdir().unwrap();
        let registry = ProjectRegistry::open(&service_paths(fixture.path())).unwrap();
        let root = project(fixture.path(), "project");
        let registered = registry.register(&root).unwrap();
        fs::rename(
            root.join("truespec/.state"),
            root.join("truespec/.state-real"),
        )
        .unwrap();
        symlink(
            root.join("truespec/.state-real"),
            root.join("truespec/.state"),
        )
        .unwrap();
        assert!(crate::review::validate_reset_state(&root).is_err());
        assert!(
            registry
                .mapping_matches(&registered.id, &registered.root)
                .unwrap()
        );
        fs::remove_file(root.join("truespec/.state")).unwrap();
        fs::rename(
            root.join("truespec/.state-real"),
            root.join("truespec/.state"),
        )
        .unwrap();
        assert_eq!(registry.project(&registered.id).unwrap().id, registered.id);
    }
}
