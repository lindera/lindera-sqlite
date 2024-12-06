LINDERA_SQLITE_VERSION ?= $(shell cargo metadata --no-deps --format-version=1 | jq -r '.packages[] | select(.name=="lindera-sqlite") | .version')

.DEFAULT_GOAL := build

clean:
	cargo clean

format:
	cargo fmt

lint:
	cargo clippy --features=cjk

build:
	cargo build --release --features=cjk

test:
	LINDERA_CONFIG_PATH=./resources/lindera.yml cargo test --features=cjk

bench:
	LINDERA_CONFIG_PATH=./resources/lindera.yml cargo bench --features=cjk

tag:
	git tag v$(LINDERA_SQLITE_VERSION)
	git push origin v$(LINDERA_SQLITE_VERSION)

publish:
ifeq ($(shell curl -s -XGET https://crates.io/api/v1/crates/lindera-sqlite | jq -r '.versions[].num' | grep $(LINDERA_SQLITE_VERSION)),)
	(cargo package && cargo publish)
endif
