use atlassian_cli_api::pagination::{BitbucketPage, JiraPage, Page};
use atlassian_cli_api::ratelimit::RateLimiter;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;

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

// The previous benchmarks here measured `PagedResponse::has_next`/`next_start`,
// arithmetic on a type no production code called, shaped for a Jira endpoint
// that has since been removed. These measure what the pagination path actually
// does per page: deserialize a page and split it into items plus a cursor, and
// rebuild the request for the next one.

fn bitbucket_body(page_size: usize) -> serde_json::Value {
    serde_json::json!({
        "values": (0..page_size).map(|i| serde_json::json!({"id": i})).collect::<Vec<_>>(),
        "next": "https://api.bitbucket.org/2.0/items?page=2",
        "size": 1000,
    })
}

fn bench_page_into_parts(c: &mut Criterion) {
    let mut group = c.benchmark_group("pagination_page_into_parts");

    for page_size in [10, 50, 100, 500].iter() {
        let body = serde_json::to_string(&bitbucket_body(*page_size)).unwrap();
        group.bench_with_input(BenchmarkId::from_parameter(page_size), &body, |b, body| {
            b.iter(|| {
                let page: BitbucketPage<serde_json::Value> =
                    serde_json::from_str(black_box(body)).unwrap();
                black_box(page.into_parts())
            });
        });
    }

    group.finish();
}

fn bench_jira_page_into_parts(c: &mut Criterion) {
    let mut group = c.benchmark_group("pagination_jira_into_parts");

    let body = serde_json::to_string(&serde_json::json!({
        "issues": (0..100).map(|i| serde_json::json!({"id": i})).collect::<Vec<_>>(),
        "nextPageToken": "CAEaAggD",
        "isLast": false,
    }))
    .unwrap();

    group.bench_function("hundred_issues", |b| {
        b.iter(|| {
            let page: JiraPage<serde_json::Value> = serde_json::from_str(black_box(&body)).unwrap();
            black_box(page.into_parts())
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_rate_limiter_concurrent_access,
    bench_page_into_parts,
    bench_jira_page_into_parts
);
criterion_main!(benches);
