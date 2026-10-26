//! Real Git CLI against the real mounted filesystem; loopback backend only.
//! Not a GUI application, physical reboot, Git repository-on-drive or provider test.
use super::*;
use std::ffi::OsStr;

async fn git_output(root: &Path, mount: &Path, args: &[&OsStr]) -> std::process::Output {
    let mut c = tokio::process::Command::new("git");
    c.current_dir(mount)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", root.join("empty-git-config"))
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GIT_CONFIG_COUNT")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_DIR")
        .args(["-c", "core.autocrlf=false", "-c", "core.filemode=false"])
        .arg(format!(
            "--git-dir={}",
            root.join("application.git").display()
        ));
    if args.first() != Some(&OsStr::new("init")) {
        c.arg(format!("--work-tree={}", mount.display()));
    }
    c.args(args).stdin(Stdio::null()).kill_on_drop(true);
    let out = tokio::time::timeout(Duration::from_secs(20), c.output())
        .await
        .expect("Git application deadline")
        .unwrap();
    assert!(out.stdout.len() <= 65536 && out.stderr.len() <= 65536);
    out
}
async fn git(root: &Path, mount: &Path, args: &[&OsStr]) -> String {
    let out = git_output(root, mount, args).await;
    assert!(
        out.status.success(),
        "Git {:?}: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.len() <= 65536 && out.stderr.len() <= 65536);
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}
async fn stage(root: &Path, mount: &Path, bytes: &[u8]) -> String {
    std::fs::write(root.join("document-source"), bytes).unwrap();
    let hash = git(
        root,
        mount,
        &[
            "hash-object".as_ref(),
            "-w".as_ref(),
            root.join("document-source").as_os_str(),
        ],
    )
    .await;
    assert_eq!(hash.len(), 40);
    git(
        root,
        mount,
        &[
            "update-index".as_ref(),
            "--add".as_ref(),
            "--cacheinfo".as_ref(),
            "100644".as_ref(),
            hash.as_ref(),
            NAME.as_ref(),
        ],
    )
    .await;
    hash
}
async fn checkout(root: &Path, mount: &Path) {
    let prefix = format!("--prefix={}/", mount.display());
    git(
        root,
        mount,
        &[
            "checkout-index".as_ref(),
            "--force".as_ref(),
            "--all".as_ref(),
            prefix.as_ref(),
        ],
    )
    .await;
}
const NAME: &str = "application 日本語 🦀.txt";
const MOVED: &str = "renamed application 日本語.txt";
const FIRST: &[u8] = b"first document\r\nretained reader\0binary tail";
const SECOND: &[u8] = b"second document\nshort";

pub async fn mounted(config: &s3drive_core::Config, root: &Path, s: &Server) {
    use native_mount::{Mount, blocking};
    native_lifecycle::mount_configuration(config, root, s).await;
    std::fs::write(root.join("empty-git-config"), b"").unwrap();
    let mut mount = Mount::start(root).await;
    let base = mount.path.clone();
    // All Git metadata is private and outside the mounted drive; no hooks,
    // remotes, credential helpers, commits, global config or identity changes.
    git(root, &base, &["init".as_ref(), "--bare".as_ref()]).await;
    let version = git(root, &base, &["--version".as_ref()]).await;
    let first_hash = stage(root, &base, FIRST).await;
    checkout(root, &base).await;
    let path = base.join(NAME);
    let mut reader = blocking(&mut mount, move || {
        assert_eq!(std::fs::read(&path).unwrap(), FIRST);
        std::fs::File::open(path).unwrap()
    })
    .await;
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        drop(reader);
        let path = base.join(NAME);
        let exclusive = blocking(&mut mount, move || {
            std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(path)
                .unwrap()
        })
        .await;
        let refused = git_output(
            root,
            &base,
            &[
                "hash-object".as_ref(),
                "--no-filters".as_ref(),
                base.join(NAME).as_os_str(),
            ],
        )
        .await;
        assert!(
            !refused.status.success(),
            "Git read ignored Windows exclusive sharing"
        );
        drop(exclusive);
        let path = base.join(NAME);
        reader = blocking(&mut mount, move || std::fs::File::open(path).unwrap()).await;
        eprintln!("HOSTED_APPLICATION windows_exclusive_read=refused unlocked_reopen=ok");
    }
    let second_hash = stage(root, &base, SECOND).await;
    checkout(root, &base).await;
    assert_eq!(
        git(
            root,
            &base,
            &[
                "hash-object".as_ref(),
                "--no-filters".as_ref(),
                base.join(NAME).as_os_str()
            ]
        )
        .await,
        second_hash
    );
    git(
        root,
        &base,
        &["mv".as_ref(), "--".as_ref(), NAME.as_ref(), MOVED.as_ref()],
    )
    .await;
    git(
        root,
        &base,
        &[
            "rm".as_ref(),
            "--force".as_ref(),
            "--".as_ref(),
            MOVED.as_ref(),
        ],
    )
    .await;
    let check_base = base.clone();
    blocking(&mut mount, move || {
        use std::io::Read;
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).unwrap();
        assert_eq!(
            bytes, FIRST,
            "Git replacement/unlink must preserve old reader"
        );
        assert!(!check_base.join(NAME).exists() && !check_base.join(MOVED).exists());
    })
    .await;
    // Fresh external process recreation, then acknowledged data followed by
    // abrupt *owned daemon* termination. This is not a machine reboot.
    assert_eq!(stage(root, &base, SECOND).await, second_hash);
    checkout(root, &base).await;
    let path = base.join(NAME);
    blocking(&mut mount, move || {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap()
            .sync_all()
            .unwrap();
    })
    .await;
    mount.interrupt().await;
    drop(mount);
    let mut mount = Mount::start(root).await;
    let base = mount.path.clone();
    assert_eq!(
        git(
            root,
            &base,
            &[
                "hash-object".as_ref(),
                "--no-filters".as_ref(),
                base.join(NAME).as_os_str()
            ]
        )
        .await,
        second_hash
    );
    blocking(&mut mount, move || {
        assert_eq!(std::fs::read(base.join(NAME)).unwrap(), SECOND);
        assert!(!base.join(MOVED).exists());
    })
    .await;
    mount.stop().await;
    drop(mount);
    std::fs::write(
        root.join("application-operations.json"),
        serde_json::to_vec(&s.proxy.applies()).unwrap(),
    )
    .unwrap();
    eprintln!(
        "HOSTED_APPLICATION git={version:?} initial={first_hash} final={second_hash} create=exact replace=exact rename=exact delete=exact held_reader=exact interrupted_daemon_cold_read=exact"
    );
}

// Independently cross-check three application saves against actual object bytes
// and canonical authority identities, not just a guessed aggregate PUT count.
type ApplicationObjects = std::collections::BTreeMap<String, (Vec<u8>, String)>;
fn physical_key(source: &s3drive_core::Source, key: &str) -> String {
    format!("/{}/{}{}", source.bucket, source.prefix, key)
}
pub fn verify_objects(root: &Path, f: &fixture::Fixture) {
    let config = s3drive_core::Config::parse(
        &std::fs::read_to_string(root.join("native-drive.toml")).unwrap(),
    )
    .unwrap();
    let applies: Vec<Apply> =
        serde_json::from_slice(&std::fs::read(root.join("application-operations.json")).unwrap())
            .unwrap();
    verify_map(applies, &f.objects.lock().unwrap(), &config.sources[0]);
}
fn verify_map(applies: Vec<Apply>, objects: &ApplicationObjects, source: &s3drive_core::Source) {
    use s3drive_core::manifest::{
        Entry,
        objects::{Namespace, Role, RootDescriptor},
    };
    use sha2::{Digest, Sha256};
    use std::collections::{BTreeMap, BTreeSet};
    let mut unique = BTreeMap::new();
    let mut ordered = Vec::new();
    for a in applies {
        if let Some(previous) = unique.insert(a.operation.clone(), a.clone()) {
            assert_eq!(previous, a, "operation retry changed identity/input");
        } else {
            ordered.push(a);
        }
    }
    // Namespace keys are relative to the parsed configured source.
    let mut expected = BTreeSet::new();
    let mut creators = BTreeSet::new();
    let mut reservations = BTreeSet::new();
    let mut published = BTreeSet::new();
    let mut contents = Vec::new();
    for a in &ordered {
        match &a.mutation {
            Mutation::CreateEmptyManifest { store, name, .. }
            | Mutation::CreateDeferredEmptyManifest { store, name, .. } => {
                assert_eq!(name, NAME);
                assert!(creators.insert(a.operation.clone()));
                let (descriptor, bytes) = RootDescriptor::empty(
                    binary(&a.volume),
                    binary(&a.operation),
                    binary(store),
                    binary(&a.operation),
                )
                .unwrap();
                let ns = Namespace::new(
                    "writable/.s3drive/".into(),
                    binary(&a.volume),
                    binary(&a.operation),
                    binary(store),
                )
                .unwrap();
                let key = physical_key(source, ns.root_key(&descriptor).unwrap().as_str());
                assert_eq!(
                    objects
                        .get(&key)
                        .unwrap_or_else(|| panic!(
                            "missing canonical empty object {key}; actual keys {:?}",
                            objects.keys().collect::<Vec<_>>()
                        ))
                        .0,
                    bytes
                );
                assert!(expected.insert(key));
            }
            Mutation::ReserveManifest {
                node,
                store,
                manifest,
                ..
            } => {
                assert!(creators.contains(node));
                assert!(reservations.insert(a.operation.clone()));
                let descriptor = manifest.root().unwrap();
                let ns = Namespace::new(
                    "writable/.s3drive/".into(),
                    binary(&a.volume),
                    binary(node),
                    binary(store),
                )
                .unwrap();
                let key = physical_key(source, ns.root_key(&descriptor).unwrap().as_str());
                let page = descriptor
                    .verify_page(&objects.get(&key).unwrap().0)
                    .unwrap();
                assert!(expected.insert(key));
                let [Entry::Data { length, object }] = page.page().entries() else {
                    panic!("expected exactly one data extent")
                };
                let key = physical_key(source, ns.key(Role::Data, object).unwrap().as_str());
                let bytes = &objects.get(&key).unwrap().0;
                assert_eq!(*length, bytes.len() as u64);
                assert_eq!(object.sha256().as_slice(), Sha256::digest(bytes).as_slice());
                contents.push(bytes.clone());
                assert!(expected.insert(key));
            }
            Mutation::Publish { reservation, .. } => {
                assert!(published.insert(reservation.clone()));
            }
            _ => {}
        }
    }
    assert_eq!(creators.len(), 3);
    assert_eq!(reservations.len(), 3);
    assert_eq!(published, reservations);
    assert_eq!(contents, [FIRST.to_vec(), SECOND.to_vec(), SECOND.to_vec()]);
    assert_eq!(expected.len(), 9);
    let actual = objects
        .keys()
        .filter(|k| k.starts_with(&physical_key(source, "writable/.s3drive/")))
        .cloned()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual, expected,
        "unexpected or missing immutable object identity"
    );
    eprintln!(
        "HOSTED_APPLICATION_PROVENANCE creators=3 exact_published_saves=3 canonical_objects=9 payloads=exact"
    );
}

fn constructed_objects(
    bucket: &str,
    prefix: &str,
) -> (Vec<Apply>, ApplicationObjects, s3drive_core::Source) {
    use s3drive_core::manifest::{
        Entry, Page, Reference,
        objects::{Namespace, Role, RootDescriptor},
    };
    use s3drive_state::namespace::{ContentGuard, ManifestRoot};
    use sha2::{Digest, Sha256};
    let config = s3drive_core::Config::parse(&format!("version=1\nmode='read-only'\n[[sources]]\nalias='docs'\nbucket='{bucket}'\nregion='us-east-1'\nprefix='{prefix}'\n")).unwrap();
    let source = config.sources.into_iter().next().unwrap();
    let mut records = Vec::new();
    let mut objects = ApplicationObjects::new();
    for (i, bytes) in [FIRST, SECOND, SECOND].into_iter().enumerate() {
        let volume = id(1);
        let store = id(200);
        let node = id(100 + i as u64);
        let generation = id(1000 + i as u64);
        let reserve = id(2000 + i as u64);
        let ns = Namespace::new(
            "writable/.s3drive/".into(),
            binary(&volume),
            binary(&node),
            binary(&store),
        )
        .unwrap();
        let (empty, body) = RootDescriptor::empty(
            binary(&volume),
            binary(&node),
            binary(&store),
            binary(&node),
        )
        .unwrap();
        objects.insert(
            format!(
                "/{bucket}/{prefix}{}",
                ns.root_key(&empty).unwrap().as_str()
            ),
            (body, "fixture-etag".into()),
        );
        records.push(Apply {
            volume: volume.clone(),
            operation: node.clone(),
            expected_sequence: 1,
            mutation: Mutation::CreateEmptyManifest {
                parent: s3drive_authority::ROOT.into(),
                parent_revision: 1,
                name: NAME.into(),
                store: store.clone(),
                upload_operation: "a".repeat(64),
                etag: "fixture-etag".into(),
                version: None,
            },
        });
        let data = Reference::new(
            binary(&store),
            binary(&generation),
            0,
            Sha256::digest(bytes).into(),
        )
        .unwrap();
        objects.insert(
            format!(
                "/{bucket}/{prefix}{}",
                ns.key(Role::Data, &data).unwrap().as_str()
            ),
            (bytes.to_vec(), "fixture-etag".into()),
        );
        let page = Page::new(
            binary(&volume),
            binary(&node),
            0,
            0,
            vec![Entry::Data {
                length: bytes.len() as u64,
                object: data,
            }],
        )
        .unwrap();
        let descriptor = RootDescriptor::new(
            binary(&volume),
            binary(&node),
            bytes.len() as u64,
            Reference::new(binary(&store), binary(&generation), 1 << 56, page.digest()).unwrap(),
        )
        .unwrap();
        let body = page.encode();
        objects.insert(
            format!(
                "/{bucket}/{prefix}{}",
                ns.root_key(&descriptor).unwrap().as_str()
            ),
            (body.clone(), "fixture-etag".into()),
        );
        records.push(Apply {
            volume: volume.clone(),
            operation: reserve.clone(),
            expected_sequence: 2,
            mutation: Mutation::ReserveManifest {
                node,
                expected_revision: 1,
                store,
                size: body.len() as u64,
                sha256: page.digest().iter().map(|b| format!("{b:02x}")).collect(),
                manifest: Box::new(ManifestRoot::new(descriptor).unwrap()),
                guard: ContentGuard {
                    expected: None,
                    detached: false,
                    removal: None,
                    removal_claim: None,
                },
            },
        });
        records.push(Apply {
            volume,
            operation: id(3000 + i as u64),
            expected_sequence: 3,
            mutation: Mutation::Publish {
                reservation: reserve,
                upload_operation: "b".repeat(64),
                etag: "fixture-etag".into(),
                version: None,
            },
        });
    }
    (records, objects, source)
}
#[test]
fn application_object_provenance_accepts_exact_configured_three_saves() {
    for (bucket, prefix) in [
        ("test-bucket", "desktop/"),
        ("other-bucket", "nested/prefix/"),
    ] {
        let (mut records, objects, source) = constructed_objects(bucket, prefix);
        assert_eq!(objects.len(), 9);
        records.push(records[0].clone()); // Exact original retries are not extra saves.
        verify_map(records, &objects, &source);
    }
}
#[test]
fn application_object_provenance_refuses_missing_extra_corrupt_and_foreign_evidence() {
    let (records, objects, source) = constructed_objects("test-bucket", "desktop/");
    for case in [
        "missing", "extra", "data", "page", "empty", "prefix", "bucket", "publish", "retry",
    ] {
        let mut records = records.clone();
        let mut objects = objects.clone();
        let mut source = source.clone();
        match case {
            "missing" => {
                objects.pop_first();
            }
            "extra" => {
                objects.insert(
                    "/test-bucket/desktop/writable/.s3drive/unexpected".into(),
                    (vec![1], "fixture".into()),
                );
            }
            "data" | "page" | "empty" => {
                let key = objects
                    .keys()
                    .find(|key| match case {
                        "data" => key.contains("/data/"),
                        "empty" => key.contains(&format!("/{}/page/", id(100))),
                        _ => key.contains(&format!("/{}/page/", id(1000))),
                    })
                    .unwrap()
                    .clone();
                objects.get_mut(&key).unwrap().0[0] ^= 1;
            }
            "prefix" => source.prefix = "wrong/".into(),
            "bucket" => source.bucket = "wrong-bucket".into(),
            "publish" => {
                records.pop();
            }
            "retry" => {
                let mut changed = records[0].clone();
                changed.expected_sequence += 1;
                records.push(changed);
            }
            _ => unreachable!(),
        }
        assert!(
            std::panic::catch_unwind(|| verify_map(records, &objects, &source)).is_err(),
            "accepted {case}"
        );
    }
}
