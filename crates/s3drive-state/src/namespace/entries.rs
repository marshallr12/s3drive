//! Atomic entry CAS. No payload is uploaded, destroyed or resurrected here.
use super::*;

fn guarded(db: &Connection, guard: &NodeGuard) -> Result<Node> {
    let n = match node(db, &guard.id) {
        Err(Error::NotFound) => return Err(Error::Conflict),
        other => other?,
    };
    if n.deleted || n.revision != guard.revision {
        return Err(Error::Conflict);
    }
    Ok(n)
}
fn parent(db: &Connection, guard: &NodeGuard) -> Result<Node> {
    let n = guarded(db, guard)?;
    if n.kind != Kind::Directory {
        return Err(Error::NotDirectory);
    }
    Ok(n)
}
fn children(db: &Connection, id: &str) -> Result<i64> {
    sql(db.query_row(
        "SELECT count(*) FROM namespace_nodes WHERE parent_id=? AND deleted=0",
        [id],
        |r| r.get(0),
    ))
}
fn tombstone(db: &Connection, mut n: Node, sequence: i64) -> Result<Node> {
    let changed = sql(db.execute(
        "UPDATE namespace_nodes SET deleted=1,revision=?1 WHERE node_id=?2 AND revision=?3 AND deleted=0",
        params![sequence, n.id, n.revision as i64],
    ))?;
    n.revision = sequence as u64;
    n.deleted = true;
    if changed != 1 || node(db, &n.id)? != n {
        return Err(Error::Internal);
    }
    Ok(n)
}
pub(super) fn mutate(
    db: &Connection,
    request: &Apply,
    sequence: i64,
) -> Result<(String, Option<Node>)> {
    if volume(db)?.name_policy != names::PORTABLE {
        return Err(Error::Invalid);
    }
    match &request.mutation {
        Mutation::RemoveEntry {
            node: guard,
            parent: p,
        } => {
            if guard.id == ROOT {
                return Err(Error::Root);
            }
            let n = guarded(db, guard)?;
            let p = parent(db, p)?;
            if n.parent.as_ref() != Some(&p.id) {
                return Err(Error::Conflict);
            }
            if children(db, &n.id)? != 0 {
                return Err(Error::NotEmpty);
            }
            let n = tombstone(db, n, sequence)?;
            Ok((n.id, None))
        }
        Mutation::MoveEntry {
            node: guard,
            source_parent,
            parent: p,
            name,
            destination,
        } => {
            if guard.id == ROOT {
                return Err(Error::Root);
            }
            let mut n = guarded(db, guard)?;
            let source = parent(db, source_parent)?;
            let p = parent(db, p)?;
            if n.parent.as_ref() != Some(&source.id) {
                return Err(Error::Conflict);
            }
            let found = named(db, &p.id, name)?;
            let target = match (destination, found) {
                (None, None) => None,
                (None, Some(_)) => return Err(Error::AlreadyExists),
                (Some(g), Some(id)) if id == g.id => Some(guarded(db, g)?),
                _ => return Err(Error::Conflict),
            };
            // Validate the destination chain before changing either entry.
            if n.kind == Kind::Directory {
                let mut cursor = Some(p.id.clone());
                let mut depth = 0;
                while let Some(id) = cursor {
                    if id == n.id {
                        return Err(Error::Cycle);
                    }
                    depth += 1;
                    if depth > MAX_NODES {
                        return Err(Error::Internal);
                    }
                    cursor = directory(db, &id)?.parent;
                }
            }
            if source.id != p.id && target.is_none() && children(db, &p.id)? >= MAX_CHILDREN as i64
            {
                return Err(Error::Limit);
            }
            let displaced = match target {
                Some(t) if t.id != n.id => {
                    match (n.kind == Kind::Directory, t.kind == Kind::Directory) {
                        (true, false) => return Err(Error::NotDirectory),
                        (false, true) => return Err(Error::IsDirectory),
                        _ => (),
                    }
                    if children(db, &t.id)? != 0 {
                        return Err(Error::NotEmpty);
                    }
                    Some(tombstone(db, t, sequence)?)
                }
                _ => None,
            };
            let key = names::collision_key(name).ok_or(Error::Invalid)?;
            let changed = sql(db.execute(
                "UPDATE namespace_nodes SET parent_id=?1,name=?2,name_key=?3,revision=?4 WHERE node_id=?5 AND revision=?6 AND deleted=0",
                params![p.id, name, key, sequence, n.id, n.revision as i64],
            ))?;
            n.parent = Some(p.id);
            n.name = name.clone();
            n.revision = sequence as u64;
            if changed != 1 || node(db, &n.id)? != n {
                return Err(Error::Internal);
            }
            Ok((n.id, displaced))
        }
        _ => Err(Error::Invalid),
    }
}

#[cfg(test)]
mod tests;
