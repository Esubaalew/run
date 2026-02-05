use super::*;
use crate::v2::{Error, Result};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct AsyncConfig {
    pub timeout: Option<Duration>,
    pub max_concurrent: usize,
    pub enable_fuel: bool,
    pub fuel_limit: u64,
}

impl Default for AsyncConfig {
    fn default() -> Self {
        Self {
            timeout: Some(Duration::from_secs(30)),
            max_concurrent: 100,
            enable_fuel: true,
            fuel_limit: 1_000_000_000,
        }
    }
}

#[derive(Debug)]
pub struct AsyncCallResult {
    pub result: ExecutionResult,
    pub wait_time_ms: u64,
    pub exec_time_ms: u64,
}

#[cfg(feature = "v2")]
pub struct EpochTicker {
    running: Arc<AtomicBool>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

#[cfg(feature = "v2")]
impl EpochTicker {
    pub fn start(engine: wasmtime::Engine, interval: Duration) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        let handle = tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            while running_clone.load(Ordering::SeqCst) {
                interval_timer.tick().await;
                engine.increment_epoch();
            }
        });

        Self {
            running,
            handle: Some(handle),
        }
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

#[cfg(feature = "v2")]
impl Drop for EpochTicker {
    fn drop(&mut self) {
        self.stop();
    }
}

pub async fn call_async(
    runtime: &RuntimeEngine,
    handle: &InstanceHandle,
    function: &str,
    args: Vec<ComponentValue>,
    config: AsyncConfig,
) -> Result<AsyncCallResult> {
    let start = std::time::Instant::now();

    #[cfg(feature = "v2")]
    let _ticker = if config.timeout.is_some() {
        Some(EpochTicker::start(
            runtime.wasmtime_engine().clone(),
            Duration::from_millis(10),
        ))
    } else {
        None
    };

    let result = if let Some(timeout) = config.timeout {
        tokio::time::timeout(timeout, async { runtime.call(handle, function, args) })
            .await
            .map_err(|_| Error::ExecutionFailed {
                component: handle.component_id.clone(),
                reason: format!("Call timed out after {:?}", timeout),
            })?
    } else {
        runtime.call(handle, function, args)
    };

    let exec_time_ms = start.elapsed().as_millis() as u64;

    result.map(|r| AsyncCallResult {
        result: r,
        wait_time_ms: 0,
        exec_time_ms,
    })
}

#[cfg(not(feature = "v2"))]
pub async fn call_async_fallback(
    runtime: &RuntimeEngine,
    handle: &InstanceHandle,
    function: &str,
    args: Vec<ComponentValue>,
    config: AsyncConfig,
) -> Result<AsyncCallResult> {
    let start = std::time::Instant::now();

    let result = if let Some(timeout) = config.timeout {
        tokio::time::timeout(timeout, async { runtime.call(handle, function, args) })
            .await
            .map_err(|_| Error::ExecutionFailed {
                component: handle.component_id.clone(),
                reason: format!("Call timed out after {:?}", timeout),
            })?
    } else {
        runtime.call(handle, function, args)
    };

    let exec_time_ms = start.elapsed().as_millis() as u64;

    result.map(|r| AsyncCallResult {
        result: r,
        wait_time_ms: 0,
        exec_time_ms,
    })
}

pub async fn call_parallel<'a>(
    runtime: &'a RuntimeEngine,
    calls: Vec<(&'a InstanceHandle, &'a str, Vec<ComponentValue>)>,
    config: AsyncConfig,
) -> Vec<Result<AsyncCallResult>> {
    let handles: Vec<_> = calls
        .into_iter()
        .map(|(handle, function, args)| {
            let runtime = runtime;
            let config = config.clone();
            async move { call_async(runtime, handle, function, args, config).await }
        })
        .collect();

    futures::future::join_all(handles).await
}

pub struct AsyncBatchExecutor {
    config: AsyncConfig,
    semaphore: Arc<tokio::sync::Semaphore>,
}

impl AsyncBatchExecutor {
    pub fn new(config: AsyncConfig) -> Self {
        let semaphore = Arc::new(tokio::sync::Semaphore::new(config.max_concurrent));
        Self { config, semaphore }
    }

    pub async fn execute(
        &self,
        runtime: &RuntimeEngine,
        handle: &InstanceHandle,
        function: &str,
        args: Vec<ComponentValue>,
    ) -> Result<AsyncCallResult> {
        let start = std::time::Instant::now();

        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| Error::ExecutionFailed {
                component: handle.component_id.clone(),
                reason: "Failed to acquire execution slot".to_string(),
            })?;

        let wait_time_ms = start.elapsed().as_millis() as u64;

        let exec_start = std::time::Instant::now();
        let result = call_async(runtime, handle, function, args, self.config.clone()).await?;

        Ok(AsyncCallResult {
            result: result.result,
            wait_time_ms,
            exec_time_ms: exec_start.elapsed().as_millis() as u64,
        })
    }

    pub async fn execute_batch<'a>(
        &'a self,
        runtime: &'a RuntimeEngine,
        calls: Vec<(&'a InstanceHandle, &'a str, Vec<ComponentValue>)>,
    ) -> Vec<Result<AsyncCallResult>> {
        let handles: Vec<_> = calls
            .into_iter()
            .map(|(handle, function, args)| self.execute(runtime, handle, function, args))
            .collect();

        futures::future::join_all(handles).await
    }
}

#[derive(Debug)]
pub enum AsyncEvent {
    Started {
        handle_id: String,
        function: String,
    },
    Completed {
        handle_id: String,
        function: String,
        duration_ms: u64,
    },
    Failed {
        handle_id: String,
        function: String,
        error: String,
    },
    Timeout {
        handle_id: String,
        function: String,
    },
}

#[derive(Debug, Default, Clone)]
pub struct AsyncMetrics {
    pub total_calls: u64,
    pub successful_calls: u64,
    pub failed_calls: u64,
    pub timed_out_calls: u64,
    pub total_exec_time_ms: u64,
    pub total_wait_time_ms: u64,
    pub peak_concurrent: usize,
}

impl AsyncMetrics {
    pub fn record_success(&mut self, exec_time_ms: u64, wait_time_ms: u64) {
        self.total_calls += 1;
        self.successful_calls += 1;
        self.total_exec_time_ms += exec_time_ms;
        self.total_wait_time_ms += wait_time_ms;
    }

    pub fn record_failure(&mut self) {
        self.total_calls += 1;
        self.failed_calls += 1;
    }

    pub fn record_timeout(&mut self) {
        self.total_calls += 1;
        self.timed_out_calls += 1;
    }

    pub fn average_exec_time_ms(&self) -> f64 {
        if self.successful_calls == 0 {
            0.0
        } else {
            self.total_exec_time_ms as f64 / self.successful_calls as f64
        }
    }

    pub fn success_rate(&self) -> f64 {
        if self.total_calls == 0 {
            1.0
        } else {
            self.successful_calls as f64 / self.total_calls as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_async_config_default() {
        let config = AsyncConfig::default();
        assert_eq!(config.max_concurrent, 100);
        assert!(config.enable_fuel);
    }

    #[test]
    fn test_async_metrics() {
        let mut metrics = AsyncMetrics::default();
        metrics.record_success(100, 10);
        metrics.record_success(200, 20);
        metrics.record_failure();

        assert_eq!(metrics.total_calls, 3);
        assert_eq!(metrics.successful_calls, 2);
        assert_eq!(metrics.failed_calls, 1);
        assert_eq!(metrics.average_exec_time_ms(), 150.0);
    }
}
