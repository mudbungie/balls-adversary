.PHONY: build test lint coverage install install-hooks clean

build:
	cargo build --release

test:
	cargo test

lint:
	cargo clippy --all-targets -- -D warnings

coverage:
	scripts/check-coverage.sh

# Install the adversary binary BESIDE the `bl` binary, where balls resolves
# plugins (config/plugins/bin/<name> symlinks point here). Override BL_DIR to
# target a different bl install.
BL_DIR ?= $(dir $(shell command -v bl))
install: build
	install -m 0755 target/release/adversary "$(BL_DIR)adversary"
	@echo "Installed adversary -> $(BL_DIR)adversary"
	@echo "Wire it into the close gate:  bl conf prepend close.pre adversary"

# Install the pre-commit gate (clippy, 300-line cap, tests, 100% coverage).
install-hooks:
	scripts/install-hooks.sh

clean:
	cargo clean
