//! Exact initial node shape, shared by authority proof and client receipt checks.
use super::*;
pub(crate) fn expected(r: &Apply, placement: Option<Store>, sequence: u64) -> Result<Node> {
    let (parent, name, kind, store) = match &r.mutation {
        Mutation::CreateEntry {
            parent,
            name,
            node_kind,
            store,
            ..
        }
        | Mutation::CreateDeferredEntry {
            parent,
            name,
            node_kind,
            store,
            ..
        } => (parent, name, node_kind, store),
        _ => return Err(Error::Internal),
    };
    if r.validate().is_err()
        || sequence <= r.expected_sequence
        || store.as_ref() != placement.as_ref().map(|p| &p.id)
        || placement.as_ref().is_some_and(|s| !s.valid())
    {
        return Err(Error::Internal);
    }
    Ok(Node {
        id: r.operation.clone(),
        parent: Some(parent.clone()),
        name: name.clone(),
        kind: *kind,
        revision: sequence,
        deleted: false,
        payload: None,
        placement,
    })
}
pub(crate) fn artifact(r: &Apply) -> Option<Artifact> {
    if let Mutation::CreateDeferredEntry { owner, .. } = &r.mutation {
        Some(Artifact::Removal(RemovalFence {
            intent: r.operation.clone(),
            owner: owner.clone(),
        }))
    } else {
        None
    }
}
