use iced::executor;
use std::sync::Arc;
use tokio::runtime::Runtime;

#[derive(Debug)]
pub struct CustomExecutor {
    runtime: Arc<Runtime>,
}

impl executor::Executor for CustomExecutor {
    fn new() -> Result<Self, std::io::Error> {
        let runtime = Runtime::new()?;
        Ok(Self {
            runtime: Arc::new(runtime),
        })
    }

    fn spawn(&self, future: impl std::future::Future<Output = ()> + Send + 'static) {
        self.runtime.spawn(future);
    }
}
