//! Owner-bound monotonic identity admission for fresh mounted retry stores.
//! Issuing an ID is observational. Only its atomic insertion advances the
//! frontier; concurrent callers may conflict and must obtain a fresh ID.
use super::*;
#[derive(Clone, Copy, Debug)]
pub enum Domain {
    Open,
    Session,
    Namespace,
    Creation,
}
impl Domain {
    fn label(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Session => "session",
            Self::Namespace => "namespace",
            Self::Creation => "creation",
        }
    }
}
fn identity(binding: &Binding, domain: Domain, serial: u64) -> Result<String> {
    if serial == 0 {
        return Err("zero native admission ordinal".into());
    }
    let digest = hash(
        b"s3drive-native-admission-v1\0",
        &(&binding.owner, domain.label(), serial),
    )?;
    let suffix: String = digest[..8].iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!("{serial:016x}{suffix}"))
}
pub(crate) fn next(db: &Connection, domain: Domain) -> Result<Option<String>> {
    let Some(binding) = load(db)? else {
        return Ok(None);
    };
    let Some(frontier) = binding.frontier else {
        return Ok(None);
    };
    Ok(Some(identity(
        &binding,
        domain,
        frontier
            .checked_add(1)
            .ok_or("native admission exhausted")?,
    )?))
}
/// Only call for an absent record, inside its inserting transaction. An exact
/// retained retry is checked before this function. A forgotten ID is expired,
/// even when its old payload and session are otherwise still well formed.
pub(crate) fn consume(db: &Connection, domain: Domain, id: &str) -> Result<()> {
    let Some(mut binding) = load(db)? else {
        return Ok(());
    };
    let Some(frontier) = binding.frontier else {
        return Ok(());
    };
    let next = frontier
        .checked_add(1)
        .ok_or("native admission exhausted")?;
    if identity(&binding, domain, next)? != id {
        return Err("expired or unissued native admission identity".into());
    }
    binding.frontier = Some(next);
    db.execute(
        "UPDATE chunk_mount_role SET record=?1,state_hash=?2 WHERE singleton=1",
        params![
            serde_json::to_vec(&binding)?,
            hash(b"s3drive-chunk-mount-binding-v1\0", &binding)?.as_slice()
        ],
    )?;
    Ok(())
}
impl Client {
    /// Persist the returned exact ID before admission. A conflict means another
    /// admission won the frontier; it does not authorize replay with a new ID.
    pub async fn next_admission(&self, domain: Domain) -> Result<Option<String>> {
        let key = self.key.clone();
        self.owner
            .call(move |worker| {
                require(&worker.state.connection, &key)?;
                next(&worker.state.connection, domain)
            })
            .await
    }
}

pub(crate) fn frontier(db: &Connection) -> Result<u64> {
    frontier_of(&load(db)?.ok_or("native binding missing")?)
}
pub(crate) fn frontier_of(binding: &Binding) -> Result<u64> {
    binding
        .frontier
        .ok_or_else(|| "native frontier missing".into())
}
pub(crate) fn retained(binding: &Binding, domain: Domain, id: &str) -> Result<()> {
    let serial = id.get(..16).ok_or("invalid native identity")?;
    let serial = u64::from_str_radix(serial, 16)?;
    if serial > binding.frontier.ok_or("native frontier missing")?
        || identity(binding, domain, serial)? != id
    {
        return Err("native identity is outside admission frontier".into());
    }
    Ok(())
}

fn target_key(target: &[u8; 32]) -> String {
    target.iter().map(|b| format!("{b:02x}")).collect()
}
pub(crate) fn witness_of(binding: &Binding, target: &[u8; 32]) -> Option<[u8; 32]> {
    binding.terminals.get(&target_key(target)).copied()
}
pub(crate) fn seal_terminal(db: &Connection, target: &[u8; 32], proof: [u8; 32]) -> Result<()> {
    let mut binding = load(db)?.ok_or("native binding missing")?;
    if binding.format != 2 {
        return Err("native terminal requires current format".into());
    }
    binding.terminals.insert(target_key(target), proof);
    binding.validate()?;
    db.execute(
        "UPDATE chunk_mount_role SET record=?1,state_hash=?2 WHERE singleton=1",
        params![
            serde_json::to_vec(&binding)?,
            hash(b"s3drive-chunk-mount-binding-v1\0", &binding)?.as_slice()
        ],
    )?;
    Ok(())
}

pub(crate) fn witnesses(db: &Connection) -> Result<std::collections::BTreeMap<String, [u8; 32]>> {
    Ok(load(db)?.map(|b| b.terminals).unwrap_or_default())
}
pub(crate) fn validate(db: &Connection) -> Result<()> {
    let Some(binding) = load(db)? else {
        return Ok(());
    };
    if binding.format != 2 {
        return Ok(());
    }
    // Reuse only this call's fully checked immutable binding. Every retained ID
    // is still checked; no evidence survives another command or transaction.
    for (table, column, domain) in [
        ("windows_open_journal", "open_id", Domain::Open),
        ("windows_mount_sessions", "session_id", Domain::Session),
    ] {
        let ids = db
            .prepare(&format!("SELECT {column} FROM {table} LIMIT 257"))?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if ids.len() > 256 {
            return Err("native retained admission capacity exceeded".into());
        }
        for id in ids {
            retained(&binding, domain, &id)?;
        }
    }
    if binding.quiescent_retention {
        let ids = db
            .prepare("SELECT operation FROM manifest_creations LIMIT 257")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if ids.len() > 256 {
            return Err("native creator admission capacity exceeded".into());
        }
        for id in ids {
            retained(&binding, Domain::Creation, &id)?;
        }
        let ids = db
            .prepare("SELECT operation_id FROM namespace_intents LIMIT 4097")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if ids.len() > 4096 {
            return Err("native namespace admission capacity exceeded".into());
        }
        for id in ids {
            if retained(&binding, Domain::Namespace, &id).is_ok()
                || retained(&binding, Domain::Creation, &id).is_ok()
            {
                continue;
            }
            if !names::opens::retained_namespace_child(db, &id, None)? {
                return Err("native namespace identity lost original admission".into());
            }
        }
    }
    Ok(())
}

/// Direct namespace requests consume the next permanent ordinal. Callback
/// children instead derive from an original retained open and callback, so
/// losing their open on a completed cut permanently expires every child ID.
pub(crate) fn namespace(db: &Connection, input: &names::Input) -> Result<()> {
    if !super::quiescent_enabled(db)? {
        return Err("namespace retention contract is absent".into());
    }
    if next(db, Domain::Namespace)?.as_deref() == Some(&input.request.operation) {
        return consume(db, Domain::Namespace, &input.request.operation);
    }
    if names::opens::retained_namespace_child(db, &input.request.operation, Some(input))? {
        Ok(())
    } else {
        Err("namespace operation is expired or lacks original callback".into())
    }
}
