//! Windowless launcher for the per-user drive-letter startup task.
//!
//! A Task Scheduler task that runs in the signed-in user's interactive session
//! (needed so the drive letter appears in that user's Explorer) would show a
//! console window if it started `powershell.exe` directly. This GUI-subsystem
//! program has no console of its own and starts the fixed system Windows
//! PowerShell with `CREATE_NO_WINDOW`, forwarding its arguments unchanged and
//! returning the child's exit code to Task Scheduler.
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    std::process::exit(windows::run(std::env::args_os().skip(1).collect()));
}

#[cfg(not(windows))]
fn main() {
    eprintln!("s3drive-launch is only used by Windows drive-letter startup");
    std::process::exit(2);
}

#[cfg(windows)]
mod windows {
    use std::ffi::OsString;
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};

    /// Win32 `CREATE_NO_WINDOW`: the console child gets no visible window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// Resolve the system Windows PowerShell 5.1 executable, never from PATH.
    fn powershell() -> Option<PathBuf> {
        let root = std::env::var_os("SystemRoot")?;
        Some(PathBuf::from(root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe"))
    }

    /// Run PowerShell with `arguments`, hidden, and return its exit code.
    /// Any launch failure returns 1 so Task Scheduler records a failed run.
    pub fn run(arguments: Vec<OsString>) -> i32 {
        let Some(program) = powershell() else {
            return 1;
        };
        let status = Command::new(program)
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status();
        status.ok().and_then(|s| s.code()).unwrap_or(1)
    }
}
