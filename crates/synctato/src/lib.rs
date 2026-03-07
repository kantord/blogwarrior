#[doc(hidden)]
pub mod git;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[doc(hidden)]
pub use paste::paste;

pub trait Schema: Sized {
    type Transaction<'a>
    where
        Self: 'a;

    fn load(path: &Path) -> anyhow::Result<Self>;
    fn save(&self) -> anyhow::Result<()>;
    fn reload(&mut self) -> anyhow::Result<()>;
    fn begin(&mut self) -> Self::Transaction<'_>;
    fn merge_remote_from_repo(
        &mut self,
        repo_path: &Path,
    ) -> anyhow::Result<Vec<(&'static str, usize)>>;
}

pub enum SyncEvent<'a> {
    Fetching,
    FetchDone,
    Pushing { first_push: bool },
    PushDone { first_push: bool },
    MergingRemote,
    MergeDone { counts: &'a [(&'static str, usize)] },
}

pub enum SyncResult {
    NoGitRepo,
    NoRemote,
    AlreadyUpToDate,
    Synced,
}

/// Clone a git remote into a new store directory.
pub fn clone_store(dir: &Path, url: &str) -> anyhow::Result<()> {
    let output = git::git_output(&["clone", "--depth", "1", url, &dir.to_string_lossy()])?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git clone failed: {}", stderr.trim());
    }
    Ok(())
}

pub struct Store<S: Schema> {
    schema: S,
    path: PathBuf,
}

impl<S: Schema> Store<S> {
    pub fn new(schema: S, path: PathBuf) -> Self {
        Self { schema, path }
    }

    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let schema = S::load(path)?;
        Ok(Self::new(schema, path.to_path_buf()))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Acquire an exclusive advisory lock on the store directory.
    pub fn lock(&self) -> anyhow::Result<fs::File> {
        fs::create_dir_all(&self.path).context("failed to create store directory")?;
        let lock_file = fs::File::create(self.path.join(".lock"))
            .context("failed to create store lock file")?;
        lock_file.lock().context("failed to acquire store lock")?;
        Ok(lock_file)
    }

    /// Lock the store, reload from disk, run the closure, save, then unlock.
    pub fn locked_transaction<F, T>(&mut self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&mut S::Transaction<'_>) -> anyhow::Result<T>,
    {
        let _lock = self.lock()?;
        self.schema.reload()?;
        let result = {
            let mut tx = self.schema.begin();
            f(&mut tx)?
        };
        self.schema.save()?;
        Ok(result)
    }

    /// Git-aware transaction: ensure_clean → lock → reload → run closure → save → auto_commit → unlock.
    pub fn transact<T>(
        &mut self,
        msg: &str,
        f: impl FnOnce(&mut S::Transaction<'_>) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let repo = git::try_open_repo(self.path());
        if let Some(ref repo) = repo {
            git::ensure_clean(repo)?;
        }
        let result = self.locked_transaction(f)?;
        if let Some(ref repo) = repo {
            git::auto_commit(repo, msg)?;
        }
        Ok(result)
    }

    /// Run a raw git command in the store directory.
    pub fn git_passthrough(&self, args: &[String]) -> anyhow::Result<()> {
        git::git_passthrough(self.path(), args)
    }

    /// Merge remote data under lock: lock → reload → merge → save → unlock.
    fn locked_merge_remote(&mut self) -> anyhow::Result<Vec<(&'static str, usize)>> {
        let _lock = self.lock()?;
        let path = self.path.clone();
        self.schema.reload()?;
        let counts = self.schema.merge_remote_from_repo(&path)?;
        self.schema.save()?;
        Ok(counts)
    }

    /// Sync with git remote (fetch, merge, push).
    pub fn sync_remote(
        &mut self,
        mut on_progress: impl FnMut(SyncEvent),
    ) -> anyhow::Result<SyncResult> {
        let path = self.path().to_path_buf();
        let repo = match git::try_open_repo(&path) {
            Some(r) => r,
            None => return Ok(SyncResult::NoGitRepo),
        };

        git::ensure_clean(&repo)?;

        if !git::has_remote(&path) {
            return Ok(SyncResult::NoRemote);
        }

        if !git::has_remote_branch(&repo) {
            on_progress(SyncEvent::Pushing { first_push: true });
            git::push(&path)?;
            on_progress(SyncEvent::PushDone { first_push: true });
            return Ok(SyncResult::Synced);
        }

        on_progress(SyncEvent::Fetching);
        git::fetch(&path)?;
        on_progress(SyncEvent::FetchDone);

        if git::is_up_to_date(&repo)? {
            return Ok(SyncResult::AlreadyUpToDate);
        }

        // Local is strictly ahead → just push
        if git::is_remote_ancestor(&repo)? {
            on_progress(SyncEvent::Pushing { first_push: false });
            git::push(&path)?;
            on_progress(SyncEvent::PushDone { first_push: false });
            return Ok(SyncResult::Synced);
        }

        // Diverged → merge remote data
        on_progress(SyncEvent::MergingRemote);
        let counts = self.locked_merge_remote()?;
        on_progress(SyncEvent::MergeDone { counts: &counts });

        git::auto_commit(&repo, "sync")?;
        git::merge_ours(&repo)?;

        on_progress(SyncEvent::Pushing { first_push: false });
        git::push(&path)?;
        on_progress(SyncEvent::PushDone { first_push: false });

        Ok(SyncResult::Synced)
    }
}

impl<S: Schema> std::ops::Deref for Store<S> {
    type Target = S;
    fn deref(&self) -> &S {
        &self.schema
    }
}

impl<S: Schema> std::ops::DerefMut for Store<S> {
    fn deref_mut(&mut self) -> &mut S {
        &mut self.schema
    }
}

pub trait TableRow: Clone + PartialEq + Serialize + DeserializeOwned {
    fn key(&self) -> String;

    const TABLE_NAME: &'static str;
    const SHARD_CHARACTERS: usize;
    const EXPECTED_CAPACITY: usize;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Row<T> {
    // Tombstone must come first: with untagged deserialization serde tries
    // variants in declaration order, and Tombstone's required `deleted_at`
    // field distinguishes it from Live.
    Tombstone {
        id: String,
        deleted_at: DateTime<Utc>,
    },
    Live {
        id: String,
        #[serde(flatten)]
        inner: T,
        #[serde(default)]
        updated_at: Option<DateTime<Utc>>,
    },
}

impl<T> Row<T> {
    pub fn id(&self) -> &str {
        match self {
            Row::Live { id, .. } | Row::Tombstone { id, .. } => id,
        }
    }

    pub fn last_modified(&self) -> Option<DateTime<Utc>> {
        match self {
            Row::Live { updated_at, .. } => *updated_at,
            Row::Tombstone { deleted_at, .. } => Some(*deleted_at),
        }
    }
}

pub(crate) fn parse_rows<T: TableRow>(content: &str) -> anyhow::Result<HashMap<String, Row<T>>> {
    let mut items = HashMap::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let row: Row<T> = serde_json::from_str(line).context("failed to parse JSONL line")?;
        items.insert(row.id().to_string(), row);
    }
    Ok(items)
}

/// Truncated SHA-256 used as a content-addressed ID. Two different keys that
/// collide on the truncated hash will silently overwrite each other; the
/// birthday-problem sizing in [`id_length_for_capacity`] keeps collision
/// probability well below 0.1% for the declared capacity.
fn hash_id(raw: &str, id_length: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    format!("{:x}", hasher.finalize())[..id_length].to_string()
}

/// Choose a hex ID length that keeps collision probability < 0.1% for up to
/// `expected_items` entries (birthday-problem formula with a 500x safety factor).
fn id_length_for_capacity(expected_items: usize) -> usize {
    if expected_items <= 1 {
        return 4;
    }
    let k = expected_items as f64;
    let n = (500.0 * k * k).ln() / 16_f64.ln();
    (n.ceil() as usize).max(4)
}

pub struct Table<T: TableRow> {
    items: HashMap<String, Row<T>>,
    dir: PathBuf,
    shard_characters: usize,
    id_length: usize,
}

impl<T: TableRow> Table<T> {
    fn read_items(dir: &Path) -> anyhow::Result<HashMap<String, Row<T>>> {
        let mut items = HashMap::new();
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(fname) = path.file_name().and_then(|f| f.to_str())
                    && fname.starts_with("items_")
                    && fname.ends_with(".jsonl")
                {
                    let content = fs::read_to_string(&path)
                        .with_context(|| format!("failed to read {}", path.display()))?;
                    let parsed: HashMap<String, Row<T>> = parse_rows(&content)
                        .with_context(|| format!("failed to parse entry in {}", path.display()))?;
                    items.extend(parsed);
                }
            }
        }
        Ok(items)
    }

    pub fn load(store: &Path) -> anyhow::Result<Self> {
        let dir = store.join(T::TABLE_NAME);
        let id_length = id_length_for_capacity(T::EXPECTED_CAPACITY);
        let items = Self::read_items(&dir)?;
        Ok(Self {
            items,
            dir,
            shard_characters: T::SHARD_CHARACTERS,
            id_length,
        })
    }

    /// Re-read all items from disk, replacing the in-memory state.
    pub fn reload(&mut self) -> anyhow::Result<()> {
        self.items = Self::read_items(&self.dir)?;
        Ok(())
    }

    pub fn upsert(&mut self, item: T) {
        let id = hash_id(&item.key(), self.id_length);

        if let Some(Row::Live {
            inner: existing, ..
        }) = self.items.get(&id)
            && item == *existing
        {
            return;
        }

        self.items.insert(
            id.clone(),
            Row::Live {
                id,
                inner: item,
                updated_at: Some(Utc::now()),
            },
        );
    }

    pub fn delete(&mut self, key: &str) -> Option<String> {
        let id = hash_id(key, self.id_length);
        if !matches!(self.items.get(&id), Some(Row::Live { .. })) {
            return None;
        }
        self.items.insert(
            id.clone(),
            Row::Tombstone {
                id: id.clone(),
                deleted_at: Utc::now(),
            },
        );
        Some(id)
    }

    pub fn delete_where(&mut self, pred: impl Fn(&T) -> bool) {
        let now = Utc::now();
        let ids: Vec<String> = self
            .items
            .iter()
            .filter_map(|(id, row)| match row {
                Row::Live { inner, .. } if pred(inner) => Some(id.clone()),
                _ => None,
            })
            .collect();
        for id in ids {
            self.items.insert(
                id.clone(),
                Row::Tombstone {
                    id,
                    deleted_at: now,
                },
            );
        }
    }

    pub fn id_of(&self, item: &T) -> String {
        hash_id(&item.key(), self.id_length)
    }

    pub fn get(&self, key: &str) -> Option<&T> {
        let id = hash_id(key, self.id_length);
        match self.items.get(&id) {
            Some(Row::Live { inner, .. }) => Some(inner),
            _ => None,
        }
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    fn shard_key(&self, id: &str) -> String {
        let end = self.shard_characters.min(id.len());
        id[..end].to_string()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.dir).context("failed to create table directory")?;

        let lock_file =
            fs::File::create(self.dir.join(".lock")).context("failed to create lock file")?;
        // std::fs::File::lock() — exclusive advisory lock (stable since Rust 1.89)
        lock_file.lock().context("failed to acquire lock")?;

        // Group items by shard key
        let mut shards: HashMap<String, Vec<&Row<T>>> = HashMap::new();
        for row in self.items.values() {
            let key = self.shard_key(row.id());
            shards.entry(key).or_default().push(row);
        }

        // Phase 1: Write new shards to temporary files.
        // If this fails, old shard files remain untouched.
        let mut tmp_paths = Vec::new();
        for (prefix, rows) in &mut shards {
            rows.sort_by(|a, b| a.id().cmp(b.id()));
            let mut out = String::new();
            for row in rows.iter() {
                out.push_str(&serde_json::to_string(row).context("failed to serialize item")?);
                out.push('\n');
            }
            let tmp_path = self.dir.join(format!("items_{}.jsonl.tmp", prefix));
            if let Err(e) = fs::write(&tmp_path, out) {
                // Clean up the failed temp file and any previously written ones
                let _ = fs::remove_file(&tmp_path);
                for (p, _) in &tmp_paths {
                    let _ = fs::remove_file(p);
                }
                return Err(e).context("failed to write shard file");
            }
            tmp_paths.push((tmp_path, format!("items_{}.jsonl", prefix)));
        }

        // Phase 2: Atomically rename temp files over old shard files.
        // On POSIX, rename() replaces the destination if it exists.
        let new_shard_names: std::collections::HashSet<String> =
            tmp_paths.iter().map(|(_, name)| name.clone()).collect();
        for (tmp_path, final_name) in tmp_paths {
            let final_path = self.dir.join(final_name);
            fs::rename(&tmp_path, &final_path).context("failed to rename shard file")?;
        }

        // Phase 3: Remove stale shard files that no longer have data
        if let Ok(entries) = fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(fname) = path.file_name().and_then(|f| f.to_str())
                    && fname.starts_with("items_")
                    && fname.ends_with(".jsonl")
                    && !fname.ends_with(".tmp")
                    && !new_shard_names.contains(fname)
                {
                    fs::remove_file(&path).context("failed to remove stale shard file")?;
                }
            }
        }

        Ok(())
    }

    pub fn merge_remote(&mut self, remote: HashMap<String, Row<T>>) {
        for (id, remote_row) in remote {
            let dominated = match self.items.get(&id) {
                None => true,
                Some(local_row) => match (local_row.last_modified(), remote_row.last_modified()) {
                    (_, None) => false,
                    (None, Some(_)) => true,
                    (Some(local_ts), Some(remote_ts)) => remote_ts > local_ts,
                },
            };
            if dominated {
                self.items.insert(id, remote_row);
            }
        }
    }

    pub fn items(&self) -> Vec<T> {
        self.items
            .values()
            .filter_map(|r| match r {
                Row::Live { inner, .. } => Some(inner.clone()),
                Row::Tombstone { .. } => None,
            })
            .collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &T)> {
        self.items.values().filter_map(|r| match r {
            Row::Live { id, inner, .. } => Some((id.as_str(), inner)),
            Row::Tombstone { .. } => None,
        })
    }
}

#[macro_export]
macro_rules! schema {
    ($vis:vis $name:ident { $($field:ident : $row:ty),* $(,)? }) => {
        $crate::paste! {
            $vis struct $name {
                $($field: $crate::Table<$row>,)*
            }

            $vis struct [<$name Transaction>]<'a> {
                $(pub $field: &'a mut $crate::Table<$row>,)*
            }

            impl $name {
                $(
                    pub fn $field(&self) -> &$crate::Table<$row> {
                        &self.$field
                    }
                )*
            }
        }
    };
}

#[macro_export]
macro_rules! store {
    ($name:ident { $($field:ident : $row:ty),* $(,)? }) => {
        $crate::paste! {
            impl $crate::Schema for $name {
                type Transaction<'a> = [<$name Transaction>]<'a>;

                fn load(path: &::std::path::Path) -> ::anyhow::Result<Self> {
                    Ok($name {
                        $($field: $crate::Table::<$row>::load(path)?,)*
                    })
                }

                fn save(&self) -> ::anyhow::Result<()> {
                    $(self.$field.save()?;)*
                    Ok(())
                }

                fn reload(&mut self) -> ::anyhow::Result<()> {
                    $(self.$field.reload()?;)*
                    Ok(())
                }

                fn begin(&mut self) -> [<$name Transaction>]<'_> {
                    [<$name Transaction>] {
                        $($field: &mut self.$field,)*
                    }
                }

                fn merge_remote_from_repo(
                    &mut self,
                    repo_path: &::std::path::Path,
                ) -> ::anyhow::Result<Vec<(&'static str, usize)>> {
                    let repo = $crate::git::open_repo(repo_path)?;
                    let mut counts = Vec::new();
                    $(
                        let remote = $crate::git::read_remote_table::<$row>(
                            &repo,
                            <$row as $crate::TableRow>::TABLE_NAME,
                        )?;
                        let c = remote.len();
                        self.$field.merge_remote(remote);
                        counts.push((<$row as $crate::TableRow>::TABLE_NAME, c));
                    )*
                    Ok(counts)
                }
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;
    use rstest::rstest;
    use rusty_fork::rusty_fork_test;
    use serde::Deserialize;
    use tempfile::TempDir;

    fn utc_rfc3339(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().to_utc()
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestItem {
        #[serde(default)]
        raw_id: String,
        title: String,
    }

    impl TableRow for TestItem {
        fn key(&self) -> String {
            self.raw_id.clone()
        }

        const TABLE_NAME: &'static str = "t";
        const SHARD_CHARACTERS: usize = 2;
        const EXPECTED_CAPACITY: usize = 1000;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct UnshardedItem {
        #[serde(default)]
        raw_id: String,
        title: String,
    }

    impl TableRow for UnshardedItem {
        fn key(&self) -> String {
            self.raw_id.clone()
        }

        const TABLE_NAME: &'static str = "t";
        const SHARD_CHARACTERS: usize = 0;
        const EXPECTED_CAPACITY: usize = 1000;
    }

    fn new_test_table() -> (TempDir, Table<TestItem>) {
        let dir = TempDir::new().unwrap();
        let table = Table::<TestItem>::load(dir.path()).unwrap();
        (dir, table)
    }

    fn make_item(raw_id: &str, title: &str) -> TestItem {
        TestItem {
            raw_id: raw_id.to_string(),
            title: title.to_string(),
        }
    }

    #[test]
    fn test_upsert_hashes_id() {
        let (_dir, mut table) = new_test_table();
        let item = make_item("raw-id", "Post");
        table.upsert(item.clone());
        assert_eq!(
            table.id_of(&item),
            hash_id(
                "raw-id",
                id_length_for_capacity(TestItem::EXPECTED_CAPACITY)
            )
        );
        assert_eq!(table.items().len(), 1);
    }

    #[test]
    fn test_upsert_overwrites_existing() {
        let (_dir, mut table) = new_test_table();
        table.upsert(make_item("same-id", "Original"));
        table.upsert(make_item("same-id", "Updated"));
        let items = table.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Updated");
    }

    #[test]
    fn test_load_save_roundtrip() {
        let (dir, mut table) = new_test_table();
        table.upsert(make_item("id-1", "First"));
        table.upsert(make_item("id-2", "Second"));
        table.save().unwrap();

        let loaded = Table::<TestItem>::load(dir.path()).unwrap();
        assert_eq!(loaded.items().len(), 2);

        let titles: Vec<String> = loaded.items().iter().map(|i| i.title.clone()).collect();
        assert!(titles.contains(&"First".to_string()));
        assert!(titles.contains(&"Second".to_string()));
    }

    #[test]
    fn test_load_nonexistent_file() {
        let (_dir, table) = new_test_table();
        assert_eq!(table.items().len(), 0);
    }

    /// Read all lines from all shard files in the table directory.
    fn read_lines(dir: &TempDir, name: &str) -> Vec<String> {
        let table_dir = dir.path().join(name);
        let mut lines = Vec::new();
        if let Ok(entries) = fs::read_dir(&table_dir) {
            let mut paths: Vec<_> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|f| f.to_str())
                        .is_some_and(|f| f.starts_with("items_") && f.ends_with(".jsonl"))
                })
                .collect();
            paths.sort();
            for path in paths {
                for line in fs::read_to_string(&path)
                    .unwrap()
                    .lines()
                    .filter(|l| !l.is_empty())
                {
                    lines.push(line.to_string());
                }
            }
        }
        lines
    }

    fn ids_from_lines(lines: &[String]) -> Vec<String> {
        lines
            .iter()
            .map(|l| {
                let v: serde_json::Value = serde_json::from_str(l).unwrap();
                v["id"].as_str().unwrap().to_string()
            })
            .collect()
    }

    /// List shard file names in a table directory.
    fn shard_files(dir: &TempDir, name: &str) -> Vec<String> {
        let table_dir = dir.path().join(name);
        let mut names: Vec<String> = fs::read_dir(&table_dir)
            .unwrap()
            .flatten()
            .filter_map(|e| {
                let fname = e.file_name().to_str()?.to_string();
                if fname.starts_with("items_") && fname.ends_with(".jsonl") {
                    Some(fname)
                } else {
                    None
                }
            })
            .collect();
        names.sort();
        names
    }

    #[test]
    fn test_save_sorts_items_by_id() {
        let (dir, mut table) = new_test_table();
        table.upsert(make_item("zzz", "Last"));
        table.upsert(make_item("aaa", "First"));
        table.upsert(make_item("mmm", "Middle"));
        table.save().unwrap();

        let ids = ids_from_lines(&read_lines(&dir, "t"));
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn test_save_sort_order_is_stable_across_roundtrips() {
        let (dir, mut table) = new_test_table();
        table.upsert(make_item("c", "C"));
        table.upsert(make_item("a", "A"));
        table.upsert(make_item("b", "B"));
        table.save().unwrap();

        let ids1 = ids_from_lines(&read_lines(&dir, "t"));

        let loaded = Table::<TestItem>::load(dir.path()).unwrap();
        loaded.save().unwrap();

        let ids2 = ids_from_lines(&read_lines(&dir, "t"));
        assert_eq!(ids1, ids2);
    }

    #[test]
    fn test_save_sort_order_preserved_after_upsert() {
        let (dir, mut table) = new_test_table();
        table.upsert(make_item("b", "B"));
        table.upsert(make_item("a", "A"));
        table.save().unwrap();

        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        table.upsert(make_item("c", "C"));
        table.save().unwrap();

        let ids = ids_from_lines(&read_lines(&dir, "t"));
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn test_save_single_item_sorted() {
        let (dir, mut table) = new_test_table();
        table.upsert(make_item("only", "Only"));
        table.save().unwrap();

        let ids = ids_from_lines(&read_lines(&dir, "t"));
        assert_eq!(ids.len(), 1);
    }

    #[test]
    fn test_save_empty_table() {
        let (dir, table) = new_test_table();
        table.save().unwrap();

        let lines = read_lines(&dir, "t");
        assert!(lines.is_empty());
    }

    #[test]
    fn test_items_land_in_correct_shard_files() {
        let (dir, mut table) = new_test_table();
        table
            .items
            .insert("aabb11".to_string(), make_row_with_id("aabb11", "Item AA"));
        table
            .items
            .insert("aabb22".to_string(), make_row_with_id("aabb22", "Item AA2"));
        table
            .items
            .insert("ccdd33".to_string(), make_row_with_id("ccdd33", "Item CC"));
        table.save().unwrap();

        let files = shard_files(&dir, "t");
        assert_eq!(files, vec!["items_aa.jsonl", "items_cc.jsonl"]);

        let aa_content = fs::read_to_string(dir.path().join("t").join("items_aa.jsonl")).unwrap();
        let aa_lines: Vec<&str> = aa_content.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(aa_lines.len(), 2);

        let cc_content = fs::read_to_string(dir.path().join("t").join("items_cc.jsonl")).unwrap();
        let cc_lines: Vec<&str> = cc_content.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(cc_lines.len(), 1);
    }

    #[test]
    fn test_load_reads_from_multiple_shard_files() {
        let dir = TempDir::new().unwrap();
        let table_dir = dir.path().join("t");
        fs::create_dir_all(&table_dir).unwrap();

        let item1 = r#"{"id":"aa1111","title":"From AA"}"#;
        let item2 = r#"{"id":"bb2222","title":"From BB"}"#;
        fs::write(table_dir.join("items_aa.jsonl"), format!("{}\n", item1)).unwrap();
        fs::write(table_dir.join("items_bb.jsonl"), format!("{}\n", item2)).unwrap();

        let table = Table::<TestItem>::load(dir.path()).unwrap();
        assert_eq!(table.items().len(), 2);
        let titles: Vec<String> = table.items().iter().map(|i| i.title.clone()).collect();
        assert!(titles.contains(&"From AA".to_string()));
        assert!(titles.contains(&"From BB".to_string()));
    }

    #[test]
    fn test_roundtrip_with_sharding_preserves_all_items() {
        let (dir, mut table) = new_test_table();
        table.upsert(make_item("alpha", "Alpha"));
        table.upsert(make_item("beta", "Beta"));
        table.upsert(make_item("gamma", "Gamma"));
        table.save().unwrap();

        let loaded = Table::<TestItem>::load(dir.path()).unwrap();
        assert_eq!(loaded.items().len(), 3);
        let titles: Vec<String> = loaded.items().iter().map(|i| i.title.clone()).collect();
        assert!(titles.contains(&"Alpha".to_string()));
        assert!(titles.contains(&"Beta".to_string()));
        assert!(titles.contains(&"Gamma".to_string()));
    }

    #[test]
    fn test_shard_characters_zero_puts_everything_in_items_empty() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<UnshardedItem>::load(dir.path()).unwrap();
        table.items.insert(
            "aabb11".to_string(),
            Row::Live {
                id: "aabb11".to_string(),
                inner: UnshardedItem {
                    raw_id: String::new(),
                    title: "Item 1".to_string(),
                },
                updated_at: None,
            },
        );
        table.items.insert(
            "ccdd22".to_string(),
            Row::Live {
                id: "ccdd22".to_string(),
                inner: UnshardedItem {
                    raw_id: String::new(),
                    title: "Item 2".to_string(),
                },
                updated_at: None,
            },
        );
        table.save().unwrap();

        let files = shard_files(&dir, "t");
        assert_eq!(files, vec!["items_.jsonl"]);

        let content = fs::read_to_string(dir.path().join("t").join("items_.jsonl")).unwrap();
        let lines: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_save_cleans_up_old_shard_files() {
        let dir = TempDir::new().unwrap();
        let table_dir = dir.path().join("t");
        fs::create_dir_all(&table_dir).unwrap();

        let old_item = r#"{"id":"zz9999","title":"Old"}"#;
        fs::write(table_dir.join("items_zz.jsonl"), format!("{}\n", old_item)).unwrap();

        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        table.items.insert(
            "zz9999".to_string(),
            Row::Tombstone {
                id: "zz9999".to_string(),
                deleted_at: Utc::now(),
            },
        );
        table
            .items
            .insert("aabb11".to_string(), make_row_with_id("aabb11", "Item AA"));
        table.save().unwrap();

        let loaded = Table::<TestItem>::load(dir.path()).unwrap();
        assert_eq!(loaded.items().len(), 1);
        assert_eq!(loaded.items()[0].title, "Item AA");

        fs::write(table_dir.join("items_qq.jsonl"), "").unwrap();
        loaded.save().unwrap();
        assert!(!table_dir.join("items_qq.jsonl").exists());
    }

    #[test]
    fn test_upsert_same_id_overwrites() {
        let (_dir, mut table) = new_test_table();
        table.upsert(make_item("same", "First"));
        table.upsert(make_item("same", "Second"));
        let items = table.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Second");
    }

    fn get_updated_at(table: &Table<TestItem>) -> Option<DateTime<Utc>> {
        match table.items.values().next().unwrap() {
            Row::Live { updated_at, .. } => *updated_at,
            Row::Tombstone { .. } => None,
        }
    }

    #[test]
    fn test_upsert_sets_updated_at_on_new_item() {
        let (_dir, mut table) = new_test_table();
        table.upsert(make_item("new", "New Item"));
        assert!(get_updated_at(&table).is_some());
    }

    #[test]
    fn test_upsert_preserves_updated_at_when_unchanged() {
        let (_dir, mut table) = new_test_table();
        table.upsert(make_item("x", "Same"));
        let ts1 = get_updated_at(&table);

        table.upsert(make_item("x", "Same"));
        let ts2 = get_updated_at(&table);
        assert_eq!(ts1, ts2);
    }

    #[test]
    fn test_upsert_updates_updated_at_when_content_changes() {
        let (_dir, mut table) = new_test_table();
        table.upsert(make_item("x", "Original"));
        let ts1 = get_updated_at(&table);

        table.upsert(make_item("x", "Changed"));
        let ts2 = get_updated_at(&table);
        assert_ne!(ts1, ts2);
        assert!(ts2 > ts1);
    }

    #[test]
    fn test_updated_at_survives_save_load_roundtrip() {
        let (dir, mut table) = new_test_table();
        table.upsert(make_item("x", "Item"));
        let ts = get_updated_at(&table);
        table.save().unwrap();

        let loaded = Table::<TestItem>::load(dir.path()).unwrap();
        assert_eq!(get_updated_at(&loaded), ts);
    }

    #[test]
    fn test_upsert_unchanged_after_roundtrip() {
        let (dir, mut table) = new_test_table();
        table.upsert(make_item("x", "Item"));
        table.save().unwrap();

        let mut loaded = Table::<TestItem>::load(dir.path()).unwrap();
        let ts_before = get_updated_at(&loaded);

        loaded.upsert(make_item("x", "Item"));
        let ts_after = get_updated_at(&loaded);
        assert_eq!(ts_before, ts_after);
    }

    #[test]
    fn test_delete_removes_from_items() {
        let (_dir, mut table) = new_test_table();
        table.upsert(make_item("x", "Item"));
        assert_eq!(table.items().len(), 1);

        table.delete("x");
        assert_eq!(table.items().len(), 0);
    }

    #[test]
    fn test_delete_tombstone_survives_roundtrip() {
        let (dir, mut table) = new_test_table();
        table.upsert(make_item("x", "Item"));
        table.delete("x");
        table.save().unwrap();

        let loaded = Table::<TestItem>::load(dir.path()).unwrap();
        assert_eq!(loaded.items().len(), 0);
    }

    #[test]
    fn test_upsert_resurrects_deleted_item() {
        let (_dir, mut table) = new_test_table();
        table.upsert(make_item("x", "Original"));
        table.delete("x");
        assert_eq!(table.items().len(), 0);

        table.upsert(make_item("x", "Resurrected"));
        let items = table.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Resurrected");
    }

    #[test]
    fn test_upsert_resurrects_after_roundtrip() {
        let (dir, mut table) = new_test_table();
        table.upsert(make_item("x", "Original"));
        table.delete("x");
        table.save().unwrap();

        let mut loaded = Table::<TestItem>::load(dir.path()).unwrap();
        assert_eq!(loaded.items().len(), 0);

        loaded.upsert(make_item("x", "Back"));
        let items = loaded.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Back");
    }

    #[test]
    fn test_contains_key_live() {
        let (_dir, mut table) = new_test_table();
        table.upsert(make_item("x", "Item"));
        assert!(table.contains_key("x"));
    }

    #[test]
    fn test_contains_key_missing() {
        let (_dir, table) = new_test_table();
        assert!(!table.contains_key("x"));
    }

    #[test]
    fn test_contains_key_deleted() {
        let (_dir, mut table) = new_test_table();
        table.upsert(make_item("x", "Item"));
        table.delete("x");
        assert!(!table.contains_key("x"));
    }

    #[test]
    fn test_delete_nonexistent_key_returns_none() {
        let (_dir, mut table) = new_test_table();
        table.upsert(make_item("a", "Keep"));
        assert!(table.delete("never-added").is_none());
        assert_eq!(table.items().len(), 1);
    }

    #[test]
    fn test_delete_mixed_with_live() {
        let (_dir, mut table) = new_test_table();
        table.upsert(make_item("a", "Keep"));
        table.upsert(make_item("b", "Delete"));
        table.upsert(make_item("c", "Also Keep"));
        table.delete("b");

        let items = table.items();
        assert_eq!(items.len(), 2);
        let titles: Vec<&str> = items.iter().map(|i| i.title.as_str()).collect();
        assert!(titles.contains(&"Keep"));
        assert!(titles.contains(&"Also Keep"));
        assert!(!titles.contains(&"Delete"));
    }

    #[rstest]
    #[case::truncated_json("{\"id\":\"abc\",\"title\":\"tr\n", Some("items_aa.jsonl"))]
    #[case::completely_invalid("not json at all\n", None)]
    #[case::mixed_valid_and_invalid(
        "{\"id\":\"aa1111\",\"title\":\"Valid\"}\ncorrupted line\n",
        None
    )]
    fn test_load_invalid_content(#[case] content: &str, #[case] error_contains: Option<&str>) {
        let dir = TempDir::new().unwrap();
        let table_dir = dir.path().join("t");
        fs::create_dir_all(&table_dir).unwrap();
        fs::write(table_dir.join("items_aa.jsonl"), content).unwrap();
        let result = Table::<TestItem>::load(dir.path());
        assert!(result.is_err());
        if let Some(substr) = error_contains {
            let err_msg = format!("{:#}", result.err().unwrap());
            assert!(
                err_msg.contains(substr),
                "error should contain '{substr}', got: {err_msg}"
            );
        }
    }

    #[test]
    fn test_load_empty_lines_between_valid_entries() {
        let dir = TempDir::new().unwrap();
        let table_dir = dir.path().join("t");
        fs::create_dir_all(&table_dir).unwrap();

        let content = format!(
            "{}\n\n{}\n\n",
            r#"{"id":"aa1111","title":"First"}"#, r#"{"id":"bb2222","title":"Second"}"#,
        );
        fs::write(table_dir.join("items_aa.jsonl"), content).unwrap();

        let table = Table::<TestItem>::load(dir.path()).unwrap();
        assert_eq!(table.items().len(), 2);
    }

    #[test]
    fn test_load_extra_unknown_fields_ignored() {
        let dir = TempDir::new().unwrap();
        let table_dir = dir.path().join("t");
        fs::create_dir_all(&table_dir).unwrap();

        let content =
            r#"{"id":"aa1111","title":"Post","extra_field":"should be ignored","another":42}"#;
        fs::write(table_dir.join("items_aa.jsonl"), format!("{}\n", content)).unwrap();

        let table = Table::<TestItem>::load(dir.path()).unwrap();
        assert_eq!(table.items().len(), 1);
        assert_eq!(table.items()[0].title, "Post");
    }

    #[cfg(unix)]
    rusty_fork_test! {
        #[test]
        fn test_failed_save_preserves_previous_data() {
            let dir = TempDir::new().unwrap();

            let mut table = Table::<TestItem>::load(dir.path()).unwrap();
            table
                .items
                .insert("aabb11".to_string(), make_row_with_id("aabb11", "Original"));
            table.save().unwrap();

            let loaded = Table::<TestItem>::load(dir.path()).unwrap();
            assert_eq!(loaded.items().len(), 1);

            let dir_path = dir.path().to_path_buf();
            let child_status = unsafe { libc::fork() };
            match child_status {
                -1 => panic!("fork failed"),
                0 => {
                    unsafe {
                        libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
                        let limit = libc::rlimit {
                            rlim_cur: 8,
                            rlim_max: libc::RLIM_INFINITY,
                        };
                        libc::setrlimit(libc::RLIMIT_FSIZE, &limit);
                    }
                    let table = Table::<TestItem>::load(&dir_path).unwrap();
                    let _ = table.save();
                    std::process::exit(0);
                }
                child_pid => {
                    let mut wstatus: libc::c_int = 0;
                    unsafe {
                        libc::waitpid(child_pid, &mut wstatus, 0);
                    }
                }
            }

            let recovered =
                Table::<TestItem>::load(dir.path()).expect("load should not fail after a failed save");
            assert_eq!(
                recovered.items().len(),
                1,
                "original data should survive a failed save()"
            );
            assert_eq!(recovered.items()[0].title, "Original");
        }
    }

    schema!(TestDb { t: TestItem });
    store!(TestDb { t: TestItem });

    #[test]
    fn test_locked_transaction_reloads_before_mutating() {
        let dir = TempDir::new().unwrap();

        let mut db = Store::<TestDb>::open(dir.path()).unwrap();
        db.locked_transaction(|tx| {
            tx.t.upsert(make_item("x", "Original"));
            Ok(())
        })
        .unwrap();

        let mut other = Store::<TestDb>::open(dir.path()).unwrap();
        other
            .locked_transaction(|tx| {
                tx.t.upsert(make_item("x", "From Other"));
                Ok(())
            })
            .unwrap();

        db.locked_transaction(|tx| {
            tx.t.upsert(make_item("y", "New Item"));
            Ok(())
        })
        .unwrap();

        let final_db = Store::<TestDb>::open(dir.path()).unwrap();
        let items = final_db.t.items();
        assert_eq!(items.len(), 2);
        let titles: Vec<&str> = items.iter().map(|i| i.title.as_str()).collect();
        assert!(
            titles.contains(&"From Other"),
            "reload should preserve other process's write"
        );
        assert!(titles.contains(&"New Item"));
    }

    #[test]
    fn test_locked_transaction_preserves_concurrent_writes() {
        let dir = TempDir::new().unwrap();

        let mut db = Store::<TestDb>::open(dir.path()).unwrap();
        db.locked_transaction(|tx| {
            tx.t.upsert(make_item("existing", "Existing"));
            Ok(())
        })
        .unwrap();

        let mut db_a = Store::<TestDb>::open(dir.path()).unwrap();
        db_a.locked_transaction(|tx| {
            tx.t.upsert(make_item("from-a", "From A"));
            Ok(())
        })
        .unwrap();

        let mut db_b = Store::<TestDb>::open(dir.path()).unwrap();
        db_b.locked_transaction(|tx| {
            tx.t.upsert(make_item("from-b", "From B"));
            Ok(())
        })
        .unwrap();

        let final_db = Store::<TestDb>::open(dir.path()).unwrap();
        assert_eq!(
            final_db.t.items().len(),
            3,
            "locked_transaction should preserve all items"
        );
    }

    /// Helper to create a Row with a pre-set id (no hashing).
    fn make_row_with_id(id: &str, title: &str) -> Row<TestItem> {
        Row::Live {
            id: id.to_string(),
            inner: TestItem {
                raw_id: String::new(),
                title: title.to_string(),
            },
            updated_at: None,
        }
    }

    fn make_live_row(id: &str, title: &str, ts: DateTime<Utc>) -> Row<TestItem> {
        Row::Live {
            id: id.to_string(),
            inner: TestItem {
                raw_id: String::new(),
                title: title.to_string(),
            },
            updated_at: Some(ts),
        }
    }

    fn make_tombstone_row(id: &str, ts: DateTime<Utc>) -> Row<TestItem> {
        Row::Tombstone {
            id: id.to_string(),
            deleted_at: ts,
        }
    }

    #[test]
    fn test_merge_remote_only_on_remote() {
        let (_dir, mut table) = new_test_table();
        let ts = Utc::now();
        let mut remote = HashMap::new();
        remote.insert("aa".to_string(), make_live_row("aa", "Remote", ts));
        table.merge_remote(remote);
        assert_eq!(table.items().len(), 1);
        assert_eq!(table.items()[0].title, "Remote");
    }

    #[test]
    fn test_merge_remote_only_on_local() {
        let (_dir, mut table) = new_test_table();
        table.upsert(make_item("x", "Local"));
        let remote = HashMap::new();
        table.merge_remote(remote);
        assert_eq!(table.items().len(), 1);
        assert_eq!(table.items()[0].title, "Local");
    }

    #[rstest]
    #[case::local_newer(
        Some("Local"),
        "2024-06-01T00:00:00Z",
        Some("Remote"),
        "2024-01-01T00:00:00Z",
        Some("Local")
    )]
    #[case::remote_newer(
        Some("Local"),
        "2024-01-01T00:00:00Z",
        Some("Remote"),
        "2024-06-01T00:00:00Z",
        Some("Remote")
    )]
    #[case::tombstone_wins(
        Some("Local"),
        "2024-01-01T00:00:00Z",
        None,
        "2024-06-01T00:00:00Z",
        None
    )]
    #[case::live_wins(
        None,
        "2024-01-01T00:00:00Z",
        Some("Remote"),
        "2024-06-01T00:00:00Z",
        Some("Remote")
    )]
    #[case::same_ts_local_wins(
        Some("Local"),
        "2024-01-01T00:00:00Z",
        Some("Remote"),
        "2024-01-01T00:00:00Z",
        Some("Local")
    )]
    fn test_merge_remote_lww(
        #[case] local_title: Option<&str>,
        #[case] local_ts: &str,
        #[case] remote_title: Option<&str>,
        #[case] remote_ts: &str,
        #[case] expected_title: Option<&str>,
    ) {
        let (_dir, mut table) = new_test_table();
        let id = hash_id("x", id_length_for_capacity(TestItem::EXPECTED_CAPACITY));
        let local_row = match local_title {
            Some(t) => make_live_row(&id, t, utc_rfc3339(local_ts)),
            None => make_tombstone_row(&id, utc_rfc3339(local_ts)),
        };
        table.items.insert(id.clone(), local_row);
        let remote_row = match remote_title {
            Some(t) => make_live_row(&id, t, utc_rfc3339(remote_ts)),
            None => make_tombstone_row(&id, utc_rfc3339(remote_ts)),
        };
        let mut remote = HashMap::new();
        remote.insert(id.clone(), remote_row);
        table.merge_remote(remote);
        match expected_title {
            Some(title) => {
                assert_eq!(table.items().len(), 1);
                assert_eq!(table.items()[0].title, title);
            }
            None => assert!(table.items().is_empty()),
        }
    }

    #[test]
    fn test_merge_remote_survives_roundtrip() {
        let (dir, mut table) = new_test_table();
        let ts = Utc::now();

        let mut remote = HashMap::new();
        remote.insert("aa11".to_string(), make_live_row("aa11", "Remote", ts));
        table.merge_remote(remote);
        table.save().unwrap();

        let loaded = Table::<TestItem>::load(dir.path()).unwrap();
        assert_eq!(loaded.items().len(), 1);
        assert_eq!(loaded.items()[0].title, "Remote");
    }

    #[rstest]
    #[case::two_valid(
        "{\"id\":\"aa\",\"title\":\"First\"}\n{\"id\":\"bb\",\"title\":\"Second\"}\n",
        2
    )]
    #[case::empty("", 0)]
    #[case::blank_lines_skipped(
        "{\"id\":\"aa\",\"title\":\"First\"}\n\n\n{\"id\":\"bb\",\"title\":\"Second\"}\n\n",
        2
    )]
    fn test_parse_rows_valid(#[case] content: &str, #[case] expected_len: usize) {
        let rows: HashMap<String, Row<TestItem>> = parse_rows(content).unwrap();
        assert_eq!(rows.len(), expected_len);
    }

    #[test]
    fn test_parse_rows_invalid_json() {
        let content = "not valid json\n";
        let result: anyhow::Result<HashMap<String, Row<TestItem>>> = parse_rows(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_rows_duplicate_ids_last_wins() {
        let content = "{\"id\":\"aa\",\"title\":\"First\"}\n{\"id\":\"aa\",\"title\":\"Second\"}\n";
        let rows: HashMap<String, Row<TestItem>> = parse_rows(content).unwrap();
        assert_eq!(rows.len(), 1);
        match &rows["aa"] {
            Row::Live { inner, .. } => assert_eq!(inner.title, "Second"),
            _ => panic!("expected Live row"),
        }
    }

    #[test]
    fn test_last_modified_live_with_updated_at() {
        let ts = Utc::now();
        let row: Row<TestItem> = Row::Live {
            id: "abc".to_string(),
            inner: make_item("x", "Item"),
            updated_at: Some(ts),
        };
        assert_eq!(row.last_modified(), Some(ts));
    }

    #[test]
    fn test_last_modified_tombstone() {
        let ts = Utc::now();
        let row: Row<TestItem> = Row::Tombstone {
            id: "abc".to_string(),
            deleted_at: ts,
        };
        assert_eq!(row.last_modified(), Some(ts));
    }

    #[test]
    fn test_last_modified_live_without_updated_at() {
        let row: Row<TestItem> = Row::Live {
            id: "abc".to_string(),
            inner: make_item("x", "Item"),
            updated_at: None,
        };
        assert_eq!(row.last_modified(), None);
    }
}
