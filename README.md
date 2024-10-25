# Overview

lindera-sqlite is a C ABI library which exposes a [FTS5](https://www.sqlite.org/fts5.html) tokenizer function.

When used as a custom FTS5 tokenizer this enables application to support Chinese, Japanese and Korean in full-text search.

## Extension Build/Usage Example

```sh
cargo rustc --features extension -- --crate-type=cdylib
```

Load extension from `./target/release/liblindera_tokenizer.dylib`.

```sql
CREATE VIRTUAL TABLE
fts
USING fts5(content, tokenize='lindera_tokenizer')
```

## Generating headers

```sh
cbindgen --profile release . -o target/release/fts5-tokenizer.h
```
