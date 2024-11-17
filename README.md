# Overview

lindera-sqlite is a C ABI library which exposes a [FTS5](https://www.sqlite.org/fts5.html) tokenizer function.

When used as a custom FTS5 tokenizer this enables application to support Chinese, Japanese and Korean in full-text search.

## Build extension

```sh
% cargo build --features=ipadic,ko-dic,cc-cedict,compress,extension
```

## Set enviromment variable for Lindera configuration

```sh
% export LINDERA_CONFIG_PATH=./resources/lindera.yml
```

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
