.PHONY: build

build:
	cargo build --release
	mkdir -p bin
	cp target/release/lint bin/
