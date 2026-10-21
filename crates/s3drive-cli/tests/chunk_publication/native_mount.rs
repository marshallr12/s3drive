//! Owned native daemon and bounded syscall helpers.
use super::*;
#[cfg(any(target_os = "linux", windows))]
pub(super) struct Mount {
    child: tokio::process::Child,
    log: tokio::task::JoinHandle<String>,
    pub(super) path: std::path::PathBuf,
}
#[cfg(target_os = "linux")]
impl Drop for Mount {
    fn drop(&mut self) {
        // Last resort only. Normal teardown below verifies successful unmount.
        if let Ok(mut process) = std::process::Command::new("fusermount3")
            .arg("-uz")
            .arg(&self.path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            let until = std::time::Instant::now() + Duration::from_secs(3);
            while process.try_wait().ok().flatten().is_none() && std::time::Instant::now() < until {
                std::thread::sleep(Duration::from_millis(20));
            }
            let _ = process.kill();
            let _ = process.wait();
        }
    }
}
#[cfg(any(target_os = "linux", windows))]
impl Mount {
    pub(super) async fn start(root: &Path) -> Self {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let path = root.join("native mount 日本語");
        #[cfg(target_os = "linux")]
        std::fs::create_dir_all(&path).unwrap();
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_s3drive"));
        command
            .arg("mount")
            .arg(root.join("native-drive.toml"))
            .arg(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.arg("--unmount-on-stdin-close");
        let mut child = command.spawn().unwrap();
        let stderr = child.stderr.take().unwrap();
        let (send, receive) = tokio::sync::oneshot::channel();
        let log = tokio::spawn(async move {
            let mut send = Some(send);
            let mut output = String::new();
            let mut lines = BufReader::new(stderr.take(65537)).lines();
            while let Some(line) = lines.next_line().await.unwrap() {
                if line.starts_with("Mounted ")
                    && let Some(send) = send.take()
                {
                    let _ = send.send(());
                }
                output.push_str(&line);
                output.push('\n');
            }
            output
        });
        let mut mount = Self { child, log, path };
        if !matches!(
            tokio::time::timeout(Duration::from_secs(30), receive).await,
            Ok(Ok(()))
        ) {
            let _ = tokio::time::timeout(Duration::from_secs(10), mount.child.kill()).await;
            let _ = tokio::time::timeout(Duration::from_secs(10), mount.child.wait()).await;
            // Borrow join handle because Linux Drop owns last-resort unmount.
            let log = tokio::time::timeout(Duration::from_secs(5), &mut mount.log)
                .await
                .unwrap()
                .unwrap();
            panic!("native chunk writer mount never became ready: {log}");
        }
        mount
    }
    pub(super) async fn stop(&mut self) {
        #[cfg(target_os = "linux")]
        {
            let status = tokio::time::timeout(
                Duration::from_secs(15),
                tokio::process::Command::new("fusermount3")
                    .arg("-u")
                    .arg(&self.path)
                    .status(),
            )
            .await
            .unwrap()
            .unwrap();
            assert!(status.success(), "native chunk writer unmount failed");
        }
        #[cfg(windows)]
        drop(self.child.stdin.take());
        let status = tokio::time::timeout(Duration::from_secs(15), self.child.wait())
            .await
            .unwrap()
            .unwrap();
        let log = tokio::time::timeout(Duration::from_secs(5), &mut self.log)
            .await
            .unwrap()
            .unwrap();
        assert!(
            status.success() && log.len() <= 65536,
            "native chunk writer daemon failed: {status}: {log}"
        );
    }
}
#[cfg(any(target_os = "linux", windows))]
pub(super) async fn blocking<T: Send + 'static>(
    mount: &mut Mount,
    f: impl FnOnce() -> T + Send + 'static,
) -> T {
    let mut task = tokio::task::spawn_blocking(f);
    let result = tokio::time::timeout(Duration::from_secs(120), &mut task).await;
    if let Ok(Ok(value)) = result {
        return value;
    }
    // A timed-out blocking filesystem call cannot be aborted by Tokio. Tear
    // down the owned daemon first, then give the syscall/thread time to return.
    drop(mount.child.stdin.take());
    let _ = tokio::time::timeout(Duration::from_secs(10), mount.child.kill()).await;
    let _ = tokio::time::timeout(Duration::from_secs(10), mount.child.wait()).await;
    #[cfg(target_os = "linux")]
    let _ = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::process::Command::new("fusermount3")
            .arg("-uz")
            .arg(&mount.path)
            .status(),
    )
    .await;
    if result.is_err() {
        let reaped = tokio::time::timeout(Duration::from_secs(5), &mut task).await;
        assert!(
            reaped.is_ok(),
            "native filesystem worker failed to return after owned daemon teardown"
        );
    }
    let log = tokio::time::timeout(Duration::from_secs(5), &mut mount.log).await;
    panic!("native filesystem worker failed or exceeded its bound; daemon reaped: {log:?}");
}
