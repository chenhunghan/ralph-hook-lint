.PHONY: build lint fmt check test ci

build:
	cargo build --release
	mkdir -p bin
	cp target/release/ralph-hook-lint bin/

lint:
	cargo clippy --all-targets --all-features -- -D warnings

fmt:
	cargo fmt

fmt-check:
	cargo fmt -- --check

check:
	cargo check --all-targets --all-features

test:
	cargo test

ci: fmt-check lint test
