//! Counts successful plaintext I/O without inspecting or retaining its contents.
//! Delegate every I/O operation, including vectored writes, flush and shutdown.
use std::{
    io::{self, IoSlice},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    task::{Context, Poll},
    time::Instant,
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub(super) struct Progress {
    pub connection: u64,
    pub started: Instant,
    pub read: AtomicU64,
    pub written: AtomicU64,
    pub flushed: AtomicU64,
    pub dispatched: AtomicBool,
}
pub(super) struct ObservedIo<T> {
    inner: T,
    progress: Arc<Progress>,
}
impl<T> ObservedIo<T> {
    pub fn new(inner: T, connection: u64) -> (Self, Arc<Progress>) {
        let progress = Arc::new(Progress {
            connection,
            started: Instant::now(),
            read: AtomicU64::new(0),
            written: AtomicU64::new(0),
            flushed: AtomicU64::new(0),
            dispatched: AtomicBool::new(false),
        });
        (
            Self {
                inner,
                progress: progress.clone(),
            },
            progress,
        )
    }
}
impl<T: AsyncRead + Unpin> AsyncRead for ObservedIo<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let before = buf.filled().len();
        let result = Pin::new(&mut this.inner).poll_read(cx, buf);
        this.progress
            .read
            .fetch_add((buf.filled().len() - before) as u64, Ordering::Relaxed);
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
        let result = Pin::new(&mut this.inner).poll_write(cx, buf);
        if let Poll::Ready(Ok(n)) = &result {
            this.progress
                .written
                .fetch_add(*n as u64, Ordering::Relaxed);
        }
        result
    }
    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.inner).poll_write_vectored(cx, bufs);
        if let Poll::Ready(Ok(n)) = &result {
            this.progress
                .written
                .fetch_add(*n as u64, Ordering::Relaxed);
        }
        result
    }
    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.inner).poll_flush(cx);
        if let Poll::Ready(Ok(())) = &result {
            this.progress.flushed.fetch_add(1, Ordering::Relaxed);
        }
        result
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn preserves_partial_io_flush_shutdown_and_counts_only_new_bytes() {
        let (inner, mut peer) = tokio::io::duplex(2);
        let (mut io, p) = ObservedIo::new(inner, 1);
        // More bytes than the pipe can hold forces pending/partial writes.
        let mut received = [0; 5];
        let (write, read) = tokio::join!(io.write_all(b"abcde"), peer.read_exact(&mut received));
        write.unwrap();
        read.unwrap();
        assert_eq!(&received, b"abcde");
        assert_eq!(p.written.load(Ordering::Relaxed), 5);
        let n = io
            .write_vectored(&[IoSlice::new(b"f"), IoSlice::new(b"gh")])
            .await
            .unwrap();
        assert!((1..=2).contains(&n));
        let mut received = vec![0; n];
        peer.read_exact(&mut received).await.unwrap();
        assert_eq!(received, b"fgh"[..n]);
        assert_eq!(p.written.load(Ordering::Relaxed), 5 + n as u64);
        peer.write_all(b"ij").await.unwrap();
        let mut buffer = [0; 8];
        buffer[0] = b'x';
        let mut read = ReadBuf::new(&mut buffer);
        read.set_filled(1);
        std::future::poll_fn(|cx| Pin::new(&mut io).poll_read(cx, &mut read))
            .await
            .unwrap();
        assert_eq!(read.filled(), b"xij");
        assert_eq!(p.read.load(Ordering::Relaxed), 2);
        io.flush().await.unwrap();
        assert!(p.flushed.load(Ordering::Relaxed) > 0);
        io.shutdown().await.unwrap();
        assert_eq!(peer.read(&mut buffer).await.unwrap(), 0);
    }
}
