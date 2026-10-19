//! Fresh live observation followed by exact local clean-head CAS. No remote
//! mutation, apply-later candidate, global freshness promise or native activation.
use super::*;
use sparse_views::{Adoption, ObservationStart, ScopedOperation};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservationKind {
    Recovered,
    Unchanged,
    Adopted,
}
#[derive(Clone, Debug)]
pub struct CleanObservation {
    view: View,
    kind: ObservationKind,
}
impl CleanObservation {
    pub fn view(&self) -> &View {
        &self.view
    }
    pub fn kind(&self) -> ObservationKind {
        self.kind
    }
}
fn same_scope(a: &Binding, b: &Binding) -> Result<bool> {
    let aa = a.base()?;
    let bb = b.base()?;
    Ok(aa.volume() == bb.volume()
        && aa.node() == bb.node()
        && a.store() == b.store()
        && a.source() == b.source()
        && a.fingerprint() == b.fingerprint()
        && a.root() == b.root())
}
impl Reader {
    /// Fresh authenticated observation for unknown intent, historical local
    /// recovery for known intent. Neither path grants upload or read authority.
    pub async fn observe_clean(
        &self,
        local: sparse_views::Client,
        token: ScopedOperation,
        parent: Id,
    ) -> Result<CleanObservation> {
        bounded(
            self.0.options,
            Box::pin(self.observe_inner(local, token, parent)),
        )
        .await
    }
    async fn observe_inner(
        &self,
        local: sparse_views::Client,
        token: ScopedOperation,
        parent: Id,
    ) -> Result<CleanObservation> {
        let guard = match local.prepare_observation(token, parent).await? {
            ObservationStart::Known(view) => {
                let view = *view;
                if view.binding().base()?.volume() != self.0.volume {
                    return Err(ReadFailure::Binding.into());
                }
                return Ok(CleanObservation {
                    view,
                    kind: ObservationKind::Recovered,
                });
            }
            ObservationStart::Ready(guard) => guard,
        };
        let descriptor = guard.binding().base()?;
        if descriptor.volume() != self.0.volume {
            return Err(ReadFailure::Binding.into());
        }
        if !self.0.history {
            return Err(ReadFailure::UnsupportedContent.into());
        }
        let _active = self
            .0
            .active
            .clone()
            .try_acquire_owned()
            .map_err(|_| ReadFailure::Busy)?;
        let snapshot = self
            .0
            .snapshots
            .clone()
            .try_acquire_owned()
            .map_err(|_| ReadFailure::Busy)?;
        let file = Box::pin(self.open_inner(descriptor.node(), snapshot)).await?;
        let binding = file.base.sparse_binding()?;
        if !same_scope(guard.binding(), &binding)? {
            return Err(ReadFailure::Binding.into());
        }
        Box::pin(file.base.authorize_sparse()).await?;
        let live = self.load_node(descriptor.node()).await?;
        if live.kind != Kind::File
            || live.payload != file.node.payload
            || live.placement != file.node.placement
        {
            return Err(ReadFailure::Binding.into());
        }
        let (view, kind) = match local.adopt_clean_intent(token, parent, binding).await? {
            Adoption::Unchanged(view) => (view, ObservationKind::Unchanged),
            Adoption::Adopted(view) => (view, ObservationKind::Adopted),
        };
        Ok(CleanObservation { view, kind })
    }
}
