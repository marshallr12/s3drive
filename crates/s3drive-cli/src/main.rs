use clap::{Parser, Subcommand};
use s3drive_core::Config;
use s3drive_s3::S3Reader;
use std::{error::Error, fs, io::Write, path::PathBuf};

#[derive(Parser)]
#[command(
    version,
    about = "S3Drive read-only mounts, S3 tools and opt-in conformance probes"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Plan/run an explicit disposable-key conditional-write capability probe.
    ProbeWrites {
        #[command(subcommand)]
        command: ProbeCommand,
    },
    /// Validate a local configuration without contacting a provider.
    ConfigCheck { config: PathBuf },
    /// Validate explicit upload policy and recorded evidence locally; no S3 or credentials.
    UploadPolicyCheck {
        config: PathBuf,
        source: String,
        #[arg(long)]
        policy: PathBuf,
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        report: PathBuf,
    },
    /// Show which bucket/key a slash-separated virtual file path would use.
    Resolve { config: PathBuf, path: String },
    /// Create or reopen a local SQLite metadata database (no network access).
    InitState {
        #[arg(long)]
        directory: PathBuf,
    },
    /// Display implemented and planned components, without probing the host.
    Status,
    /// List one bounded page under a source; pass the returned token unchanged.
    List {
        config: PathBuf,
        source: String,
        #[arg(default_value = "")]
        directory: String,
        #[arg(long, default_value_t = 1000)]
        max_keys: u16,
        #[arg(long)]
        token: Option<String>,
    },
    /// Inspect a remote file's metadata without downloading its contents.
    Stat { config: PathBuf, path: String },
    /// Write a revision-pinned range of a remote file to standard output.
    Read {
        config: PathBuf,
        path: String,
        #[arg(long, default_value_t = 0)]
        offset: u64,
        #[arg(long, default_value_t = 65536)]
        length: usize,
    },
    /// Mount a read-only drive in the foreground (Linux FUSE or Windows WinFsp).
    Mount {
        config: PathBuf,
        mountpoint: PathBuf,
        /// Unmount when the supervising process closes standard input (Windows).
        #[cfg(windows)]
        #[arg(long)]
        unmount_on_stdin_close: bool,
    },
}

#[derive(Subcommand)]
enum ProbeCommand {
    /// Prepare six random, source-bound fixture keys; no provider connection.
    Plan {
        config: PathBuf,
        source: String,
        #[arg(long)]
        output: PathBuf,
    },
    /// Create/replace/delete only the reserved keys, after plan confirmation.
    Run {
        config: PathBuf,
        source: String,
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        confirm: String,
        #[arg(long)]
        report: PathBuf,
    },
    /// Dispose only matching owned fixtures after an interrupted probe; no PUT.
    Cleanup {
        config: PathBuf,
        source: String,
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        confirm: String,
        #[arg(long)]
        report: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    match Cli::parse().command {
        // Keep SDK mutation futures out of the existing mount startup envelope.
        Command::ProbeWrites { command } => Box::pin(run_probe(command)).await?,
        Command::UploadPolicyCheck {
            config,
            source,
            policy,
            plan,
            report,
        } => {
            let source = probe_source(config, &source)?;
            s3drive_s3::uploads::WritePolicy::from_toml(
                &source,
                &read_evidence(policy)?,
                &read_evidence(plan)?,
                &read_evidence(report)?,
            )?;
            println!(
                "Valid source-bound policy and local evidence. No credentials resolved or provider connection attempted. New claims still require fresh evidence and matching runtime credentials; mounts remain read-only."
            );
        }
        Command::ConfigCheck { config } => {
            let config = Config::parse(&fs::read_to_string(config)?)?;
            println!(
                "Valid read-only configuration: {} source(s). No provider connection attempted.",
                config.sources.len()
            );
        }
        Command::Resolve { config, path } => {
            let config = Config::parse(&fs::read_to_string(config)?)?;
            let location = config.resolve(&path)?;
            println!(
                "alias={} bucket={} key={}",
                location.alias, location.bucket, location.key
            );
        }
        Command::InitState { directory } => {
            let state = s3drive_state::State::open_managed(&directory)?;
            let info = state.info()?;
            println!(
                "Local state ready: schema={}, journal={}, SQLite={}. No file data or uploads created.",
                info.schema_version, info.journal_mode, info.sqlite_version
            );
        }
        Command::Status => {
            println!(
                "Read-only transport and Linux/Windows filesystem adapters; host={}",
                std::env::consts::OS
            );
            println!("Implemented: config validation, namespace planning, local SQLite bootstrap.");
            println!("S3: {}", s3drive_s3::IMPLEMENTATION_STATUS);
            println!("Linux: {}", s3drive_fuse::IMPLEMENTATION_STATUS);
            println!("Windows: {}", s3drive_winfsp::IMPLEMENTATION_STATUS);
            println!("See docs/src/implementation-plan.md for the acceptance gates.");
        }
        Command::List {
            config,
            source,
            directory,
            max_keys,
            token,
        } => {
            let config = Config::parse(&fs::read_to_string(config)?)?;
            let source = config
                .sources
                .iter()
                .find(|entry| entry.alias == source)
                .ok_or("unknown source alias")?;
            let reader = S3Reader::from_source(source).await?;
            let page = reader
                .list_page(&directory, token.as_deref(), max_keys)
                .await?;
            println!(
                "configuration-fingerprint={}",
                reader.configuration_fingerprint()
            );
            for directory in page.directories {
                println!("directory\t{directory:?}");
            }
            for object in page.objects {
                println!("object\t{}\t{:?}", object.size, object.key);
            }
            if let Some(token) = page.next_token {
                println!("next-token={token:?}");
            }
        }
        Command::Stat { config, path } => {
            let (reader, key) = reader_for_path(config, &path).await?;
            println!("{:#?}", reader.head(&key).await?);
        }
        Command::Read {
            config,
            path,
            offset,
            length,
        } => {
            let (reader, key) = reader_for_path(config, &path).await?;
            let handle = reader.open(&key).await?;
            let bytes = handle.read_range(offset, length).await?;
            std::io::stdout().lock().write_all(&bytes)?;
        }
        Command::Mount {
            config,
            mountpoint,
            #[cfg(windows)]
            unmount_on_stdin_close,
        } => {
            #[cfg(target_os = "linux")]
            {
                let config = Config::parse(&fs::read_to_string(config)?)?;
                check_cache_location(&config, &mountpoint)?;
                let filesystem = s3drive_fs::ReadOnlyFs::new(&config).await?;
                s3drive_fuse::run(filesystem, &mountpoint).await?;
            }
            #[cfg(target_os = "windows")]
            {
                let config = Config::parse(&fs::read_to_string(config)?)?;
                check_cache_location(&config, &mountpoint)?;
                let filesystem = s3drive_fs::ReadOnlyFs::new(&config).await?;
                s3drive_winfsp::run(filesystem, &mountpoint, unmount_on_stdin_close).await?;
            }
            #[cfg(not(any(target_os = "linux", target_os = "windows")))]
            {
                let _ = (config, mountpoint);
                return Err("only Linux and Windows adapters are planned".into());
            }
        }
    }
    Ok(())
}

async fn run_probe(command: ProbeCommand) -> Result<(), Box<dyn Error + Send + Sync>> {
    use s3drive_s3::probe::Plan;
    let cleanup = matches!(&command, ProbeCommand::Cleanup { .. });
    match command {
        ProbeCommand::Plan {
            config,
            source,
            output,
        } => {
            let source = probe_source(config, &source)?;
            let plan = Plan::prepare(&source)?;
            let mut file = create_private_new(&output)?;
            file.write_all(plan.to_toml()?.as_bytes())?;
            file.sync_all()?;
            println!(
                "Prepared {} disposable keys under {}/{}",
                plan.keys().len(),
                plan.bucket(),
                plan.full_prefix()
            );
            println!("plan-digest={}", plan.digest()?);
            println!(
                "No credentials resolved or provider requests made. Review the plan before run."
            );
        }
        ProbeCommand::Run {
            config,
            source,
            plan,
            confirm,
            report,
        }
        | ProbeCommand::Cleanup {
            config,
            source,
            plan,
            confirm,
            report,
        } => {
            let source = probe_source(config, &source)?;
            let plan = {
                use std::io::Read as _;
                let mut text = String::new();
                fs::File::open(plan)?
                    .take(32769)
                    .read_to_string(&mut text)?;
                Plan::from_toml(&text)?
            };
            plan.validate(&source, &confirm)?;
            // Fail on an unusable/existing report path before any remote write.
            let mut file = create_private_new(&report)?;
            file.write_all(format!("# Probe started; plan-digest={confirm}\n# An interrupted report is not successful capability evidence.\n").as_bytes())?;
            file.sync_all()?;
            let result = if cleanup {
                plan.cleanup(&source, &confirm).await?
            } else {
                plan.execute(&source, &confirm).await?
            };
            file.write_all(result.to_toml()?.as_bytes())?;
            file.sync_all()?;
            for check in &result.checks {
                println!(
                    "{} {}: {}",
                    if check.passed { "PASS" } else { "FAIL" },
                    check.name,
                    check.detail
                );
            }
            for error in &result.errors {
                eprintln!("Probe error: {error}");
            }
            if !result.remaining_keys.is_empty() {
                eprintln!(
                    "Fixtures retained for recovery: {:?}",
                    result.remaining_keys
                );
            }
            if !result.passed {
                return Err("probe did not pass; inspect its report and cleanup state".into());
            }
            println!(
                "Probe completed; exact fixture keys removed. This does not enable writable mounts."
            );
        }
    }
    Ok(())
}
fn probe_source(
    config: PathBuf,
    alias: &str,
) -> Result<s3drive_core::Source, Box<dyn Error + Send + Sync>> {
    Config::parse(&fs::read_to_string(config)?)?
        .sources
        .into_iter()
        .find(|source| source.alias == alias)
        .ok_or_else(|| "unknown source alias".into())
}
fn create_private_new(path: &std::path::Path) -> std::io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}
fn read_evidence(path: PathBuf) -> std::io::Result<String> {
    use std::io::Read as _;
    let mut text = String::new();
    fs::File::open(path)?
        .take(32769)
        .read_to_string(&mut text)?;
    Ok(text)
}

async fn reader_for_path(
    config: PathBuf,
    path: &str,
) -> Result<(S3Reader, String), Box<dyn Error + Send + Sync>> {
    let config = Config::parse(&fs::read_to_string(config)?)?;
    let location = config.resolve(path)?;
    let source = config
        .sources
        .iter()
        .find(|source| source.alias == location.alias)
        .ok_or("unknown source alias")?;
    let key = location
        .key
        .strip_prefix(&source.prefix)
        .ok_or("key outside source prefix")?
        .to_owned();
    Ok((S3Reader::from_source(source).await?, key))
}

/// Check before initializing local state or mounting. Resolve existing ancestor
/// aliases as well as lexical paths, without creating any directories.
fn check_cache_location(
    config: &Config,
    mountpoint: &std::path::Path,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let Some(cache) = &config.cache else {
        return Ok(());
    };
    fn projected(path: &std::path::Path) -> std::io::Result<PathBuf> {
        let absolute = std::path::absolute(path)?;
        let mut current = absolute.as_path();
        let mut tail = Vec::new();
        let mut result = loop {
            match fs::canonicalize(current) {
                Ok(path) => break path,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    let Some(name) = current.file_name() else {
                        return Ok(absolute);
                    };
                    tail.push(name.to_owned());
                    current = current.parent().ok_or(error)?;
                }
                Err(error) => return Err(error),
            }
        };
        for name in tail.into_iter().rev() {
            result.push(name);
        }
        Ok(result)
    }
    fn parts(path: &std::path::Path) -> Vec<String> {
        #[cfg(windows)]
        let path = PathBuf::from(
            path.to_string_lossy()
                .trim_start_matches(r"\\?\")
                .to_uppercase(),
        );
        path.components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect()
    }
    let local = parts(&projected(&cache.directory)?);
    let mount = parts(&projected(mountpoint)?);
    if local.starts_with(&mount) || mount.starts_with(&local) {
        return Err("cache directory and mountpoint must be separate local trees".into());
    }
    Ok(())
}
