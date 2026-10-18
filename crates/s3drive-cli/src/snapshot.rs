//! Offline metadata maintenance only. No credentials, network, service startup
//! or restore path. Completion/identity never establishes history continuity.
use clap::{Args, Subcommand};
use s3drive_state::authority_snapshot::{self, Inspection, Limits};
use std::{fmt, path::PathBuf, sync::atomic::Ordering, time::Duration};

#[derive(Args)]
pub struct Arguments {
    #[command(subcommand)]
    command: Operation,
}
#[derive(Subcommand)]
enum Operation {
    /// Copy an exclusively owned, stopped coordinator into a new private artifact.
    Create {
        #[arg(long)]
        directory: PathBuf,
        /// New output directory outside the coordinator and mounted trees.
        #[arg(long)]
        output: PathBuf,
        #[command(flatten)]
        identity: Identity,
        #[command(flatten)]
        bounds: Bounds,
    },
    /// Check a private artifact without migrating it or authorizing restoration.
    Inspect {
        #[arg(long)]
        directory: PathBuf,
        #[command(flatten)]
        identity: Identity,
        #[command(flatten)]
        bounds: Bounds,
    },
}
#[derive(Args)]
struct Identity {
    #[arg(long)]
    expect_volume: String,
    #[arg(long, value_parser = ["exact-utf8-v1", "portable-collision-v1"])]
    name_policy: String,
}
#[derive(Args)]
struct Bounds {
    /// Maximum standalone database bytes (4 KiB through 8 GiB).
    #[arg(long, default_value_t = 536_870_912, value_parser = clap::value_parser!(u64).range(4096..=8_589_934_592))]
    max_bytes: u64,
    /// Cooperative deadline; cannot interrupt a stalled OS filesystem call.
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(1..=300))]
    timeout_seconds: u64,
}
#[derive(Debug)]
pub enum Error {
    Snapshot(authority_snapshot::Error),
    Worker,
    Signal,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Self::Snapshot(error) = self {
            return error.fmt(f);
        }
        write!(
            f,
            "offline snapshot failed ({self:?}); preserve source and any partial output; inspect completion; restoration is not authorized"
        )
    }
}
impl std::error::Error for Error {}

pub async fn run(arguments: Arguments) -> Result<(), Error> {
    let bounds = match &arguments.command {
        Operation::Create { bounds, .. } | Operation::Inspect { bounds, .. } => bounds,
    };
    let limits = Limits {
        max_bytes: bounds.max_bytes,
        timeout: Duration::from_secs(bounds.timeout_seconds),
        ..Limits::default()
    };
    let cancel = limits.cancelled.clone();
    let mut job = tokio::task::spawn_blocking(move || match arguments.command {
        Operation::Create {
            directory,
            output,
            identity,
            ..
        } => authority_snapshot::create(
            &directory,
            &identity.expect_volume,
            &identity.name_policy,
            &output,
            &limits,
        )
        .map(|manifest| Inspection {
            manifest,
            history_continuity_proven: false,
            restore_authorized: false,
        }),
        Operation::Inspect {
            directory,
            identity,
            ..
        } => authority_snapshot::inspect(
            &directory,
            &identity.expect_volume,
            &identity.name_policy,
            &limits,
        ),
    });
    let outcome = tokio::select! {
        result = &mut job => result,
        signal = tokio::signal::ctrl_c() => {
            cancel.store(true, Ordering::Relaxed);
            // A running blocking task cannot be aborted. Await its cooperative
            // cleanup and preserve CompletionUnknown after marker publication.
            let result = job.await;
            if signal.is_err() { return Err(Error::Signal); }
            result
        }
    }
    .map_err(|_| Error::Worker)?
    .map_err(Error::Snapshot)?;
    // Only the validated manifest is printed, never input database/config data.
    println!(
        "{}",
        serde_json::to_string(&outcome).map_err(|_| Error::Worker)?
    );
    Ok(())
}
