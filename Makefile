.PHONY: build test check lint coverage install install-hooks clean

build:
	cargo build --release

test:
	cargo test

lint:
	cargo clippy --all-targets -- -D warnings

coverage:
	scripts/check-coverage.sh

# The complete gate — what the noodlezoo builder runs on the staged tree
# (README "The gate"). The pre-commit hook execs bl-gate and runs none of
# this here; by hand, `bl-remote-run check` runs it there.
check: lint test
	scripts/check-line-lengths.sh
	scripts/check-coverage.sh

# Install the adversary binary BESIDE the `bl` binary, where balls resolves
# plugins (config/plugins/bin/<name> symlinks point here). Override BL_DIR to
# target a different bl install.
BL_DIR ?= $(dir $(shell command -v bl))
install: build
	install -m 0755 target/release/adversary "$(BL_DIR)adversary"
	@echo "Installed adversary -> $(BL_DIR)adversary"
	@echo "Wire it into the close gate:  bl conf prepend close.pre adversary"

# Seat the pre-commit hook: a stub that execs the committing worktree's
# scripts/pre-commit, which execs bl-gate (README "The gate").
install-hooks:
	scripts/install-hooks.sh

clean:
	cargo clean
