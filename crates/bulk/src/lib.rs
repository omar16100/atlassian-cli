use std::sync::Arc;

use anyhow::Result;
use futures::stream::{self, StreamExt, TryStreamExt};
use indicatif::{ProgressBar, ProgressStyle};
use thiserror::Error;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

#[derive(Error, Debug)]
pub enum BulkError {
    #[error("Multiple tasks failed: {count} failures")]
    MultipleFailed { count: usize },

    #[error("Semaphore acquire error: {0}")]
    SemaphoreError(#[from] tokio::sync::AcquireError),
}

#[derive(Clone)]
pub struct BulkConfig {
    pub concurrency: usize,
    pub dry_run: bool,
    pub show_progress: bool,
    pub fail_fast: bool,
}

impl Default for BulkConfig {
    fn default() -> Self {
        Self {
            concurrency: 4,
            dry_run: false,
            show_progress: true,
            fail_fast: false,
        }
    }
}

#[derive(Debug)]
pub struct BulkResult<T> {
    pub successful: Vec<T>,
    pub failed: Vec<(usize, anyhow::Error)>,
}

impl<T> BulkResult<T> {
    pub fn is_complete_success(&self) -> bool {
        self.failed.is_empty()
    }

    pub fn success_count(&self) -> usize {
        self.successful.len()
    }

    pub fn failure_count(&self) -> usize {
        self.failed.len()
    }
}

/// Executes multiple operations with optional concurrency limits and dry-run support.
pub struct BulkExecutor {
    concurrency: usize,
    dry_run: bool,
    show_progress: bool,
    fail_fast: bool,
}

impl BulkExecutor {
    pub fn new(concurrency: usize, dry_run: bool) -> Self {
        Self {
            concurrency: concurrency.max(1),
            dry_run,
            show_progress: true,
            fail_fast: false,
        }
    }

    pub fn from_config(config: BulkConfig) -> Self {
        Self {
            concurrency: config.concurrency.max(1),
            dry_run: config.dry_run,
            show_progress: config.show_progress,
            fail_fast: config.fail_fast,
        }
    }

    pub fn with_progress(mut self, show_progress: bool) -> Self {
        self.show_progress = show_progress;
        self
    }

    pub fn with_fail_fast(mut self, fail_fast: bool) -> Self {
        self.fail_fast = fail_fast;
        self
    }

    pub async fn run<T, Fut, F>(&self, items: Vec<T>, job: F) -> Result<()>
    where
        T: Send + Sync + std::fmt::Debug + 'static,
        F: Fn(T) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<()>> + Send,
    {
        if items.is_empty() {
            debug!("No items to process");
            return Ok(());
        }

        let total = items.len();
        info!(
            total,
            concurrency = self.concurrency,
            "Starting bulk execution"
        );

        let semaphore = Arc::new(Semaphore::new(self.concurrency));
        let job = Arc::new(job);
        let progress = self.create_progress_bar(total);
        let dry_run = self.dry_run;

        let results = stream::iter(items.into_iter().enumerate().map(|(idx, item)| {
            let job = Arc::clone(&job);
            let semaphore = Arc::clone(&semaphore);
            let progress = progress.clone();
            async move {
                let _permit = semaphore.acquire().await?;
                if dry_run {
                    info!(?item, "Dry run: skipping execution");
                    progress.inc(1);
                    return Ok(());
                }
                debug!(index = idx, "Processing item");
                match job(item).await {
                    Ok(()) => {
                        progress.inc(1);
                        Ok(())
                    }
                    Err(e) => {
                        warn!(index = idx, error = %e, "Task failed");
                        progress.inc(1);
                        Err(e)
                    }
                }
            }
        }))
        .buffer_unordered(self.concurrency);

        if self.fail_fast {
            results.try_collect::<Vec<_>>().await?;
        } else {
            let all_results: Vec<Result<()>> = results.collect().await;
            let failures: Vec<_> = all_results.into_iter().filter_map(|r| r.err()).collect();

            if !failures.is_empty() {
                warn!(failure_count = failures.len(), "Some tasks failed");
                progress.finish_with_message(format!("Completed with {} failures", failures.len()));
                return Err(BulkError::MultipleFailed {
                    count: failures.len(),
                }
                .into());
            }
        }

        progress.finish_with_message("All tasks completed successfully");
        info!(total, "Bulk execution completed");
        Ok(())
    }

    pub async fn execute_with_results<T, R, Fut, F>(
        &self,
        items: Vec<T>,
        job: F,
    ) -> Result<BulkResult<R>>
    where
        T: Send + Sync + std::fmt::Debug + 'static,
        R: Send + 'static,
        F: Fn(T) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<R>> + Send,
    {
        if items.is_empty() {
            debug!("No items to process");
            return Ok(BulkResult {
                successful: vec![],
                failed: vec![],
            });
        }

        let total = items.len();
        info!(
            total,
            concurrency = self.concurrency,
            "Starting bulk execution with results"
        );

        let semaphore = Arc::new(Semaphore::new(self.concurrency));
        let job = Arc::new(job);
        let progress = self.create_progress_bar(total);
        let dry_run = self.dry_run;

        let results: Vec<(usize, Result<R>)> =
            stream::iter(items.into_iter().enumerate().map(|(idx, item)| {
                let job = Arc::clone(&job);
                let semaphore = Arc::clone(&semaphore);
                let progress = progress.clone();
                async move {
                    let _permit = semaphore.acquire().await?;
                    if dry_run {
                        info!(?item, "Dry run: skipping execution");
                        progress.inc(1);
                        return Ok::<(usize, Result<R>), anyhow::Error>((
                            idx,
                            Err(anyhow::anyhow!("Dry run")),
                        ));
                    }
                    debug!(index = idx, "Processing item");
                    let result = job(item).await;
                    progress.inc(1);
                    Ok((idx, result))
                }
            }))
            .buffer_unordered(self.concurrency)
            .try_collect()
            .await?;

        let mut successful = Vec::new();
        let mut failed = Vec::new();

        for (idx, result) in results {
            match result {
                Ok(value) => successful.push(value),
                Err(error) => failed.push((idx, error)),
            }
        }

        if !failed.is_empty() {
            warn!(
                success_count = successful.len(),
                failure_count = failed.len(),
                "Some tasks failed"
            );
            progress.finish_with_message(format!(
                "Completed: {} succeeded, {} failed",
                successful.len(),
                failed.len()
            ));
        } else {
            progress.finish_with_message("All tasks completed successfully");
        }

        info!(
            success = successful.len(),
            failures = failed.len(),
            "Bulk execution completed"
        );

        Ok(BulkResult { successful, failed })
    }

    fn create_progress_bar(&self, total: usize) -> ProgressBar {
        let progress = if self.show_progress {
            ProgressBar::new(total as u64)
        } else {
            ProgressBar::hidden()
        };

        progress.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
            )
            .unwrap()
            .progress_chars("#>-")
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ "),
        );

        progress
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_new_executor() {
        let executor = BulkExecutor::new(5, false);
        assert_eq!(executor.concurrency, 5);
        assert!(!executor.dry_run);
    }

    #[test]
    fn test_new_executor_zero_concurrency() {
        let executor = BulkExecutor::new(0, false);
        assert_eq!(executor.concurrency, 1);
    }

    #[test]
    fn test_new_executor_dry_run() {
        let executor = BulkExecutor::new(3, true);
        assert_eq!(executor.concurrency, 3);
        assert!(executor.dry_run);
    }

    #[tokio::test]
    async fn test_run_empty_items() {
        let executor = BulkExecutor::new(2, false);
        let items: Vec<i32> = vec![];

        let result = executor.run(items, |_item| async { Ok(()) }).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_single_item() {
        let executor = BulkExecutor::new(1, false);
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        let items = vec![1];
        let result = executor
            .run(items, move |_item| {
                let counter = Arc::clone(&counter_clone);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_run_multiple_items() {
        let executor = BulkExecutor::new(3, false);
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        let items = vec![1, 2, 3, 4, 5];
        let result = executor
            .run(items, move |_item| {
                let counter = Arc::clone(&counter_clone);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(counter.load(Ordering::SeqCst), 5);
    }

    #[tokio::test]
    async fn test_dry_run_skips_execution() {
        let executor = BulkExecutor::new(2, true);
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        let items = vec![1, 2, 3];
        let result = executor
            .run(items, move |_item| {
                let counter = Arc::clone(&counter_clone);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_run_with_error() {
        let executor = BulkExecutor::new(2, false).with_fail_fast(true);
        let items = vec![1, 2, 3];

        let result = executor
            .run(items, |item| async move {
                if item == 2 {
                    anyhow::bail!("Test error on item 2");
                }
                Ok(())
            })
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Test error on item 2"));
    }

    #[tokio::test]
    async fn test_run_with_multiple_errors() {
        let executor = BulkExecutor::new(2, false);
        let items = vec![1, 2, 3, 4];

        let result = executor
            .run(items, |item| async move {
                if item == 2 || item == 4 {
                    anyhow::bail!("Test error on item {}", item);
                }
                Ok(())
            })
            .await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Multiple tasks failed") || err_msg.contains("2 failures"));
    }

    #[tokio::test]
    async fn test_concurrency_limit() {
        use std::time::Duration;
        use tokio::time::sleep;

        let executor = BulkExecutor::new(2, false);
        let active_count = Arc::new(AtomicUsize::new(0));
        let max_concurrent = Arc::new(AtomicUsize::new(0));
        let active_clone = Arc::clone(&active_count);
        let max_clone = Arc::clone(&max_concurrent);

        let items = vec![1, 2, 3, 4, 5];
        let result = executor
            .run(items, move |_item| {
                let active = Arc::clone(&active_clone);
                let max = Arc::clone(&max_clone);
                async move {
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    max.fetch_max(current, Ordering::SeqCst);
                    sleep(Duration::from_millis(10)).await;
                    active.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                }
            })
            .await;

        assert!(result.is_ok());
        assert!(max_concurrent.load(Ordering::SeqCst) <= 2);
    }
}
