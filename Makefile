.PHONY: build test clippy fmt docker run

build:
	cargo build --release

test:
	cargo test

clippy:
	cargo clippy -- -D warnings

fmt:
	cargo fmt -- --check

docker:
	docker build -t omem-server .

run:
	cargo run --release
