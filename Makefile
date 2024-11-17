LINDERA_SQLITE_VERSION ?= $(shell cargo metadata --no-deps --format-version=1 | jq -r '.packages[] | select(.name=="lindera-sqlite") | .version')

tag:
	git tag v$(LINDERA_SQLITE_VERSION)
	git push origin v$(LINDERA_SQLITE_VERSION)

publish:
ifeq ($(shell curl -s -XGET https://crates.io/api/v1/crates/lindera-sqlite | jq -r '.versions[].num' | grep $(LINDERA_SQLITE_VERSION)),)
	(cargo package && cargo publish)
