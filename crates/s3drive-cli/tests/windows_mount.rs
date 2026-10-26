//! Explicit native Windows acceptance; a missing driver is a failure, not a skip.
#![cfg(windows)]
#[path = "support/cache.rs"]
mod cache;
#[path = "support/creation.rs"]
mod creation;
#[path = "support/fixture.rs"]
mod fixture;
#[path = "support/unicode.rs"]
mod unicode;
#[path = "support/writes.rs"]
mod writes;
use fixture::Fixture;
use std::{
    collections::BTreeSet,
    io::{Read, Seek, SeekFrom},
    os::windows::fs::{FileExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
};

async fn ready(child: &mut Child) {
    let mut lines = BufReader::new(child.stderr.take().unwrap()).lines();
    assert!(
        tokio::time::timeout(Duration::from_secs(30), async {
            while let Some(line) = lines.next_line().await.unwrap() {
                eprintln!("{line}");
                if line.starts_with("Mounted ") {
                    return true;
                }
            }
            false
        })
        .await
        .unwrap(),
        "Windows mount failed"
    );
}
async fn stop(child: &mut Child) {
    drop(child.stdin.take());
    let status = tokio::time::timeout(Duration::from_secs(15), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(status.success(), "mount daemon failed: {status}");
}
fn names(path: &Path) -> BTreeSet<String> {
    std::fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "native write acceptance: Windows with WinFsp 2.1"]
async fn actual_windows_existing_file_writes_and_restart() {
    let mut fixture = Fixture::start().await;
    let temp = tempfile::tempdir().unwrap();
    let setup = writes::setup(&fixture, temp.path()).await;
    let mount = temp.path().join("write-mount");
    let mut command = setup.command(&mount);
    command
        .arg("--unmount-on-stdin-close")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().unwrap();
    ready(&mut child).await;
    tokio::task::block_in_place(|| writes::exercise(&mount, &fixture));
    let expected = tokio::task::block_in_place(|| writes::uncertain_write(&mount, &fixture));
    let puts = writes::put_count(&fixture);
    stop(&mut child).await;
    writes::restore_observation(&fixture);
    let mut command = setup.command(&mount);
    command
        .arg("--unmount-on-stdin-close")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().unwrap();
    ready(&mut child).await;
    let path = mount.join("docs/writable").join(writes::NAME);
    assert_eq!(
        tokio::task::spawn_blocking(move || std::fs::read(path).unwrap())
            .await
            .unwrap(),
        expected
    );
    assert_eq!(
        puts,
        writes::put_count(&fixture),
        "restart replayed an uncertain PUT"
    );
    eprintln!("PASS: native uncertain-write restart observed and acknowledged without another PUT");
    let rejected = tokio::task::block_in_place(|| writes::conflict_write(&mount, &fixture));
    stop(&mut child).await;
    writes::inspect_conflict(&setup.state, &rejected).await;
    assert!(!fixture.server.is_finished());
    assert!(!fixture.server.is_finished());
    fixture.server.abort();
    let _ = (&mut fixture.server).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "native acceptance: Windows MSVC with installed WinFsp 2.1 driver"]
async fn actual_windows_drive_names_reads_security_and_teardown() {
    let mut fixture = Fixture::start().await;
    unicode::seed(&fixture);
    let data: Vec<u8> = (0..350_123).map(|n| (n % 251) as u8).collect();
    fixture.put("/test-bucket/desktop/large.bin", &data, "large-v1");
    fixture.put("/test-bucket/desktop/nested/", b"", "marker");
    fixture.put(
        "/test-bucket/desktop/nested/a%2Fb + café.txt",
        b"literal key bytes",
        "literal",
    );
    fixture.put("/test-bucket/desktop/empty", b"", "empty");
    fixture.put("/other-bucket/readme", b"other bucket", "other");
    for i in 0..310 {
        fixture.put(
            &format!("/test-bucket/desktop/many/item-{i:04}"),
            b"x",
            "one",
        );
    }
    fixture.put("/test-bucket/desktop/collision/Case", b"one", "one");
    fixture.put("/test-bucket/desktop/collision/case", b"two", "two");
    fixture.put("/test-bucket/desktop/unicode-collision/café", b"one", "one");
    fixture.put("/test-bucket/desktop/unicode-collision/CAFÉ", b"two", "two");
    fixture.put("/test-bucket/desktop/invalid/NUL.txt", b"one", "one");
    fixture.put("/test-bucket/desktop/ambiguous/file", b"one", "one");
    fixture.put("/test-bucket/desktop/ambiguous/file/child", b"two", "two");
    let dir = tempfile::tempdir().unwrap();
    let letter = (b'M'..=b'Z')
        .rev()
        .find(|c| !Path::new(&format!("{}:\\", *c as char)).exists())
        .expect("no unused test drive");
    let drive = format!("{}:", letter as char);
    let mount = PathBuf::from(format!("{drive}\\"));
    let config = dir.path().join("drive.toml");
    std::fs::write(&config,format!("version=1\nmode='read-only'\n[[sources]]\nalias='docs'\nbucket='test-bucket'\nregion='us-east-1'\nprefix='desktop/'\nendpoint='{}'\nprofile='fixture'\npath_style=true\nallow_http_loopback=true\n[[sources]]\nalias='other'\nbucket='other-bucket'\nregion='us-east-1'\nendpoint='{}'\nprofile='fixture'\npath_style=true\nallow_http_loopback=true\n",fixture.endpoint,fixture.endpoint)).unwrap();
    let credentials = dir.path().join("credentials");
    std::fs::write(
        &credentials,
        "[fixture]\naws_access_key_id=TESTPROFILE\naws_secret_access_key=TESTPROFILESECRET\n",
    )
    .unwrap();
    let aws_config = dir.path().join("aws-config");
    std::fs::write(&aws_config, "").unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_s3drive"));
    command
        .arg("mount")
        .arg(&config)
        .arg(&drive)
        .arg("--unmount-on-stdin-close")
        .env("AWS_SHARED_CREDENTIALS_FILE", &credentials)
        .env("AWS_CONFIG_FILE", &aws_config)
        .env("AWS_EC2_METADATA_DISABLED", "true")
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .kill_on_drop(true);
    let mut child = command.spawn().unwrap();
    ready(&mut child).await;
    // A second daemon must not steal or silently choose a different drive.
    let occupied = tokio::time::timeout(Duration::from_secs(15), command.output())
        .await
        .unwrap()
        .unwrap();
    assert!(
        !occupied.status.success(),
        "occupied drive mount unexpectedly succeeded"
    );
    let path = mount.clone();
    let objects = fixture.objects.clone();
    let mut checks = tokio::task::spawn_blocking(move || {
        unicode::verify(&path.join("docs/unicode"));
        assert_eq!(
            names(&path),
            BTreeSet::from(["docs".into(), "other".into()])
        );
        assert_eq!(
            std::fs::read(path.join("OTHER/README")).unwrap(),
            b"other bucket"
        );
        assert_eq!(
            std::fs::read(path.join("docs/nested/A%2fB + CAFÉ.TXT")).unwrap(),
            b"literal key bytes"
        );
        assert!(std::fs::read(path.join("docs/empty")).unwrap().is_empty());
        assert_eq!(
            names(&path.join("docs/many")),
            (0..310).map(|n| format!("item-{n:04}")).collect()
        );
        let large = path.join("docs/large.bin");
        let mut file = std::fs::File::open(&large).unwrap();
        let mut sequential = Vec::new();
        file.read_to_end(&mut sequential).unwrap();
        assert_eq!(sequential, data);
        for offset in [0, 1, 65535, 262144, data.len() - 10, data.len()] {
            let mut buffer = [0; 79];
            let len = file.seek_read(&mut buffer, offset as u64).unwrap();
            assert_eq!(&buffer[..len], &data[offset..(offset + 79).min(data.len())]);
        }
        std::thread::scope(|scope| {
            for n in 0..8 {
                let file = &file;
                let data = &data;
                scope.spawn(move || {
                    let mut buffer = [0; 1024];
                    assert_eq!(file.seek_read(&mut buffer, n * 4096).unwrap(), 1024);
                    assert_eq!(&buffer, &data[n as usize * 4096..n as usize * 4096 + 1024]);
                });
            }
        });
        assert!(std::fs::write(path.join("docs/new"), b"no").is_err());
        assert!(std::fs::remove_file(&large).is_err());
        assert!(std::fs::rename(&large, path.join("docs/renamed")).is_err());
        assert!(
            std::fs::OpenOptions::new()
                .write(true)
                .open(&large)
                .is_err()
        );
        for directory in ["collision", "unicode-collision", "invalid", "ambiguous"] {
            assert!(
                std::fs::read_dir(path.join("docs").join(directory))
                    .and_then(|i| i.collect::<std::io::Result<Vec<_>>>())
                    .is_err(),
                "bad names leaked: {directory}"
            );
        }
        // Windows sharing is enforced by the real driver, across independent opens.
        let exclusive_path = path.join("other/readme");
        let exclusive = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&exclusive_path)
            .unwrap();
        assert!(std::fs::File::open(&exclusive_path).is_err());
        drop(exclusive);
        assert!(std::fs::File::open(&exclusive_path).is_ok());
        objects.lock().unwrap().insert(
            "/test-bucket/desktop/large.bin".into(),
            (b"changed".to_vec(), "\"large-v2\"".into()),
        );
        file.seek(SeekFrom::Start(0)).unwrap();
        assert!(
            file.read(&mut [0; 3]).is_err(),
            "old handle served a replaced revision"
        );
        drop(file);
        assert_eq!(std::fs::read(&large).unwrap(), b"changed");
    });
    let result = tokio::time::timeout(Duration::from_secs(120), &mut checks).await;
    if !matches!(result, Ok(Ok(()))) {
        stop(&mut child).await;
        if result.is_err() {
            let _ = tokio::time::timeout(Duration::from_secs(65), checks).await;
        }
        panic!("Windows filesystem checks failed: {result:?}");
    }
    // PowerShell filesystem and Explorer's Shell.Application namespace/ACL APIs.
    let script = format!(
        r#"$ErrorActionPreference='Stop'; Import-Module (Join-Path $PSHOME 'Modules\Microsoft.PowerShell.Security\Microsoft.PowerShell.Security.psd1'); if ((Get-ChildItem -LiteralPath '{drive}\').Count -ne 2) {{ throw 'aliases' }}; if ([IO.File]::ReadAllText('{drive}\other\readme') -ne 'other bucket') {{ throw 'read' }}; $acl=Get-Acl -LiteralPath '{drive}\other\readme'; $sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value; $rules=$acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier]); if ($rules.Count -ne 1 -or $rules[0].IdentityReference.Value -ne $sid -or (([int]$rules[0].FileSystemRights -band 2) -ne 0)) {{ throw 'owner read-only ACL' }}; $shell=New-Object -ComObject Shell.Application; $folder=$shell.NameSpace('{drive}\'); if ($null -eq $folder -or $folder.Items().Count -ne 2) {{ throw 'Explorer namespace' }}; Write-Output 'PowerShell, owner ACL, and Explorer Shell namespace verified'"#
    );
    let shell = tokio::time::timeout(
        Duration::from_secs(30),
        Command::new("powershell.exe")
            // Avoid inheriting PowerShell 7 module paths into Windows PowerShell.
            .env_remove("PSModulePath")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    eprintln!("{}", String::from_utf8_lossy(&shell.stdout));
    assert!(
        shell.status.success(),
        "{}",
        String::from_utf8_lossy(&shell.stderr)
    );
    // The upstream volume-information subset only reads volume metadata; its
    // file tests create mutable fixtures and do not fit a read-only filesystem.
    let upstream = std::env::var_os("S3DRIVE_WINFSP_TEST_EXE")
        .expect("set S3DRIVE_WINFSP_TEST_EXE to the official WinFsp 2.1 x64 test executable");
    let upstream = tokio::time::timeout(
        Duration::from_secs(30),
        Command::new(upstream)
            .args(["--external", "+getvolinfo_test"])
            .current_dir(&mount)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    eprintln!("{}", String::from_utf8_lossy(&upstream.stdout));
    assert!(
        upstream.status.success(),
        "WinFsp volume conformance exited {}: {}",
        upstream.status,
        String::from_utf8_lossy(&upstream.stderr)
    );
    assert!(
        String::from_utf8_lossy(&upstream.stdout).contains("getvolinfo_test"),
        "upstream test did not execute"
    );
    stop(&mut child).await;
    assert!(!mount.exists(), "drive remains after normal teardown");
    // A folder provides one stable path across logon sessions. WinFsp must
    // create/remove only its new directory, never reuse an existing directory.
    let folder = dir.path().join("mounted café 文書");
    let folder_command = |point: &Path| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_s3drive"));
        command
            .args(["mount"])
            .arg(&config)
            .arg(point)
            .arg("--unmount-on-stdin-close")
            .env("AWS_SHARED_CREDENTIALS_FILE", &credentials)
            .env("AWS_CONFIG_FILE", &aws_config)
            .env("AWS_EC2_METADATA_DISABLED", "true")
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .stdout(Stdio::null())
            .kill_on_drop(true);
        command
    };
    let existing = dir.path().join("existing");
    std::fs::create_dir(&existing).unwrap();
    std::fs::write(existing.join("keep"), b"untouched").unwrap();
    for rejected in [
        &existing,
        &dir.path().join("missing/child"),
        Path::new("relative"),
        Path::new("C:relative"),
    ] {
        let result =
            tokio::time::timeout(Duration::from_secs(15), folder_command(rejected).output())
                .await
                .unwrap()
                .unwrap();
        assert!(
            !result.status.success(),
            "invalid directory mount succeeded"
        );
    }
    assert_eq!(std::fs::read(existing.join("keep")).unwrap(), b"untouched");
    let mut folder_child = folder_command(&folder).spawn().unwrap();
    ready(&mut folder_child).await;
    let occupied = tokio::time::timeout(Duration::from_secs(15), folder_command(&folder).output())
        .await
        .unwrap()
        .unwrap();
    assert!(
        !occupied.status.success(),
        "occupied directory mount succeeded"
    );
    let path = folder.clone();
    tokio::task::spawn_blocking(move || {
        unicode::verify(&path.join("docs/unicode"));
        assert_eq!(
            std::fs::read(path.join("other/readme")).unwrap(),
            b"other bucket"
        );
        assert!(std::fs::write(path.join("docs/new"), b"no").is_err());
    })
    .await
    .unwrap();
    stop(&mut folder_child).await;
    assert!(
        std::fs::symlink_metadata(&folder).is_err(),
        "directory remains after normal teardown"
    );
    let mut folder_child = folder_command(&folder).spawn().unwrap();
    ready(&mut folder_child).await;
    folder_child.kill().await.unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        while std::fs::symlink_metadata(&folder).is_ok() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("directory remains after mount process termination");
    assert_eq!(std::fs::read(existing.join("keep")).unwrap(), b"untouched");
    fixture.put("/other-bucket/stalled", b"stalled", "stalled");
    let mut child = command.spawn().unwrap();
    ready(&mut child).await;
    let stalled = mount.join("other/stalled");
    let reading = tokio::task::spawn_blocking(move || std::fs::read(stalled));
    tokio::time::timeout(Duration::from_secs(15), fixture.stalled.notified())
        .await
        .unwrap();
    stop(&mut child).await;
    assert!(
        tokio::time::timeout(Duration::from_secs(10), reading)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    assert!(!mount.exists(), "drive remains after cancellation");
    // Separate logon sessions use an isolated non-administrator account. The
    // fixture stays live while Task Scheduler owns the daemon independently.
    let startup = tokio::time::timeout(
        Duration::from_secs(360),
        Command::new("powershell.exe")
            .env_remove("PSModulePath")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../scripts/test-windows-session.ps1"),
            )
            .arg("-Executable")
            .arg(
                std::env::var_os("S3DRIVE_TEST_PACKAGED_CLI")
                    .unwrap_or_else(|| env!("CARGO_BIN_EXE_s3drive").into()),
            )
            .arg("-Config")
            .arg(&config)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    eprintln!("{}", String::from_utf8_lossy(&startup.stdout));
    assert!(
        startup.status.success(),
        "Windows session acceptance failed: {}",
        String::from_utf8_lossy(&startup.stderr)
    );
    assert!(String::from_utf8_lossy(&startup.stdout).contains("PASS: non-admin registration"));
    cache::verify(&fixture, dir.path()).await;
    assert!(!fixture.server.is_finished(), "fixture server failed");
    {
        let requests = fixture.requests.lock().unwrap();
        assert!(requests.iter().any(|r| r.contains("continuation-token=")));
        assert!(
            requests
                .iter()
                .all(|r| r.starts_with("GET ") || r.starts_with("HEAD "))
        );
        eprintln!(
            "Verified {} read-only S3 requests through drive and directory WinFsp sessions",
            requests.len()
        );
    }
    fixture.server.abort();
    let _ = (&mut fixture.server).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "native creation acceptance: requires the installed OS filesystem driver"]
async fn actual_windows_file_creation_restart_and_races() {
    let mut fixture = Fixture::start().await;
    let temp = tempfile::tempdir().unwrap();
    let setup = writes::setup(&fixture, temp.path()).await;
    creation::prepare(&fixture);
    let mount = temp.path().join("creation-mount");
    let mut puts = 0;
    for pass in 0..2 {
        let mut child = setup
            .command(&mount)
            .arg("--unmount-on-stdin-close")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        ready(&mut child).await;
        if pass == 0 {
            tokio::task::block_in_place(|| creation::exercise(&mount, &fixture));
            tokio::task::block_in_place(|| creation::uncertain(&mount, &fixture));
            puts = writes::put_count(&fixture);
        } else {
            tokio::task::block_in_place(|| creation::recovered(&mount, &fixture, puts));
            tokio::task::block_in_place(|| creation::rival_and_denied(&mount, &fixture));
        }
        stop(&mut child).await;
        writes::restore_observation(&fixture);
    }
    creation::inspect(&setup.state).await;
    assert!(!fixture.server.is_finished());
    fixture.server.abort();
    let _ = (&mut fixture.server).await;
}

#[path = "support/managed.rs"]
#[allow(dead_code)]
mod managed;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn managed_native_child() {
    managed::child().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "native managed acceptance: requires the real OS filesystem driver"]
async fn actual_managed_namespace_and_immutable_reads() {
    managed::native().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "native managed write acceptance: requires the real OS filesystem driver"]
async fn actual_managed_existing_file_writes_and_recovery() {
    managed::native_writes().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "native metadata flush acceptance: requires the real OS filesystem driver"]
async fn actual_managed_metadata_flush_burst() {
    managed::native_flush_burst().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "native managed namespace acceptance: requires the real OS filesystem driver"]
async fn actual_managed_file_and_directory_creation() {
    managed::native_creation().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "native managed mutation acceptance: requires the real OS filesystem driver"]
async fn actual_managed_rename_replace_and_retained_handles() {
    managed::native_mutations().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "native complete disposition lifecycle: Windows with WinFsp 2.1"]
async fn actual_managed_windows_disposition_lifecycle() {
    managed::native_lifecycle().await;
}
