//! Compact, hash-bound preconditions. Full payload references avoid duplicating
//! provider proof strings in the bounded namespace input row.
use super::*;
use namespace::{Artifact, Kind, Node};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub revision: u64,
    pub parent: String,
    pub name: String,
    pub kind: Kind,
    pub reference: Option<String>,
    pub placement: Option<Store>,
}
impl Entry {
    pub fn from_node(n: &Node) -> Result<Self> {
        if n.deleted || n.payload.as_ref().is_some_and(|p| !p.valid()) {
            return Err("namespace precondition requires a live verified node".into());
        }
        let e = Self {
            id: n.id.clone(),
            revision: n.revision,
            parent: n.parent.clone().ok_or("root mutation is unsupported")?,
            name: n.name.clone(),
            kind: n.kind,
            reference: namespace::payload_reference(&n.payload),
            placement: n.placement.clone(),
        };
        e.validate()?;
        Ok(e)
    }
    fn validate(&self) -> Result<()> {
        let probe = Apply {
            volume: "1".repeat(32),
            operation: "2".repeat(32),
            expected_sequence: self.revision,
            mutation: Mutation::RemoveEntry {
                node: namespace::NodeGuard {
                    id: self.id.clone(),
                    revision: self.revision,
                },
                parent: namespace::NodeGuard {
                    id: self.parent.clone(),
                    revision: 0,
                },
            },
        };
        probe
            .validate()
            .map_err(|_| "invalid namespace precondition identity")?;
        if self.id == namespace::ROOT
            || s3drive_core::names::collision_key(&self.name).is_none()
            || self
                .reference
                .as_ref()
                .is_some_and(|s| !namespace::publication::digest(s))
            || (self.kind == Kind::File) != self.reference.is_some()
            || self.placement.as_ref().is_some_and(|s| !s.valid())
            || (self.kind == Kind::Directory && self.placement.is_some())
        {
            return Err("invalid namespace precondition".into());
        }
        Ok(())
    }
    pub(crate) fn matches(&self, n: &Node) -> bool {
        self.id == n.id
            && self.revision == n.revision
            && n.parent.as_ref() == Some(&self.parent)
            && self.name == n.name
            && self.kind == n.kind
            && self.reference == namespace::payload_reference(&n.payload)
            && self.placement == n.placement
    }
    fn guard_matches(&self, g: &namespace::NodeGuard) -> bool {
        self.id == g.id && self.revision == g.revision
    }
}
pub(super) fn validate(input: &Input) -> Result<()> {
    for entry in &input.entries {
        entry.validate()?;
    }
    match &input.request.mutation {
        Mutation::CreateEntry { .. } | Mutation::CreateDeferredEntry { .. }
            if input.entries.is_empty() =>
        {
            Ok(())
        }
        Mutation::RemoveEntry { node, parent }
        | Mutation::ArmRemoval { node, parent, .. }
        | Mutation::PrepareRemoval { node, parent, .. }
        | Mutation::TransitionRemoval { node, parent, .. }
        | Mutation::CommitRemoval { node, parent, .. }
            if input.entries.len() == 1
                && input.entries[0].guard_matches(node)
                && input.entries[0].parent == parent.id =>
        {
            Ok(())
        }
        Mutation::CancelRemoval { node, .. }
            if input.entries.len() == 1 && input.entries[0].id == *node =>
        {
            Ok(())
        }
        Mutation::MoveEntry {
            node,
            source_parent,
            parent,
            destination,
            ..
        }
        | Mutation::MoveDeferredEntry {
            node,
            source_parent,
            parent,
            destination,
            ..
        } => {
            let count = if destination.as_ref().is_some_and(|d| d.id != node.id) {
                2
            } else {
                1
            };
            if input.entries.len() != count
                || !input.entries[0].guard_matches(node)
                || input.entries[0].parent != source_parent.id
            {
                return Err("namespace source precondition mismatch".into());
            }
            if let Some(target) = destination {
                let e = if count == 2 {
                    &input.entries[1]
                } else {
                    &input.entries[0]
                };
                if !e.guard_matches(target) || e.parent != parent.id {
                    return Err("namespace target precondition mismatch".into());
                }
            }
            Ok(())
        }
        _ => Err("namespace preconditions do not cover request".into()),
    }
}
pub(super) fn effects(input: &Input, r: &Receipt) -> Result<Vec<Node>> {
    let mut expected = input.entries[0].clone();
    expected.revision = r.sequence;
    let (deleted, displaced) = match &input.request.mutation {
        Mutation::RemoveEntry { .. } | Mutation::CommitRemoval { .. } => (true, None),
        Mutation::ArmRemoval { .. }
        | Mutation::PrepareRemoval { .. }
        | Mutation::TransitionRemoval { .. }
        | Mutation::CancelRemoval { .. } => (false, None),
        Mutation::MoveEntry { parent, name, .. }
        | Mutation::MoveDeferredEntry { parent, name, .. } => {
            expected.parent = parent.id.clone();
            expected.name = name.clone();
            (false, input.entries.get(1))
        }
        _ => return Err("invalid namespace entry outcome".into()),
    };
    if r.sequence <= input.request.expected_sequence
        || !expected.matches(&r.node)
        || r.node.deleted != deleted
        || r.node.payload.as_ref().is_some_and(|p| {
            !p.valid()
                || p.reservation.volume != input.request.volume
                || p.reservation.node != r.node.id
        })
    {
        return Err("namespace source receipt does not match frozen input".into());
    }
    if let Some(fence) = super::removals::fence(input)
        && !matches!(input.request.mutation, Mutation::MoveDeferredEntry { .. })
    {
        if r.artifact != Some(Artifact::Removal(fence)) {
            return Err("removal receipt changed ownership fence".into());
        }
        return Ok(vec![r.node.clone()]);
    }
    let actual = match (&input.request.mutation, &r.artifact) {
        (
            Mutation::MoveDeferredEntry { fence, .. },
            Some(Artifact::RemovalMove {
                fence: actual,
                displaced,
            }),
        ) if fence.as_ref() == actual => displaced,
        (
            Mutation::MoveEntry { .. } | Mutation::RemoveEntry { .. },
            Some(Artifact::Namespace { displaced }),
        ) => displaced,
        _ => return Err("namespace receipt lacks exact entry/fence artifact".into()),
    };
    let mut nodes = vec![r.node.clone()];
    match (displaced, actual) {
        (None, None) => (),
        (Some(before), Some(after)) => {
            let mut expected = before.clone();
            expected.revision = r.sequence;
            if !expected.matches(after)
                || !after.deleted
                || after.payload.as_ref().is_some_and(|p| {
                    !p.valid()
                        || p.reservation.volume != input.request.volume
                        || p.reservation.node != after.id
                })
            {
                return Err("namespace displaced receipt changed data".into());
            }
            nodes.push(after.clone());
        }
        _ => return Err("namespace displaced receipt mismatch".into()),
    }
    Ok(nodes)
}

#[cfg(test)]
mod tests;
