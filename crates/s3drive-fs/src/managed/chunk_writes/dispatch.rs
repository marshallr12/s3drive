//! Poll sparse operations on Tokio workers, never on a native driver callback stack.
use super::*;
use std::future::Future;
struct Task<T>(tokio::task::JoinHandle<Result<T>>);
impl<T> Drop for Task<T> {
    fn drop(&mut self) {
        // A plain JoinHandle drop detaches. Callback timeout/cancellation must
        // request cancellation and retain the inner operation's recovery state.
        self.0.abort();
    }
}
pub(super) fn run<T: Send + 'static>(
    future: impl Future<Output = Result<T>> + Send + 'static,
) -> impl Future<Output = Result<T>> + Send {
    // Construct the owned job before the join future. Callers must prove Send
    // only for its result/handle, not traverse an inactive SDK future's type.
    let mut task = Task(tokio::spawn(future));
    async move { (&mut task.0).await.map_err(|_| Error::Internal)? }
}
impl Writes {
    pub async fn create(self: &Arc<Self>, parent: &str, name: &str) -> Result<RemoteNode> {
        let permit = self.admit()?;
        let writer = self.clone();
        let (parent, name) = (parent.to_owned(), name.to_owned());
        run(async move {
            let _permit = permit;
            Box::pin(writer.create_inner(&parent, &name)).await
        })
        .await
    }
    pub async fn create_directory(
        self: &Arc<Self>,
        parent: &str,
        name: &str,
    ) -> Result<RemoteNode> {
        let permit = self.admit()?;
        let writer = self.clone();
        let (parent, name) = (parent.to_owned(), name.to_owned());
        run(async move {
            let _permit = permit;
            tokio::time::timeout(
                DEADLINE,
                Box::pin(writer.create_directory_inner(&parent, &name)),
            )
            .await
            .map_err(|_| Error::Timeout)?
        })
        .await
    }
    pub async fn open(
        self: &Arc<Self>,
        node: &str,
        writing: bool,
        legacy: s3drive_managed::Reader,
    ) -> Result<Opened> {
        let permit = self.admit()?;
        let writer = self.clone();
        let node = node.to_owned();
        run(async move {
            let _permit = permit;
            Box::pin(writer.select_inner(&node, writing, legacy)).await
        })
        .await
    }
    pub async fn read(
        self: &Arc<Self>,
        file: &Arc<File>,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>> {
        let permit = self.admit()?;
        let (writer, file) = (self.clone(), file.clone());
        run(async move {
            let _permit = permit;
            Box::pin(writer.read_inner(&file, offset, length)).await
        })
        .await
    }
    pub async fn write(
        self: &Arc<Self>,
        file: &Arc<File>,
        offset: u64,
        input: Arc<[u8]>,
        append: bool,
        constrained: bool,
    ) -> Result<usize> {
        let permit = self.admit()?;
        let (writer, file) = (self.clone(), file.clone());
        run(async move {
            let _permit = permit;
            Box::pin(writer.write_inner(&file, offset, input, append, constrained)).await
        })
        .await
    }
    pub async fn truncate(self: &Arc<Self>, file: &Arc<File>, size: u64) -> Result<()> {
        let permit = self.admit()?;
        let (writer, file) = (self.clone(), file.clone());
        run(async move {
            let _permit = permit;
            Box::pin(writer.truncate_inner(&file, size)).await
        })
        .await
    }
    pub async fn flush(self: &Arc<Self>, file: &Arc<File>) -> Result<()> {
        let permit = self.admit()?;
        let (writer, file) = (self.clone(), file.clone());
        run(async move {
            let _permit = permit;
            Box::pin(writer.flush_inner(&file)).await
        })
        .await
    }
    pub async fn sync_namespace(self: &Arc<Self>) -> Result<()> {
        let permit = self.admit()?;
        let writer = self.clone();
        run(async move {
            let _permit = permit;
            Box::pin(writer.sync_namespace_inner()).await
        })
        .await
    }
    pub async fn flush_all(self: &Arc<Self>) -> Result<()> {
        let permit = self.admit()?;
        let writer = self.clone();
        run(async move {
            let _permit = permit;
            Box::pin(writer.flush_all_inner()).await
        })
        .await
    }
    pub async fn stop(self: &Arc<Self>) -> Result<()> {
        self.closing.store(true, Ordering::Release);
        let writer = self.clone();
        run(async move { Box::pin(writer.stop_inner()).await }).await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancelled_callback_aborts_owned_task_and_releases_its_slot() {
        struct End(Option<tokio::sync::oneshot::Sender<()>>);
        impl Drop for End {
            fn drop(&mut self) {
                if let Some(send) = self.0.take() {
                    let _ = send.send(());
                }
            }
        }
        let slots = Arc::new(Semaphore::new(1));
        let permit = slots.clone().acquire_owned().await.unwrap();
        let (start, started) = tokio::sync::oneshot::channel();
        let (end, ended) = tokio::sync::oneshot::channel();
        let callback = tokio::spawn(run(async move {
            let _end = End(Some(end));
            let _permit = permit;
            let _ = start.send(());
            std::future::pending::<Result<()>>().await
        }));
        started.await.unwrap();
        callback.abort();
        assert!(callback.await.unwrap_err().is_cancelled());
        tokio::time::timeout(Duration::from_secs(5), ended)
            .await
            .unwrap()
            .unwrap();
        let _permit = tokio::time::timeout(Duration::from_secs(5), slots.acquire_owned())
            .await
            .unwrap()
            .unwrap();
    }
    #[tokio::test]
    async fn unpolled_join_drop_cancels_owned_job_and_releases_admission() {
        let slots = Arc::new(Semaphore::new(1));
        let permit = slots.clone().acquire_owned().await.unwrap();
        let join = run(async move {
            let _permit = permit;
            std::future::pending::<Result<()>>().await
        });
        drop(join);
        let _permit = tokio::time::timeout(Duration::from_secs(5), slots.acquire_owned())
            .await
            .unwrap()
            .unwrap();
    }
}
