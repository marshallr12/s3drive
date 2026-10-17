//! Operator bootstrap. Frozen registration requests are private local documents,
//! never a new source of provider authorization or automatic retries.
use clap::{Args, Subcommand, ValueEnum};
use s3drive_authority::{
    Apply, Artifact, Mutation, ROOT, Reply, Request, Store, Token, Volume, WireClient,
};
use s3drive_core::{Config, Source};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    path::{Component, Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Args)]
pub struct Connection {
    #[arg(long)]
    token_file: PathBuf,
    /// Owner-private PEM trust roots; required again for plans prepared with them.
    #[arg(long)]
    ca_file: Option<PathBuf>,
}
#[derive(Args)]
pub struct Target {
    #[arg(long)]
    authority: String,
    #[arg(long)]
    volume: String,
    #[command(flatten)]
    connection: Connection,
}
#[derive(Subcommand)]
pub enum Command {
    /// Read the pinned volume identity and current sequence.
    Info {
        #[command(flatten)]
        target: Target,
    },
    /// Prepare an immutable store registration; no metadata mutation or S3 I/O.
    StorePlan {
        config: PathBuf,
        source: String,
        #[command(flatten)]
        target: Target,
        /// Relative to the source prefix; must end in '/'.
        #[arg(long)]
        root: String,
        #[arg(long)]
        directory: PathBuf,
        /// Optional explicit unique store/operation ID (32 lowercase hex digits).
        #[arg(long)]
        store_id: Option<String>,
    },
    /// Apply only this exact registration, or observe its existing result.
    StoreApply {
        config: PathBuf,
        source: String,
        plan: PathBuf,
        #[command(flatten)]
        connection: Connection,
        #[arg(long)]
        confirm: String,
    },
    /// Inspect a retained registration without applying or rebasing anything.
    StoreStatus {
        plan: PathBuf,
        #[command(flatten)]
        connection: Connection,
    },
    /// Generate a new local managed drive configuration from registered plans.
    Configure {
        config: PathBuf,
        #[arg(long, required = true, num_args = 1, action = clap::ArgAction::Append)]
        plan: Vec<PathBuf>,
        #[command(flatten)]
        connection: Connection,
        #[arg(long)]
        directory: PathBuf,
        /// Explicit STORE_ID=LOCAL_SOURCE_ALIAS mapping for this machine.
        #[arg(long = "map")]
        mappings: Vec<String>,
        #[arg(long, value_enum, default_value = "read-only")]
        mode: Mode,
        #[arg(long)]
        default_store: Option<String>,
        /// Fresh private local state, separate from the input's direct journal.
        #[arg(long)]
        state_directory: Option<PathBuf>,
        #[arg(long, value_parser = ["posix-v1"])]
        windows_disposition: Option<String>,
    },
}
#[derive(Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum Mode {
    ReadOnly,
    ReadWrite,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    version: u32,
    authority: String,
    name_policy: String,
    source_hint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    ca_sha256: Option<String>,
    request: Apply,
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id != ROOT
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn absolute(path: &Path) -> bool {
    path.is_absolute()
        && !path.components().any(|c| matches!(c, Component::ParentDir))
        && !path.as_os_str().as_encoded_bytes().contains(&0)
}
fn destination(path: &Path) -> Result<PathBuf> {
    if !absolute(path) {
        return Err("destination requires an absolute local path".into());
    }
    Ok(path
        .parent()
        .ok_or("destination requires a parent")?
        .canonicalize()?
        .join(path.file_name().ok_or("destination requires a filename")?))
}
impl Plan {
    fn store(&self) -> Result<Store> {
        let Mutation::RegisterStore { route, root } = &self.request.mutation else {
            return Err("plan must contain only a store registration".into());
        };
        let store = Store {
            id: self.request.operation.clone(),
            route: route.clone(),
            root: root.clone(),
        };
        if self.version != 1
            || !store.valid()
            || !valid_id(&self.request.volume)
            || self.source_hint.is_empty()
            || self.source_hint.len() > 128
            || !matches!(
                self.name_policy.as_str(),
                "exact-utf8-v1" | "portable-collision-v1"
            )
            || self.ca_sha256.as_ref().is_some_and(|h| {
                h.len() != 64
                    || !h
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
            || self.request.validate().is_err()
        {
            return Err("invalid or unsupported registration plan".into());
        }
        Ok(store)
    }
    fn fingerprint(&self) -> Result<String> {
        let mut bytes = b"s3drive-store-plan-v1\0".to_vec();
        bytes.extend(serde_json::to_vec(self)?);
        Ok(digest(&bytes))
    }
    fn read(path: &Path) -> Result<Self> {
        let bytes = s3drive_state::read_private_secret(path, 65_536)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| "invalid registration plan encoding")?;
        let plan: Self =
            toml::from_str(text).map_err(|_| "invalid registration plan syntax or fields")?;
        plan.store()?;
        Ok(plan)
    }
    fn bind(&self, source: &Source) -> Result<()> {
        let store = self.store()?;
        if s3drive_s3::storage_fingerprint(source)? != store.route
            || source.prefix.len() + store.root.len() + 101 > 1024
        {
            return Err(
                "source route or maximum managed key length does not match this plan".into(),
            );
        }
        Ok(())
    }
}
fn client(endpoint: &str, connection: &Connection) -> Result<(WireClient, Option<String>)> {
    let token = Token::from_private_file(&connection.token_file)
        .map_err(|_| "invalid or inaccessible owner-private authority token")?;
    let roots = connection
        .ca_file
        .as_ref()
        .map(|p| s3drive_state::read_private_secret(p, 65_536))
        .transpose()?;
    let url = url::Url::parse(endpoint).map_err(|_| "invalid authority URL")?;
    let client = if url.scheme() == "https" {
        WireClient::https(endpoint, token, roots.as_deref())?
    } else if roots.is_none() {
        WireClient::loopback(endpoint, token)?
    } else {
        return Err("custom CA requires HTTPS".into());
    };
    Ok((client, roots.as_ref().map(|r| digest(r))))
}
fn plan_client(plan: &Plan, connection: &Connection) -> Result<WireClient> {
    let (wire, ca) = client(&plan.authority, connection)?;
    if ca != plan.ca_sha256 {
        return Err("authority trust mode or CA bytes differ from the retained plan".into());
    }
    Ok(wire)
}
async fn volume(wire: &WireClient, id: &str) -> Result<Volume> {
    if !valid_id(id) {
        return Err("invalid pinned volume ID".into());
    }
    match wire.request(&Request::Info {}).await? {
        Reply::Info(v)
            if v.id == id
                && matches!(
                    v.name_policy.as_str(),
                    "exact-utf8-v1" | "portable-collision-v1"
                ) =>
        {
            Ok(v)
        }
        _ => Err("authority did not return the pinned volume and supported naming policy".into()),
    }
}
async fn check_volume(wire: &WireClient, plan: &Plan) -> Result<()> {
    if volume(wire, &plan.request.volume).await?.name_policy != plan.name_policy {
        return Err("authority naming policy differs from the retained plan".into());
    }
    Ok(())
}
enum Status {
    Missing,
    Registered(Store),
    Rejected(s3drive_authority::Error),
}
impl Status {
    fn output(&self, plan: &Plan) -> Result<serde_json::Value> {
        let mut v = json!({"store_id": plan.request.operation, "plan_sha256": plan.fingerprint()?, "metadata_only": true});
        match self {
            Self::Missing => {
                v["status"] = json!("not-recorded");
                v["snapshot_only"] = json!(true);
            }
            Self::Registered(store) => {
                v["status"] = json!("registered");
                v["store"] = json!(store);
            }
            Self::Rejected(error) => {
                v["status"] = json!("rejected");
                v["error"] = json!(error);
            }
        }
        Ok(v)
    }
}
async fn status(wire: &WireClient, plan: &Plan) -> Result<Status> {
    let outcome = wire
        .request(&Request::Operation {
            volume: plan.request.volume.clone(),
            operation: plan.request.operation.clone(),
        })
        .await?;
    match outcome {
        Reply::Operation(None) => Ok(Status::Missing),
        Reply::Operation(Some(Err(e))) => Ok(Status::Rejected(e)),
        Reply::Operation(Some(Ok(receipt))) => {
            let expected = plan.store()?;
            if receipt.sequence != plan.request.expected_sequence + 1
                || receipt.node.id != ROOT
                || receipt.node.deleted
                || receipt.node.kind != s3drive_authority::Kind::Directory
                || receipt.node.parent.is_some()
                || !receipt.node.name.is_empty()
                || receipt.node.payload.is_some()
                || receipt.node.placement.is_some()
                || receipt.node.revision > receipt.sequence
                || receipt.artifact != Some(Artifact::Store(expected.clone()))
            {
                return Err(
                    "recorded operation does not match this exact registration plan".into(),
                );
            }
            match wire
                .request(&Request::Store {
                    volume: plan.request.volume.clone(),
                    store: expected.id.clone(),
                })
                .await?
            {
                Reply::Store(s) if s == expected => Ok(Status::Registered(s)),
                _ => Err("registered store descriptor differs from the retained plan".into()),
            }
        }
        _ => Err("could not inspect registration; retain the plan and retry store-status".into()),
    }
}
fn config(path: &Path) -> Result<Config> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(65_537)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 65_536 {
        return Err("source configuration exceeds 64 KiB".into());
    }
    Config::parse(std::str::from_utf8(&bytes).map_err(|_| "invalid configuration encoding")?)
        .map_err(Into::into)
}
fn source<'a>(config: &'a Config, alias: &str) -> Result<&'a Source> {
    config
        .sources
        .iter()
        .find(|s| s.alias == alias)
        .ok_or_else(|| "unknown local source alias".into())
}

pub async fn run(command: Command) -> Result<()> {
    let output = match command {
        Command::Info { target } => {
            let (wire, _) = client(&target.authority, &target.connection)?;
            json!({"volume": volume(&wire, &target.volume).await?, "metadata_only": true})
        }
        Command::StorePlan {
            config: path,
            source: alias,
            target,
            root,
            directory,
            store_id,
        } => {
            let config = config(&path)?;
            let source = source(&config, &alias)?;
            let mut random = [0u8; 16];
            getrandom::fill(&mut random)?;
            let id =
                store_id.unwrap_or_else(|| random.iter().map(|b| format!("{b:02x}")).collect());
            let (wire, ca_sha256) = client(&target.authority, &target.connection)?;
            // Validate local intent before any network request.
            let mut plan = Plan {
                version: 1,
                authority: target.authority,
                name_policy: "exact-utf8-v1".into(),
                source_hint: alias,
                ca_sha256,
                request: Apply {
                    volume: target.volume,
                    operation: id,
                    expected_sequence: 0,
                    mutation: Mutation::RegisterStore {
                        route: s3drive_s3::storage_fingerprint(source)?,
                        root,
                    },
                },
            };
            plan.bind(source)?;
            let info = volume(&wire, &plan.request.volume).await?;
            plan.request.expected_sequence = info.sequence;
            plan.name_policy = info.name_policy;
            if !matches!(status(&wire, &plan).await?, Status::Missing) {
                return Err(
                    "operation ID is already recorded; retain and inspect the original plan".into(),
                );
            }
            let text =
                toml::to_string_pretty(&plan).map_err(|_| "cannot serialize registration plan")?;
            let file = s3drive_state::create_private_document(
                &directory,
                "store-plan.toml",
                text.as_bytes(),
            )?;
            json!({"plan": file, "plan_sha256": plan.fingerprint()?, "store_id": plan.request.operation,
                "physical_prefix": format!("{}{}", source.prefix, plan.store()?.root), "metadata_only": true})
        }
        Command::StoreApply {
            config: path,
            source: alias,
            plan: file,
            connection,
            confirm,
        } => {
            let plan = Plan::read(&file)?;
            if confirm != plan.fingerprint()? {
                return Err("registration plan confirmation does not match".into());
            }
            let config = config(&path)?;
            plan.bind(source(&config, &alias)?)?;
            let wire = plan_client(&plan, &connection)?;
            check_volume(&wire, &plan).await?;
            let mut observed = status(&wire, &plan).await?;
            if matches!(observed, Status::Missing) {
                // A failure here is UNKNOWN, never permission to generate a new
                // operation ID or rebase the sequence. The plan survives first.
                wire.request(&Request::Apply {
                    request: plan.request.clone(),
                })
                .await?;
                observed = status(&wire, &plan).await?;
            }
            match observed {
                Status::Registered(_) => observed.output(&plan)?,
                Status::Rejected(_) => return Err("registration rejected; inspect store-status; original plan and operation remain unchanged".into()),
                Status::Missing => return Err("registration outcome unknown; retain the plan and retry store-status or this exact store-apply".into()),
            }
        }
        Command::StoreStatus {
            plan: file,
            connection,
        } => {
            let plan = Plan::read(&file)?;
            let wire = plan_client(&plan, &connection)?;
            check_volume(&wire, &plan).await?;
            status(&wire, &plan).await?.output(&plan)?
        }
        Command::Configure {
            config: path,
            plan: files,
            connection,
            directory,
            mappings,
            mode,
            default_store,
            state_directory,
            windows_disposition,
        } => {
            configure(
                &path,
                &files,
                &connection,
                &directory,
                &mappings,
                mode,
                default_store,
                state_directory,
                windows_disposition,
            )
            .await?
        }
    };
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

#[derive(Serialize)]
struct OutputConfig<'a> {
    version: u32,
    mode: &'static str,
    sources: &'a [Source],
    #[serde(skip_serializing_if = "Option::is_none")]
    writes: Option<OutputWrites<'a>>,
    managed: OutputManaged<'a>,
}
#[derive(Serialize)]
struct OutputWrites<'a> {
    directory: &'a Path,
    sources: &'a [s3drive_core::WriteSource],
}
#[derive(Serialize)]
struct OutputManaged<'a> {
    authority: &'a str,
    volume: &'a str,
    token_file: &'a Path,
    #[serde(skip_serializing_if = "Option::is_none")]
    authority_ca_file: Option<&'a Path>,
    #[serde(skip_serializing_if = "Option::is_none")]
    default_store: Option<&'a str>,
    #[serde(
        rename = "windows-disposition",
        skip_serializing_if = "Option::is_none"
    )]
    windows_disposition: Option<&'a str>,
    stores: Vec<OutputStore>,
}
#[derive(Serialize)]
struct OutputStore {
    id: String,
    source: String,
}

#[allow(clippy::too_many_arguments)]
async fn configure(
    path: &Path,
    files: &[PathBuf],
    connection: &Connection,
    directory: &Path,
    mappings: &[String],
    mode: Mode,
    default_store: Option<String>,
    state_directory: Option<PathBuf>,
    windows_disposition: Option<String>,
) -> Result<serde_json::Value> {
    if files.is_empty() || files.len() > 16 {
        return Err("configure requires 1..16 registered plans".into());
    }
    let config = config(path)?;
    let plans: Vec<_> = files.iter().map(|p| Plan::read(p)).collect::<Result<_>>()?;
    let first = &plans[0];
    let mut map = HashMap::new();
    for mapping in mappings {
        let (id, alias) = mapping
            .split_once('=')
            .ok_or("mapping must be STORE_ID=LOCAL_SOURCE_ALIAS")?;
        if !valid_id(id) || alias.is_empty() || map.insert(id, alias).is_some() {
            return Err("invalid or duplicate local store mapping".into());
        }
    }
    let mut ids = HashSet::new();
    let mut stores = Vec::new();
    for plan in &plans {
        if plan.authority != first.authority
            || plan.request.volume != first.request.volume
            || plan.name_policy != first.name_policy
            || plan.ca_sha256 != first.ca_sha256
            || !ids.insert(plan.request.operation.as_str())
        {
            return Err("plans must have unique stores in the same authority, volume, naming policy and trust mode".into());
        }
        let alias = map
            .remove(plan.request.operation.as_str())
            .unwrap_or(&plan.source_hint);
        plan.bind(source(&config, alias)?)?;
        stores.push(OutputStore {
            id: plan.request.operation.clone(),
            source: alias.into(),
        });
    }
    if !map.is_empty() {
        return Err("local mapping refers to a store absent from the supplied plans".into());
    }
    if mode == Mode::ReadOnly
        && (default_store.is_some() || state_directory.is_some() || windows_disposition.is_some())
    {
        return Err("write options require explicit --mode read-write".into());
    }
    let writes = if mode == Mode::ReadWrite {
        if first.name_policy != "portable-collision-v1" {
            return Err("managed writes require portable-collision-v1 naming".into());
        }
        let old = config.writes.as_ref().ok_or(
            "read-write setup requires existing explicit write policies and provider evidence",
        )?;
        let state = state_directory
            .as_deref()
            .filter(|p| absolute(p))
            .ok_or("read-write setup requires a fresh absolute --state-directory")?;
        // New setup must not inherit a direct journal, existing dirty data or
        // an alias of it. The final component must not exist (even as a link).
        let resolved = destination(state)?;
        match std::fs::symlink_metadata(&resolved) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
            Ok(_) => return Err("managed state directory already exists; preserve it and use its existing configuration".into()),
        }
        let output_directory = directory
            .canonicalize()
            .or_else(|_| destination(directory).map_err(std::io::Error::other))?;
        let input_directory = old
            .directory
            .canonicalize()
            .ok()
            .or_else(|| destination(&old.directory).ok());
        if input_directory.as_ref() == Some(&resolved)
            || state == old.directory
            || resolved.starts_with(&output_directory)
            || output_directory.starts_with(&resolved)
        {
            return Err("managed state must be a new directory separate from input state and generated configuration".into());
        }
        let default = default_store
            .as_ref()
            .ok_or("read-write setup requires --default-store")?;
        let selected = stores
            .iter()
            .find(|s| &s.id == default)
            .ok_or("default store must be mapped")?;
        if !old.sources.iter().any(|w| w.alias == selected.source) {
            return Err("default store must have a write policy".into());
        }
        for policy in &old.sources {
            if !stores.iter().any(|s| s.source == policy.alias) {
                return Err(
                    "every configured write source must have an explicit store mapping".into(),
                );
            }
            let source = source(&config, &policy.alias)?;
            let evidence = s3drive_s3::uploads::WritePolicy::from_toml(
                source,
                &crate::read_evidence(policy.policy.clone())?,
                &crate::read_evidence(policy.plan.clone())?,
                &crate::read_evidence(policy.report.clone())?,
            )?;
            if policy.alias == selected.source {
                let plan = plans
                    .iter()
                    .find(|p| &p.request.operation == default)
                    .ok_or("default plan missing")?;
                let key = format!(
                    "{}v1/{}/{}/{}",
                    plan.store()?.root,
                    first.request.volume,
                    "1".repeat(32),
                    "2".repeat(32)
                );
                evidence.check_target_scope(source, &key, 0).map_err(|_| "default store root is outside the current write policy or its evidence has expired")?;
            }
        }
        Some(OutputWrites {
            directory: state,
            sources: &old.sources,
        })
    } else {
        None
    };
    // Validate local input before any authority read; no provider checks.
    let output = OutputConfig {
        version: if mode == Mode::ReadOnly {
            3
        } else if windows_disposition.is_some() {
            7
        } else {
            6
        },
        mode: if mode == Mode::ReadOnly {
            "read-only"
        } else {
            "read-write"
        },
        sources: &config.sources,
        writes,
        managed: OutputManaged {
            authority: &first.authority,
            volume: &first.request.volume,
            token_file: &connection.token_file,
            authority_ca_file: connection.ca_file.as_deref(),
            default_store: default_store.as_deref(),
            windows_disposition: windows_disposition.as_deref(),
            stores,
        },
    };
    let text =
        toml::to_string_pretty(&output).map_err(|_| "cannot serialize managed configuration")?;
    Config::parse(&text)?;
    let wire = plan_client(first, connection)?;
    check_volume(&wire, first).await?;
    for plan in &plans {
        if !matches!(status(&wire, plan).await?, Status::Registered(_)) {
            return Err("all supplied plans must have verified registered outcomes".into());
        }
    }
    let file = s3drive_state::create_private_document(directory, "drive.toml", text.as_bytes())?;
    Ok(
        json!({"config": file, "version": output.version, "mode": output.mode, "metadata_only": true,
        "provider_access_verified": false, "startup_installed": false}),
    )
}
