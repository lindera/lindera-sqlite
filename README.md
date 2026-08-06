# Overview

lindera-sqlite is a C ABI library which exposes a [FTS5](https://www.sqlite.org/fts5.html) tokenizer function.

When used as a custom FTS5 tokenizer this enables application to support Chinese, Japanese and Korean in full-text search.

## Use a prebuilt extension

Every [release](https://github.com/lindera/lindera-sqlite/releases) publishes a per-platform, per-dictionary archive containing the shared library and a `lindera.yml` matching the dictionary that archive embedded. Unpack it and point `LINDERA_CONFIG_PATH` at that file:

```sh
% unzip lindera-sqlite-x86_64-unknown-linux-gnu-v2.0.0.zip
% export LINDERA_CONFIG_PATH=./lindera.yml
```

Then skip to [Load extension](#load-extension), using the unpacked library path.

## Build extension

```sh
% cargo build --features=embed-cjk
```

Each `embed-*` feature embeds a different set of dictionaries into the built extension:

| Feature | Embedded dictionaries |
| --- | --- |
| `embed-ipadic` | Japanese (IPADIC) |
| `embed-ipadic-neologd` | Japanese (IPADIC NEologd) |
| `embed-unidic` | Japanese (UniDic) |
| `embed-ko-dic` | Korean (ko-dic) |
| `embed-cc-cedict` | Chinese (CC-CEDICT) |
| `embed-jieba` | Chinese (Jieba) |
| `embed-cjk` | Japanese (IPADIC) + Korean (ko-dic) + Chinese (Jieba) |

## Set enviromment variable for Lindera configuration

When building from source, use the config in `resources/` matching the feature you built. `resources/lindera.yml` is the IPADIC one, used by `embed-ipadic` and `embed-cjk`:

```sh
% export LINDERA_CONFIG_PATH=./resources/lindera.yml
```

| Feature | Configuration |
| --- | --- |
| `embed-ipadic`, `embed-cjk` | `resources/lindera.yml` |
| `embed-unidic` | `resources/lindera-unidic.yml` |
| `embed-ko-dic` | `resources/lindera-ko-dic.yml` |
| `embed-cc-cedict` | `resources/lindera-cc-cedict.yml` |
| `embed-jieba` | `resources/lindera-jieba.yml` |

`embed-cjk` embeds three dictionaries, but a configuration selects one; edit the `dictionary:` line to use ko-dic or Jieba instead.

## Then start SQLite

```sh
% sqlite3 example.db
```

## Load extension

```sql
sqlite> .load ./target/debug/liblindera_sqlite lindera_fts5_tokenizer_init
```

## Create table using FTS5 with Lindera tokenizer

```sql
sqlite> CREATE VIRTUAL TABLE example USING fts5(content, tokenize='lindera_tokenizer');
```

## Insert data

```sql
sqlite> INSERT INTO example(content) VALUES ("Ｌｉｎｄｅｒａは形態素解析ｴﾝｼﾞﾝです。ユーザー辞書も利用可能です。");
```

## Search data

```sql
sqlite> SELECT * FROM example WHERE content MATCH "Lindera" ORDER BY bm25(example) LIMIT 10;
```
