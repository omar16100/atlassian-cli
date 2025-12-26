use atlassian_cli_auth::encryption::{decrypt, derive_key, encrypt};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;

fn bench_key_derivation(c: &mut Criterion) {
    c.bench_function("derive_key", |b| {
        b.iter(|| {
            // Key derivation uses Argon2 which is intentionally slow
            derive_key().unwrap()
        });
    });
}

fn bench_encryption(c: &mut Criterion) {
    let key = derive_key().unwrap();
    let mut group = c.benchmark_group("encryption");

    for size in [16, 64, 256, 1024].iter() {
        let plaintext = "a".repeat(*size);
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| encrypt(black_box(&plaintext), black_box(&key)).unwrap());
        });
    }
    group.finish();
}

fn bench_decryption(c: &mut Criterion) {
    let key = derive_key().unwrap();
    let mut group = c.benchmark_group("decryption");

    for size in [16, 64, 256, 1024].iter() {
        let plaintext = "a".repeat(*size);
        let (nonce, ciphertext) = encrypt(&plaintext, &key).unwrap();

        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| decrypt(black_box(&ciphertext), black_box(&nonce), black_box(&key)).unwrap());
        });
    }
    group.finish();
}

fn bench_encrypt_decrypt_roundtrip(c: &mut Criterion) {
    let key = derive_key().unwrap();
    let mut group = c.benchmark_group("encrypt_decrypt_roundtrip");

    for size in [16, 64, 256, 1024].iter() {
        let plaintext = "a".repeat(*size);
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                let (nonce, ciphertext) = encrypt(black_box(&plaintext), black_box(&key)).unwrap();
                decrypt(black_box(&ciphertext), black_box(&nonce), black_box(&key)).unwrap()
            });
        });
    }
    group.finish();
}

fn bench_token_sizes(c: &mut Criterion) {
    let key = derive_key().unwrap();
    let mut group = c.benchmark_group("token_encryption");

    // Typical API token sizes
    let token_sizes = [
        ("short", 32),  // Short token
        ("medium", 64), // Medium token
        ("long", 128),  // Long token
        ("jwt", 512),   // JWT-like token
    ];

    for (name, size) in token_sizes.iter() {
        let token = "t".repeat(*size);
        group.bench_with_input(BenchmarkId::new("encrypt", name), &token, |b, token| {
            b.iter(|| encrypt(black_box(token), black_box(&key)).unwrap());
        });

        let (nonce, ciphertext) = encrypt(&token, &key).unwrap();
        group.bench_with_input(BenchmarkId::new("decrypt", name), &token, |b, _| {
            b.iter(|| decrypt(black_box(&ciphertext), black_box(&nonce), black_box(&key)).unwrap());
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_key_derivation,
    bench_encryption,
    bench_decryption,
    bench_encrypt_decrypt_roundtrip,
    bench_token_sizes
);
criterion_main!(benches);
