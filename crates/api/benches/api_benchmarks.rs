use atlassian_cli_api::pagination::PagedResponse;
use atlassian_cli_api::ratelimit::RateLimiter;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

fn bench_rate_limiter_concurrent_access(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("rate_limiter_concurrent");

    for num_tasks in [1, 4, 8, 16].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_tasks),
            num_tasks,
            |b, &num_tasks| {
                b.to_async(&runtime).iter(|| async move {
                    let limiter = RateLimiter::new();

                    // Simulate concurrent access to rate limiter state
                    let tasks: Vec<_> = (0..num_tasks)
                        .map(|_| {
                            let limiter = limiter.clone();
                            tokio::spawn(async move {
                                limiter.check_limit().await;
                            })
                        })
                        .collect();

                    for task in tasks {
                        task.await.unwrap();
                    }
                });
            },
        );
    }
    group.finish();
}

fn bench_pagination_logic(c: &mut Criterion) {
    let mut group = c.benchmark_group("pagination");

    // Benchmark has_next() calculations
    group.bench_function("has_next_with_is_last", |b| {
        let page: PagedResponse<i32> = PagedResponse {
            values: vec![1, 2, 3],
            start_at: Some(0),
            max_results: Some(10),
            total: Some(100),
            is_last: Some(false),
        };
        b.iter(|| black_box(&page).has_next());
    });

    group.bench_function("has_next_with_calculation", |b| {
        let page: PagedResponse<i32> = PagedResponse {
            values: vec![1, 2, 3],
            start_at: Some(0),
            max_results: Some(10),
            total: Some(100),
            is_last: None,
        };
        b.iter(|| black_box(&page).has_next());
    });

    // Benchmark next_start() calculations
    group.bench_function("next_start", |b| {
        let page: PagedResponse<i32> = PagedResponse {
            values: vec![1, 2, 3],
            start_at: Some(0),
            max_results: Some(10),
            total: Some(100),
            is_last: Some(false),
        };
        b.iter(|| black_box(&page).next_start());
    });

    group.finish();
}

fn bench_pagination_page_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("pagination_page_processing");

    for page_size in [10, 50, 100, 500].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(page_size),
            page_size,
            |b, &page_size| {
                let page: PagedResponse<i32> = PagedResponse {
                    values: (0..page_size).collect(),
                    start_at: Some(0),
                    max_results: Some(page_size as u32),
                    total: Some(1000),
                    is_last: Some(false),
                };
                b.iter(|| {
                    let has_next = black_box(&page).has_next();
                    let next = black_box(&page).next_start();
                    (has_next, next)
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_rate_limiter_concurrent_access,
    bench_pagination_logic,
    bench_pagination_page_sizes
);
criterion_main!(benches);
