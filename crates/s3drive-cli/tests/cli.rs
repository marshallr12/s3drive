use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_s3drive"))
}

#[test]
fn mount_requires_explicit_config_and_mountpoint() {
    let result = cli().arg("mount").output().unwrap();
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(error.contains("<CONFIG> <MOUNTPOINT>"), "{error}");
}

#[cfg(any(target_os = "linux", windows))]
#[test]
fn cache_mount_overlap_is_rejected_before_creating_state() {
    let temporary = tempfile::tempdir().unwrap();
    let mount = temporary.path().join("mount");
    std::fs::create_dir(&mount).unwrap();
    let local = mount.join("cache");
    let path = local
        .to_str()
        .unwrap()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let config = temporary.path().join("drive.toml");
    std::fs::write(
        &config,
        format!(
            "{}\n[cache]\ndirectory=\"{path}\"\n",
            include_str!("../../../examples/s3drive.toml")
        ),
    )
    .unwrap();
    let result = cli()
        .arg("mount")
        .arg(&config)
        .arg(&mount)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("separate local trees"));
    assert!(
        !local.exists(),
        "overlap validation created cache data inside the mount"
    );
}

#[test]
fn config_and_state_commands_work_without_credentials() {
    let config =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/s3drive.toml");
    assert!(
        cli()
            .arg("config-check")
            .arg(config)
            .status()
            .unwrap()
            .success()
    );
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    for _ in 0..2 {
        assert!(
            cli()
                .arg("init-state")
                .arg("--directory")
                .arg(&state)
                .status()
                .unwrap()
                .success()
        );
    }
    assert!(state.join("metadata.sqlite3").is_file());
}

#[tokio::test]
async fn explicit_source_endpoint_region_and_profile_override_ambient_settings() {
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(10), async {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut headers = Vec::new();
            while !headers.ends_with(b"\r\n\r\n") {
                assert!(headers.len() < 16384);
                headers.push(socket.read_u8().await.unwrap());
            }
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nETag: \"fixture\"\r\nConnection: close\r\n\r\n").await.unwrap();
            String::from_utf8(headers).unwrap()
        }).await.unwrap()
    });
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("drive.toml");
    let profiles = dir.path().join("credentials");
    let aws_config = dir.path().join("aws-config");
    std::fs::write(&config, format!("version = 1\nmode = 'read-only'\n[[sources]]\nalias = 'docs'\nbucket = 'test-bucket'\nregion = 'us-east-1'\nendpoint = '{endpoint}'\nprofile = 'fixture'\npath_style = true\nallow_http_loopback = true\n")).unwrap();
    std::fs::write(
        &profiles,
        "[fixture]\naws_access_key_id = TESTPROFILE\naws_secret_access_key = TESTPROFILESECRET\n",
    )
    .unwrap();
    std::fs::write(&aws_config, "").unwrap();
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_s3drive"));
    command
        .arg("stat")
        .arg(config)
        .arg("docs/file")
        .env("AWS_SHARED_CREDENTIALS_FILE", profiles)
        .env("AWS_CONFIG_FILE", aws_config)
        .env("AWS_ACCESS_KEY_ID", "TESTENV")
        .env("AWS_SECRET_ACCESS_KEY", "TESTENVSECRET")
        .env_remove("AWS_SESSION_TOKEN")
        .env("AWS_PROFILE", "nonexistent")
        .env("AWS_ENDPOINT_URL", "http://127.0.0.1:1")
        .env("AWS_ENDPOINT_URL_S3", "http://127.0.0.1:1")
        .env("AWS_REGION", "wrong-region")
        .env("AWS_DEFAULT_REGION", "wrong-region")
        .env("AWS_EC2_METADATA_DISABLED", "true")
        .kill_on_drop(true);
    let result = tokio::time::timeout(Duration::from_secs(10), command.output()).await;
    if result.is_err() {
        server.abort();
        let _ = server.await;
        panic!("CLI network command timed out");
    }
    let result = result.unwrap().unwrap();
    if !result.status.success() {
        server.abort();
        let _ = server.await;
        panic!("CLI failed: {}", String::from_utf8_lossy(&result.stderr));
    }
    let headers = server.await.unwrap();
    assert!(headers.starts_with("HEAD /test-bucket/file "));
    assert!(headers.contains("Credential=TESTPROFILE/"));
    assert!(headers.contains("/us-east-1/s3/aws4_request"));
    assert!(!headers.contains("TESTENV"));
    assert!(String::from_utf8_lossy(&result.stdout).contains("size: 3"));
}
