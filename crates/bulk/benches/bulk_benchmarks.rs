use atlassian_cli_bulk::{BulkConfig, BulkExecutor};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::time::Duration;

async fn dummy_task(_item: usize) -> anyhow::Result<()> {
    // Simulate a small amount of work (e.g., an API call)
    tokio::time::sleep(Duration::from_micros(100)).await;
    Ok(())
}

fn bench_concurrency_levels(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("concurrency_levels");

    for concurrency in [1, 4, 8, 16].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(concurrency),
            concurrency,
            |b, &concurrency| {
                b.to_async(&runtime).iter(|| async move {
                    let config = BulkConfig {
                        concurrency,
                        dry_run: false,
                        show_progress: false,
                        fail_fast: false,
                    };
                    let executor = BulkExecutor::from_config(config);
                    let items: Vec<usize> = (0..100).collect();
                    executor.run(black_box(items), dummy_task).await.unwrap();
                });
            },
        );
    }
    group.finish();
}

fn bench_task_counts(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("task_counts");

    for count in [10, 50, 100, 200].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &count| {
            b.to_async(&runtime).iter(|| async move {
                let config = BulkConfig {
                    concurrency: 8,
                    dry_run: false,
                    show_progress: false,
                    fail_fast: false,
                };
                let executor = BulkExecutor::from_config(config);
                let items: Vec<usize> = (0..count).collect();
                executor.run(black_box(items), dummy_task).await.unwrap();
            });
        });
    }
    group.finish();
}

fn bench_progress_bar_overhead(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("progress_bar_overhead");

    for show_progress in [false, true].iter() {
        group.bench_with_input(
            BenchmarkId::new("progress", if *show_progress { "on" } else { "off" }),
            show_progress,
            |b, &show_progress| {
                b.to_async(&runtime).iter(|| async move {
                    let config = BulkConfig {
                        concurrency: 8,
                        dry_run: false,
                        show_progress,
                        fail_fast: false,
                    };
                    let executor = BulkExecutor::from_config(config);
                    let items: Vec<usize> = (0..100).collect();
                    executor.run(black_box(items), dummy_task).await.unwrap();
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_concurrency_levels,
    bench_task_counts,
    bench_progress_bar_overhead
);
criterion_main!(benches);
