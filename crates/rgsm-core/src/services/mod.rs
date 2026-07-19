mod config;
mod game;
mod path_resolution;
mod snapshot;
mod sync;

use std::sync::Arc;

use crate::hooks::HookPipeline;

pub use sync::{
    CloudLibraryCutoverOutcome, CloudLibraryJoinOutcome, CloudLibraryServiceError,
    CloudLibraryStatus,
};

#[derive(Clone)]
pub struct ServiceContext {
    pipeline: Arc<HookPipeline>,
}

impl ServiceContext {
    pub fn new(pipeline: Arc<HookPipeline>) -> Self {
        Self { pipeline }
    }

    pub fn pipeline(&self) -> &HookPipeline {
        self.pipeline.as_ref()
    }
}
