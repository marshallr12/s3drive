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

    struct ShutdownFails<T>(T);
    impl<T: AsyncRead + Unpin> AsyncRead for ShutdownFails<T> {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Pin::new(&mut self.get_mut().0).poll_read(cx, buf)
        }
    }
    impl<T: AsyncWrite + Unpin> AsyncWrite for ShutdownFails<T> {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            Pin::new(&mut self.get_mut().0).poll_write(cx, buf)
        }
        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut self.get_mut().0).poll_flush(cx)
        }
        fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "private shutdown failure",
            )))
        }
    }

    #[tokio::test]
    async fn hyper_identifies_shutdown_failure_after_a_complete_independently_read_reply() {
        use hyper_util::rt::TokioIo;
        let (inner, mut peer) = tokio::io::duplex(4096);
        let (io, progress) = ObservedIo::new(ShutdownFails(inner), 1);
        let observed = progress.clone();
        let service = hyper::service::service_fn(move |_| {
            observed.dispatched.store(true, Ordering::Relaxed);
            async {
                Ok::<_, std::convert::Infallible>(hyper::Response::new(axum::body::Body::from(
                    "complete reply",
                )))
            }
        });
        let mut http = hyper::server::conn::http1::Builder::new();
        http.timer(hyper_util::rt::TokioTimer::new())
            .keep_alive(false);
        let server = async { http.serve_connection(TokioIo::new(io), service).await };
        let caller = async {
            peer.write_all(b"GET / HTTP/1.1\r\nHost: fixture\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();
            let mut response = Vec::new();
            peer.read_to_end(&mut response).await.unwrap();
            let response = std::str::from_utf8(&response).unwrap();
            assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
            assert!(response.contains("content-length: 14\r\n"));
            assert!(response.ends_with("\r\n\r\ncomplete reply"));
        };
        let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(3), async {
            tokio::join!(server, caller)
        })
        .await
        .unwrap();
        let error = result.expect_err("injected shutdown must fail");
        assert!(
            error.is_shutdown(),
            "Hyper must preserve the precise shutdown phase"
        );
        assert!(!error.is_timeout() && !error.is_incomplete_message());
        assert!(progress.read.load(Ordering::Relaxed) > 0);
        assert!(progress.written.load(Ordering::Relaxed) > 0);
        assert!(progress.flushed.load(Ordering::Relaxed) > 0);
        assert!(progress.dispatched.load(Ordering::Relaxed));
    }

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
