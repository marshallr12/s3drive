//! Fixed-size, content-free observations of the owned HTTP exchange. Plaintext
//! counts sit above TLS; they do not measure TCP delivery or remote acceptance.
use super::Progress;
use std::{
    io::{self, IoSlice},
    pin::Pin,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
    task::{Context, Poll},
    time::Instant,
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// A concurrent best-effort snapshot. Each timing is elapsed milliseconds from
/// RPC creation; None means that milestone has not been observed. A driver poll
/// gap includes legitimate I/O waits and cannot by itself prove starvation.
/// Reported counts and milliseconds saturate at u32::MAX (about 49 days for
/// time). Fixed-size summaries keep the Copy error enum small.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExchangeObservation {
    pub task_started_ms: Option<u32>,
    pub http_ready_ms: Option<u32>,
    pub request_queued_ms: Option<u32>,
    pub first_driver_poll_ms: Option<u32>,
    pub last_driver_poll_ms: Option<u32>,
    pub driver_polls: u32,
    /// None while pending; Some reports only the driver's own Result. A driver
    /// may report Ok after delivering an HTTP failure to its response consumer.
    pub driver_succeeded: Option<bool>,
    pub max_driver_poll_gap_ms: u32,
    pub read_polls: u32,
    pub read_pending_polls: u32,
    pub read_bytes: u32,
    pub write_polls: u32,
    pub write_pending_polls: u32,
    pub written_bytes: u32,
    pub flush_polls: u32,
    pub shutdown_polls: u32,
    /// A hint sampled before the single enqueue. On the fresh, never-polled
    /// driver this is normally Some(false); buffering the first send is valid.
    pub sender_ready_before_enqueue: Option<bool>,
    /// Existing body-descriptor delegations were called, not proof of writes.
    pub body_end_stream_queried: bool,
    pub body_size_hint_queried: bool,
    /// At least one driver poll observed exhaustion at this boundary. These
    /// aggregate flags cannot pair two samples or identify what used budget.
    pub driver_budget_exhausted_before: bool,
    pub driver_budget_exhausted_after: bool,
}

pub(super) struct ExchangeProgress {
    started: Instant,
    task_started: OnceLock<u64>,
    http_ready: OnceLock<u64>,
    request_queued: OnceLock<u64>,
    first_driver_poll: OnceLock<u64>,
    driver_polls: AtomicU64,
    driver_result: AtomicU8,
    last_driver_poll: AtomicU64,
    max_driver_poll_gap: AtomicU64,
    read_polls: AtomicU64,
    read_pending_polls: AtomicU64,
    read_bytes: AtomicU64,
    write_polls: AtomicU64,
    write_pending_polls: AtomicU64,
    written_bytes: AtomicU64,
    flush_polls: AtomicU64,
    shutdown_polls: AtomicU64,
    sender_ready_before_enqueue: OnceLock<bool>,
    body_end_stream_queried: AtomicBool,
    body_size_hint_queried: AtomicBool,
    driver_budget_exhausted_before: AtomicBool,
    driver_budget_exhausted_after: AtomicBool,
}
impl Default for ExchangeProgress {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            task_started: OnceLock::new(),
            http_ready: OnceLock::new(),
            request_queued: OnceLock::new(),
            first_driver_poll: OnceLock::new(),
            driver_polls: AtomicU64::new(0),
            driver_result: AtomicU8::new(0),
            last_driver_poll: AtomicU64::new(0),
            max_driver_poll_gap: AtomicU64::new(0),
            read_polls: AtomicU64::new(0),
            read_pending_polls: AtomicU64::new(0),
            read_bytes: AtomicU64::new(0),
            write_polls: AtomicU64::new(0),
            write_pending_polls: AtomicU64::new(0),
            written_bytes: AtomicU64::new(0),
            flush_polls: AtomicU64::new(0),
            shutdown_polls: AtomicU64::new(0),
            sender_ready_before_enqueue: OnceLock::new(),
            body_end_stream_queried: AtomicBool::new(false),
            body_size_hint_queried: AtomicBool::new(false),
            driver_budget_exhausted_before: AtomicBool::new(false),
            driver_budget_exhausted_after: AtomicBool::new(false),
        }
    }
}
fn bounded(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
fn add(counter: &AtomicU64, n: u64) -> u64 {
    counter
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
            Some(old.saturating_add(n))
        })
        .unwrap_or_else(|old| old)
}
impl ExchangeProgress {
    fn elapsed(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
    pub fn task_started(&self) {
        let _ = self.task_started.set(self.elapsed());
    }
    pub fn http_ready(&self) {
        let _ = self.http_ready.set(self.elapsed());
    }
    pub fn request_queued(&self) {
        let _ = self.request_queued.set(self.elapsed());
    }
    pub fn sender_readiness(&self, ready: bool) {
        let _ = self.sender_ready_before_enqueue.set(ready);
    }
    pub fn body_end_stream_queried(&self) {
        self.body_end_stream_queried.store(true, Ordering::Relaxed);
    }
    pub fn body_size_hint_queried(&self) {
        self.body_size_hint_queried.store(true, Ordering::Relaxed);
    }
    pub fn driver_polled(&self) {
        if !tokio::task::coop::has_budget_remaining() {
            self.driver_budget_exhausted_before
                .store(true, Ordering::Relaxed);
        }
        let now = self.elapsed();
        let _ = self.first_driver_poll.set(now);
        let previous = self.last_driver_poll.swap(now, Ordering::Relaxed);
        if add(&self.driver_polls, 1) != 0 {
            self.max_driver_poll_gap
                .fetch_max(now.saturating_sub(previous), Ordering::Relaxed);
        }
    }
    pub fn driver_returned(&self) {
        if !tokio::task::coop::has_budget_remaining() {
            self.driver_budget_exhausted_after
                .store(true, Ordering::Relaxed);
        }
    }
    pub fn driver_finished(&self, success: bool) {
        self.driver_result
            .store(if success { 1 } else { 2 }, Ordering::Relaxed);
    }
    pub fn snapshot(&self) -> ExchangeObservation {
        ExchangeObservation {
            task_started_ms: self.task_started.get().copied().map(bounded),
            http_ready_ms: self.http_ready.get().copied().map(bounded),
            request_queued_ms: self.request_queued.get().copied().map(bounded),
            first_driver_poll_ms: self.first_driver_poll.get().copied().map(bounded),
            last_driver_poll_ms: self
                .first_driver_poll
                .get()
                .map(|_| bounded(self.last_driver_poll.load(Ordering::Relaxed))),
            driver_polls: bounded(self.driver_polls.load(Ordering::Relaxed)),
            driver_succeeded: match self.driver_result.load(Ordering::Relaxed) {
                1 => Some(true),
                2 => Some(false),
                _ => None,
            },
            max_driver_poll_gap_ms: bounded(self.max_driver_poll_gap.load(Ordering::Relaxed)),
            read_polls: bounded(self.read_polls.load(Ordering::Relaxed)),
            read_pending_polls: bounded(self.read_pending_polls.load(Ordering::Relaxed)),
            read_bytes: bounded(self.read_bytes.load(Ordering::Relaxed)),
            write_polls: bounded(self.write_polls.load(Ordering::Relaxed)),
            write_pending_polls: bounded(self.write_pending_polls.load(Ordering::Relaxed)),
            written_bytes: bounded(self.written_bytes.load(Ordering::Relaxed)),
            flush_polls: bounded(self.flush_polls.load(Ordering::Relaxed)),
            shutdown_polls: bounded(self.shutdown_polls.load(Ordering::Relaxed)),
            sender_ready_before_enqueue: self.sender_ready_before_enqueue.get().copied(),
            body_end_stream_queried: self.body_end_stream_queried.load(Ordering::Relaxed),
            body_size_hint_queried: self.body_size_hint_queried.load(Ordering::Relaxed),
            driver_budget_exhausted_before: self
                .driver_budget_exhausted_before
                .load(Ordering::Relaxed),
            driver_budget_exhausted_after: self
                .driver_budget_exhausted_after
                .load(Ordering::Relaxed),
        }
    }
}

pub(super) struct ObservedIo<T> {
    inner: T,
    progress: Arc<Progress>,
}
impl<T> ObservedIo<T> {
    pub fn new(inner: T, progress: Arc<Progress>) -> Self {
        Self { inner, progress }
    }
}
impl<T: AsyncRead + Unpin> AsyncRead for ObservedIo<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        add(&this.progress.exchange.read_polls, 1);
        let before = buf.filled().len();
        let result = Pin::new(&mut this.inner).poll_read(cx, buf);
        if result.is_pending() {
            add(&this.progress.exchange.read_pending_polls, 1);
        }
        add(
            &this.progress.exchange.read_bytes,
            (buf.filled().len() - before) as u64,
        );
        result
    }
}
impl<T: AsyncWrite + Unpin> AsyncWrite for ObservedIo<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        add(&this.progress.exchange.write_polls, 1);
        let result = Pin::new(&mut this.inner).poll_write(cx, buf);
        if result.is_pending() {
            add(&this.progress.exchange.write_pending_polls, 1);
        }
        if let Poll::Ready(Ok(n)) = &result {
            add(&this.progress.exchange.written_bytes, *n as u64);
        }
        result
    }
    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        add(&this.progress.exchange.write_polls, 1);
        let result = Pin::new(&mut this.inner).poll_write_vectored(cx, bufs);
        if result.is_pending() {
            add(&this.progress.exchange.write_pending_polls, 1);
        }
        if let Poll::Ready(Ok(n)) = &result {
            add(&this.progress.exchange.written_bytes, *n as u64);
        }
        result
    }
    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        add(&this.progress.exchange.flush_polls, 1);
        Pin::new(&mut this.inner).poll_flush(cx)
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        add(&this.progress.exchange.shutdown_polls, 1);
        Pin::new(&mut this.inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests;
