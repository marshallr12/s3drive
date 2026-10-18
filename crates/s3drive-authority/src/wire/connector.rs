//! Observe only connector call/success, never its destination, I/O or errors.
//! Capture the request scope at call time, so a later background poll cannot
//! attribute completion to another concurrent request. Unscoped calls count
//! toward neither request. No retries or replacement connections are added.
use super::Progress;
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, atomic::Ordering},
    task::{Context, Poll},
};
use tower_layer::Layer;
use tower_service::Service;

tokio::task_local! {
    pub(super) static CURRENT: Arc<Progress>;
}

#[derive(Clone, Copy)]
pub(super) struct Observe;
impl<S> Layer<S> for Observe {
    type Service = Observed<S>;
    fn layer(&self, inner: S) -> Self::Service {
        Observed(inner)
    }
}
#[derive(Clone)]
pub(super) struct Observed<S>(S);
impl<S, R> Service<R> for Observed<S>
where
    S: Service<R>,
    S::Future: Send + 'static,
    S::Response: 'static,
    S::Error: 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.0.poll_ready(cx)
    }
    fn call(&mut self, request: R) -> Self::Future {
        let progress = CURRENT.try_with(Arc::clone).ok();
        if let Some(progress) = &progress {
            progress.connector.fetch_max(1, Ordering::Release);
        }
        let future = self.0.call(request);
        Box::pin(async move {
            let result = future.await;
            if result.is_ok()
                && let Some(progress) = &progress
            {
                progress.connector.fetch_max(2, Ordering::Release);
            }
            result
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::{Ready, ready};
    #[derive(Clone)]
    struct Peer;
    impl Service<bool> for Peer {
        type Response = ();
        type Error = ();
        type Future = Ready<Result<(), ()>>;
        fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), ()>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, success: bool) -> Self::Future {
            ready(if success { Ok(()) } else { Err(()) })
        }
    }

    #[tokio::test]
    async fn completion_uses_call_scope_even_when_polled_in_another_scope() {
        let a = Arc::new(Progress::default());
        let b = Arc::new(Progress::default());
        let mut service = Observe.layer(Peer);
        let future = CURRENT.sync_scope(a.clone(), || service.call(true));
        assert_eq!(a.connector_observation(), (true, false));
        assert_eq!(a.connector.load(Ordering::Acquire), 1);
        CURRENT.scope(b.clone(), future).await.unwrap();
        assert_eq!(a.connector_observation(), (true, true));
        assert_eq!(b.connector_observation(), (false, false));
        assert_ne!(b.connector.load(Ordering::Acquire), 2);

        let failed = CURRENT.sync_scope(b.clone(), || service.call(false));
        assert!(failed.await.is_err());
        assert_eq!(b.connector_observation(), (true, false));
        assert_ne!(b.connector.load(Ordering::Acquire), 2);
        let unscoped = service.call(true);
        CURRENT.scope(b.clone(), unscoped).await.unwrap();
        assert_ne!(b.connector.load(Ordering::Acquire), 2);
    }
}
