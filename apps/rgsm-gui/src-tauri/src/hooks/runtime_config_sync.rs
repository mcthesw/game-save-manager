use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait ConfigRuntimeSync: Send + Sync {
    async fn sync_from_config(&self) -> Result<()>;
}
