use super::*;
use std::task::Waker;

struct PartialIo {
    pending: bool,
}
impl AsyncRead for PartialIo {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.pending {
            Poll::Pending
        } else {
            buf.put_slice(b"ab");
            Poll::Ready(Ok(()))
        }
    }
}
impl AsyncWrite for PartialIo {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.pending {
            Poll::Pending
        } else {
            Poll::Ready(Ok(buf.len().min(2)))
        }
    }
    fn poll_write_vectored(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        assert_eq!(bufs.len(), 2, "wrapper changed vectored write delegation");
        assert_eq!(&*bufs[1], b"cde");
        Poll::Ready(Ok(3))
    }
    fn is_write_vectored(&self) -> bool {
        true
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Pending
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Err(io::Error::from(io::ErrorKind::NotConnected)))
    }
}

#[test]
fn transparent_partial_pending_vectored_io_and_errors_count_no_private_bytes() {
    let progress = Arc::new(Progress::default());
    assert_eq!(progress.exchange.snapshot(), ExchangeObservation::default());
    let mut io = ObservedIo::new(PartialIo { pending: true }, progress.clone());
    let mut cx = Context::from_waker(Waker::noop());
    let mut buffer = [0; 8];
    buffer[0] = b'x';
    let mut read = ReadBuf::new(&mut buffer);
    read.set_filled(1);
    assert!(Pin::new(&mut io).poll_read(&mut cx, &mut read).is_pending());
    assert!(
        Pin::new(&mut io)
            .poll_write(&mut cx, b"secret payload")
            .is_pending()
    );
    io.inner.pending = false;
    assert!(matches!(
        Pin::new(&mut io).poll_read(&mut cx, &mut read),
        Poll::Ready(Ok(()))
    ));
    assert_eq!(read.filled(), b"xab");
    assert!(matches!(
        Pin::new(&mut io).poll_write(&mut cx, b"secret payload"),
        Poll::Ready(Ok(2))
    ));
    assert!(io.is_write_vectored());
    assert!(matches!(
        Pin::new(&mut io)
            .poll_write_vectored(&mut cx, &[IoSlice::new(b"ab"), IoSlice::new(b"cde")]),
        Poll::Ready(Ok(3))
    ));
    assert!(Pin::new(&mut io).poll_flush(&mut cx).is_pending());
    assert!(
        matches!(Pin::new(&mut io).poll_shutdown(&mut cx), Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::NotConnected)
    );
    let observed = progress.exchange.snapshot();
    assert_eq!(observed.read_polls, 2);
    assert_eq!(observed.read_pending_polls, 1);
    assert_eq!(
        observed.read_bytes, 2,
        "pre-filled buffer is not received data"
    );
    assert_eq!(observed.write_polls, 3);
    assert_eq!(observed.write_pending_polls, 1);
    assert_eq!(
        observed.written_bytes, 5,
        "pending writes are not sent bytes"
    );
    assert_eq!(observed.flush_polls, 1);
    assert_eq!(observed.shutdown_polls, 1);
    assert!(!format!("{observed:?}").contains("secret payload"));
    progress
        .exchange
        .driver_polls
        .store(u64::MAX, Ordering::Relaxed);
    progress.exchange.driver_polled();
    assert_eq!(progress.exchange.snapshot().driver_polls, u32::MAX);
}
