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

# Run all pre-commit checks locally
pre-commit: fmt clippy test
	@echo "✅ All checks passed!"

# Quick check (no tests)
quick-check: fmt clippy
	@echo "✅ Quick checks passed!"

# CI simulation
ci: pre-commit
	@echo "✅ CI checks passed!"

.PHONY: fmt clippy check test install pre-commit quick-check ci
