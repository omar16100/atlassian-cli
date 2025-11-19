fmt:
	cargo fmt

clippy:
	cargo clippy --all-targets --all-features

check:
	cargo check

test:
	cargo test

install:
	cargo install --path crates/cli

.PHONY: fmt clippy check test install
