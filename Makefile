LINDERA_SQLITE_VERSION ?= $(shell cargo metadata --no-deps --format-version=1 | jq -r '.packages[] | select(.name=="lindera-sqlite") | .version')

USER_AGENT ?= $(shell curl --version | head -n1 | awk '{print $1"/"$2}')
USER ?= $(shell whoami)
HOSTNAME ?= $(shell hostname)

.DEFAULT_GOAL := help

clean: ## Clean the project
	cargo clean

format: ## Format the code
	cargo fmt

lint: ## Run linter
	cargo clippy --features=embedded-cjk

build: ## Build the project
	cargo build --release --features=embedded-cjk

test: ## Run tests
	LINDERA_CONFIG_PATH=./resources/lindera.yml cargo test --features=embedded-cjk

bench: ## Run benchmarks
	LINDERA_CONFIG_PATH=./resources/lindera.yml cargo bench --features=embedded-cjk

tag: ## Make a new tag for the current version
	git tag v$(LINDERA_SQLITE_VERSION)
	git push origin v$(LINDERA_SQLITE_VERSION)

publish: ## Publish the crate to crates.io
ifeq ($(shell curl -s -XGET -H "User-Agent: $(USER_AGENT) ($(USER)@$(HOSTNAME))" https://crates.io/api/v1/crates/lindera-sqlite | jq -r '.versions[].num' | grep $(LINDERA_SQLITE_VERSION)),)
	(cargo package && cargo publish)
endif

help: ## Show help
	@echo "Available targets:"
	@grep -E '^[a-zA-Z0-9_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  %-15s %s\n", $$1, $$2}'
