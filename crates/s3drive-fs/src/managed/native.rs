//! Native file-object identity independent of the whole/sparse content backend.
use super::*;
#[derive(Clone)]
pub struct NativeFileObject(pub(crate) BackendObject);
#[derive(Clone)]
pub(crate) enum BackendObject {
    Whole(writes::lifecycle::NativeFileObject),
    Sparse(Arc<chunk_writes::native::Object>),
}
impl NativeFileObject {
    pub async fn rename(
        &self,
        parent: &str,
        name: &str,
        replace: bool,
        same_name: fn(&str, &str) -> bool,
    ) -> Result<s3drive_authority::Node> {
        match &self.0 {
            BackendObject::Whole(o) => o.rename(parent, name, replace, same_name).await,
            BackendObject::Sparse(o) => o.rename(parent, name, replace, same_name).await,
        }
    }
    pub async fn set_delete(&self, delete: bool) -> Result<()> {
        match &self.0 {
            BackendObject::Whole(o) => o.set_delete(delete).await,
            BackendObject::Sparse(o) => o.set_delete(delete).await,
        }
    }
    pub async fn cleanup(&self, flags: u32) -> Result<()> {
        match &self.0 {
            BackendObject::Whole(o) => o.cleanup(flags).await,
            BackendObject::Sparse(o) => o.cleanup(flags).await,
        }
    }
    pub fn close(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        match &self.0 {
            BackendObject::Whole(o) => Box::pin(o.close()),
            BackendObject::Sparse(o) => Box::pin(o.close()),
        }
    }
}
