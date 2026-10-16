//! Shared read-only namespace. No OS bindings and no persistent cache.
use s3drive_core::Config;
use s3drive_s3::{ListPage, ObjectMetadata, ReadHandle, S3Reader};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt,
    sync::{Arc, Mutex},
    time::Duration,
};

pub const ROOT: u64 = 1;
pub const MAX_DIRECTORY_ENTRIES: usize = 10_000;
pub const MAX_NODES: usize = 32_768;
const MAX_PAGES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Directory,
    File,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub id: u64,
    pub parent: u64,
    pub name: String,
    pub kind: Kind,
    source: Option<usize>,
    key: String,
}

#[derive(Debug)]
pub enum Error {
    Storage(s3drive_s3::Error),
    Configuration,
    NotFound,
    NotDirectory,
    IsDirectory,
    InvalidName,
    AmbiguousName,
    InvalidListing,
    ResourceLimit,
    Timeout,
    Internal,
}
pub type Result<T> = std::result::Result<T, Error>;
impl From<s3drive_s3::Error> for Error {
    fn from(value: s3drive_s3::Error) -> Self {
        Self::Storage(value)
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(e) => e.fmt(f),
            _ => write!(f, "filesystem: {self:?}"),
        }
    }
}
impl std::error::Error for Error {}

type Identity = (usize, String, Kind);
struct Nodes {
    by_id: HashMap<u64, Arc<Node>>,
    by_key: HashMap<Identity, u64>,
    next_id: u64,
}

pub struct ReadOnlyFs {
    readers: Vec<S3Reader>,
    nodes: Mutex<Nodes>,
    aliases: Vec<Arc<Node>>,
}

pub struct OpenFile {
    pub node: Arc<Node>,
    handle: ReadHandle,
}
impl OpenFile {
    pub fn metadata(&self) -> &ObjectMetadata {
        self.handle.metadata()
    }
    pub async fn read(&self, offset: u64, length: usize) -> Result<Vec<u8>> {
        Ok(self.handle.read_range(offset, length).await?)
    }
}

impl ReadOnlyFs {
    pub async fn new(config: &Config) -> Result<Self> {
        config.validate().map_err(|_| Error::Configuration)?;
        if config.sources.len() >= MAX_NODES {
            return Err(Error::ResourceLimit);
        }
        let root = Arc::new(Node {
            id: ROOT,
            parent: ROOT,
            name: String::new(),
            kind: Kind::Directory,
            source: None,
            key: String::new(),
        });
        let mut nodes = Nodes {
            by_id: HashMap::from([(ROOT, root)]),
            by_key: HashMap::new(),
            next_id: ROOT + 1,
        };
        let mut aliases = Vec::new();
        let mut readers = Vec::new();
        for (source_index, source) in config.sources.iter().enumerate() {
            readers.push(S3Reader::from_source(source).await?);
            let node = nodes.insert(
                source_index,
                "".into(),
                Kind::Directory,
                ROOT,
                source.alias.clone(),
            )?;
            aliases.push(node);
        }
        aliases.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Self {
            readers,
            nodes: Mutex::new(nodes),
            aliases,
        })
    }

    pub fn node(&self, id: u64) -> Result<Arc<Node>> {
        self.nodes
            .lock()
            .map_err(|_| Error::Internal)?
            .by_id
            .get(&id)
            .cloned()
            .ok_or(Error::NotFound)
    }

    /// A complete bounded directory view. Pagination is not an atomic S3 snapshot.
    /// No namespace lock is held while waiting for the network.
    pub async fn list(&self, id: u64) -> Result<Vec<Arc<Node>>> {
        let parent = self.node(id)?;
        if parent.kind != Kind::Directory {
            return Err(Error::NotDirectory);
        }
        let Some(source) = parent.source else {
            return Ok(self.aliases.clone());
        };
        let entries = tokio::time::timeout(Duration::from_secs(60), async {
            let mut entries = BTreeMap::new();
            let mut token = None;
            let mut seen = HashSet::new();
            for _ in 0..MAX_PAGES {
                let page = self.readers[source]
                    .list_page(&parent.key, token.as_deref(), 1000)
                    .await?;
                let next = page.next_token.clone();
                project_page(&parent.key, page, &mut entries)?;
                if entries.len() > MAX_DIRECTORY_ENTRIES {
                    return Err(Error::ResourceLimit);
                }
                match next {
                    None => return Ok(entries),
                    Some(next) => {
                        if !seen.insert(next.clone()) {
                            return Err(Error::InvalidListing);
                        }
                        token = Some(next);
                    }
                }
            }
            Err(Error::ResourceLimit)
        })
        .await
        .map_err(|_| Error::Timeout)??;
        let mut nodes = self.nodes.lock().map_err(|_| Error::Internal)?;
        // Fail before publishing a partial directory when the mount-wide cap is hit.
        let needed = entries
            .values()
            .filter(|entry| {
                !nodes
                    .by_key
                    .contains_key(&(source, entry.key.clone(), entry.kind))
            })
            .count();
        if nodes.by_id.len() + needed > MAX_NODES {
            return Err(Error::ResourceLimit);
        }
        entries
            .into_iter()
            .map(|(name, entry)| nodes.insert(source, entry.key, entry.kind, id, name))
            .collect()
    }

    pub async fn lookup(&self, parent: u64, name: &str) -> Result<Arc<Node>> {
        validate_name(name)?;
        self.list(parent)
            .await?
            .into_iter()
            .find(|node| node.name == name)
            .ok_or(Error::NotFound)
    }

    pub async fn metadata(&self, id: u64) -> Result<Option<ObjectMetadata>> {
        let node = self.node(id)?;
        match node.kind {
            Kind::Directory => Ok(None),
            Kind::File => Ok(Some(
                self.readers[node.source.ok_or(Error::Internal)?]
                    .head(&node.key)
                    .await?,
            )),
        }
    }

    pub async fn open(&self, id: u64) -> Result<OpenFile> {
        let node = self.node(id)?;
        if node.kind != Kind::File {
            return Err(Error::IsDirectory);
        }
        let handle = self.readers[node.source.ok_or(Error::Internal)?]
            .open(&node.key)
            .await?;
        Ok(OpenFile { node, handle })
    }
}

impl Nodes {
    fn insert(
        &mut self,
        source: usize,
        key: String,
        kind: Kind,
        parent: u64,
        name: String,
    ) -> Result<Arc<Node>> {
        let identity = (source, key.clone(), kind);
        if let Some(id) = self.by_key.get(&identity) {
            return self.by_id.get(id).cloned().ok_or(Error::Internal);
        }
        if self.by_id.len() >= MAX_NODES {
            return Err(Error::ResourceLimit);
        }
        let id = self.next_id;
        self.next_id = id.checked_add(1).ok_or(Error::ResourceLimit)?;
        let node = Arc::new(Node {
            id,
            parent,
            name,
            kind,
            source: Some(source),
            key,
        });
        self.by_id.insert(id, node.clone());
        self.by_key.insert(identity, id);
        Ok(node)
    }
}

#[derive(Debug)]
struct Entry {
    key: String,
    kind: Kind,
}
fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.len() > 255
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
    {
        return Err(Error::InvalidName);
    }
    Ok(())
}
fn project_page(prefix: &str, page: ListPage, entries: &mut BTreeMap<String, Entry>) -> Result<()> {
    for key in page.directories {
        let name = key
            .strip_prefix(prefix)
            .and_then(|s| s.strip_suffix('/'))
            .ok_or(Error::InvalidListing)?;
        insert_entry(
            entries,
            name.to_owned(),
            Entry {
                key,
                kind: Kind::Directory,
            },
        )?;
    }
    for object in page.objects {
        // A zero-byte object whose key equals this directory is its folder marker.
        if object.key == prefix && object.size == 0 {
            continue;
        }
        let name = object
            .key
            .strip_prefix(prefix)
            .ok_or(Error::InvalidListing)?
            .to_owned();
        insert_entry(
            entries,
            name,
            Entry {
                key: object.key,
                kind: Kind::File,
            },
        )?;
    }
    Ok(())
}
fn insert_entry(entries: &mut BTreeMap<String, Entry>, name: String, entry: Entry) -> Result<()> {
    validate_name(&name)?;
    if let Some(previous) = entries.get(&name) {
        return Err(if previous.kind == entry.kind {
            Error::InvalidListing
        } else {
            Error::AmbiguousName
        });
    }
    entries.insert(name, entry);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use s3drive_s3::ListedObject;
    fn page(files: &[(&str, u64)], dirs: &[&str]) -> ListPage {
        ListPage {
            objects: files
                .iter()
                .map(|(key, size)| ListedObject {
                    key: (*key).into(),
                    size: *size,
                    etag: None,
                    modified_unix_seconds: None,
                })
                .collect(),
            directories: dirs.iter().map(|s| (*s).into()).collect(),
            next_token: None,
        }
    }
    #[test]
    fn projects_exact_names_and_ignores_only_empty_folder_markers() {
        let mut entries = BTreeMap::new();
        project_page(
            "dir/",
            page(
                &[
                    ("dir/", 0),
                    ("dir/a%2Fb + café", 3),
                    ("dir/Case", 0),
                    ("dir/case", 0),
                ],
                &["dir/sub/"],
            ),
            &mut entries,
        )
        .unwrap();
        assert_eq!(
            entries.keys().map(String::as_str).collect::<Vec<_>>(),
            ["Case", "a%2Fb + café", "case", "sub"]
        );
        assert_eq!(entries["sub"].key, "dir/sub/");
        assert!(project_page("dir/", page(&[("dir/", 5)], &[]), &mut BTreeMap::new()).is_err());
    }
    #[test]
    fn fails_whole_directory_on_collisions_duplicates_and_invalid_projection() {
        for bad in [
            page(&[("a", 0)], &["a/"]),
            page(&[("a", 0), ("a", 0)], &[]),
            page(&[("../x", 0)], &[]),
            page(&[("a/b", 0)], &[]),
            page(&[("a\\b", 0)], &[]),
            page(&[("\0", 0)], &[]),
            page(&[(".", 0)], &[]),
            page(&[("", 4)], &[]),
            page(&[(&"é".repeat(128), 0)], &[]),
        ] {
            assert!(project_page("", bad, &mut BTreeMap::new()).is_err());
        }
        assert!(
            project_page(
                "prefix/",
                page(&[("outside", 0)], &[]),
                &mut BTreeMap::new()
            )
            .is_err()
        );
        let mut entries = BTreeMap::new();
        project_page("", page(&[("a", 0)], &[]), &mut entries).unwrap();
        assert!(matches!(
            project_page("", page(&[], &["a/"]), &mut entries),
            Err(Error::AmbiguousName)
        ));
    }
    #[test]
    fn stable_inodes_do_not_alias_sources_or_types() {
        let mut nodes = Nodes {
            by_id: HashMap::new(),
            by_key: HashMap::new(),
            next_id: 2,
        };
        let a = nodes
            .insert(0, "file".into(), Kind::File, 1, "file".into())
            .unwrap();
        let same = nodes
            .insert(0, "file".into(), Kind::File, 1, "file".into())
            .unwrap();
        let other = nodes
            .insert(1, "file".into(), Kind::File, 1, "file".into())
            .unwrap();
        let dir = nodes
            .insert(0, "file/".into(), Kind::Directory, 1, "file".into())
            .unwrap();
        assert_eq!(a.id, same.id);
        assert_ne!(a.id, other.id);
        assert_ne!(a.id, dir.id);
    }
}
