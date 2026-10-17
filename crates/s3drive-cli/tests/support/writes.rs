//! Native write acceptance uses the SAME real CLI probe and SDK as production,
//! only against its owned loopback fixture and isolated dummy profile files.
use super::fixture::Fixture;
use std::{
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};
use tokio::process::Command;
pub const NAME: &str = "日本語 %2F + café.txt";
pub const KEY: &str = "/test-bucket/desktop/writable/日本語 %2F + café.txt";
pub struct Setup {
    pub config: PathBuf,
    pub credentials: PathBuf,
    pub aws_config: PathBuf,
    pub state: PathBuf,
}
impl Setup {
    pub fn command(&self, mount: &Path) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_s3drive"));
        command
            .arg("mount")
            .arg(&self.config)
            .arg(mount)
            .env("AWS_SHARED_CREDENTIALS_FILE", &self.credentials)
            .env("AWS_CONFIG_FILE", &self.aws_config)
            .env("AWS_EC2_METADATA_DISABLED", "true");
        command
    }
}
pub async fn setup(fixture: &Fixture, root: &Path) -> Setup {
    fixture.writable.store(true, Ordering::SeqCst);
    fixture.put(KEY, b"abcdefghij", "initial");
    fixture.put(
        "/test-bucket/desktop/readonly.txt",
        b"read only",
        "readonly",
    );
    let config = root.join("drive.toml");
    let credentials = root.join("credentials");
    let aws_config = root.join("aws-config");
    std::fs::write(
        &credentials,
        "[fixture]\naws_access_key_id=TESTPROFILE\naws_secret_access_key=TESTPROFILESECRET\n",
    )
    .unwrap();
    std::fs::write(&aws_config, "").unwrap();
    let source = format!(
        "[[sources]]\nalias='docs'\nbucket='test-bucket'\nregion='us-east-1'\nprefix='desktop/'\nendpoint='{}'\nprofile='fixture'\npath_style=true\nallow_http_loopback=true\n",
        fixture.endpoint
    );
    std::fs::write(&config, format!("version=1\nmode='read-only'\n{source}")).unwrap();
    let plan = root.join("plan.toml");
    let report = root.join("report.toml");
    let result = Command::new(env!("CARGO_BIN_EXE_s3drive"))
        .args(["probe-writes", "plan"])
        .arg(&config)
        .arg("docs")
        .arg("--output")
        .arg(&plan)
        .output()
        .await
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let digest = s3drive_s3::probe::Plan::from_toml(&std::fs::read_to_string(&plan).unwrap())
        .unwrap()
        .digest()
        .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_s3drive"))
        .args(["probe-writes", "run"])
        .arg(&config)
        .arg("docs")
        .arg("--plan")
        .arg(&plan)
        .arg("--confirm")
        .arg(digest)
        .arg("--report")
        .arg(&report)
        .env("AWS_SHARED_CREDENTIALS_FILE", &credentials)
        .env("AWS_CONFIG_FILE", &aws_config)
        .env("AWS_EC2_METADATA_DISABLED", "true")
        .output()
        .await
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let text = std::fs::read_to_string(&report).unwrap();
    let value: toml::Value = toml::from_str(&text).unwrap();
    assert_eq!(value["passed"].as_bool(), Some(true));
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let policy = root.join("policy.toml");
    std::fs::write(&policy,format!("version=1\nsource_fingerprint='{}'\nreport_sha256='{hash}'\nwritable_prefix='writable/'\nmax_object_bytes=8388608\noperations='create-and-replace'\nobject_metadata='s3drive-markers-only'\nevidence_max_age_seconds=86400\n",value["source_fingerprint"].as_str().unwrap())).unwrap();
    let state = root.join("state");
    std::fs::write(&config,format!("version=2\nmode='read-write'\n{source}\n[writes]\ndirectory={:?}\n[[writes.sources]]\nalias='docs'\npolicy={:?}\nplan={:?}\nreport={:?}\n",state.to_str().unwrap(),policy.to_str().unwrap(),plan.to_str().unwrap(),report.to_str().unwrap())).unwrap();
    Setup {
        config,
        credentials,
        aws_config,
        state,
    }
}
pub fn exercise(mount: &Path, fixture: &Fixture) {
    let path = mount.join("docs/writable").join(NAME);
    let mut first = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    let mut reader = std::fs::File::open(&path).unwrap();
    first.set_len(0).unwrap();
    first.write_all(b"abcdefghij").unwrap();
    first.seek(SeekFrom::Start(2)).unwrap();
    first.write_all(b"XY\0\xff").unwrap();
    first.sync_all().unwrap();
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"abXY\0\xffghij");
    assert_eq!(fixture.objects.lock().unwrap()[KEY].0, bytes);
    first.set_len(5).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"abXY\0");
    first.set_len(12).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"abXY\0\0\0\0\0\0\0\0");
    let mut append = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    append.write_all(b"tail").unwrap();
    append.sync_all().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"abXY\0\0\0\0\0\0\0\0tail");
    assert!(first.set_len(8 * 1024 * 1024 + 1).is_err());
    assert!(
        std::fs::OpenOptions::new()
            .write(true)
            .open(mount.join("docs/readonly.txt"))
            .is_err()
    );
    assert!(
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(mount.join("docs/writable/new-file"))
            .is_err()
    );
    let puts = fixture
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|l| l.starts_with("PUT "))
        .count();
    first.sync_all().unwrap();
    first.sync_all().unwrap();
    assert_eq!(
        puts,
        fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|l| l.starts_with("PUT "))
            .count()
    );
    eprintln!(
        "PASS: native write, shared reads, truncate, sparse growth, append, remote flush, bounds and read-only prefix"
    );
}
