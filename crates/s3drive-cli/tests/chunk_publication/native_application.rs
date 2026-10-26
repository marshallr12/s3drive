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
    eprintln!(
        "HOSTED_APPLICATION git={version:?} initial={first_hash} final={second_hash} create=exact replace=exact rename=exact delete=exact held_reader=exact interrupted_daemon_cold_read=exact"
    );
}
