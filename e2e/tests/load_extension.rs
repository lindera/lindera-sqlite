//! Loads the built extension into SQLite and uses it through FTS5.
//!
//! The test needs two environment variables:
//!
//! - `LINDERA_SQLITE_LIB_DIR`: the directory of the built extension, such as `target/debug`
//! - `LINDERA_CONFIG_FILE`: the Lindera configuration, as an absolute path, since the tests run in
//!   the directory of this package

use std::env::consts::{DLL_PREFIX, DLL_SUFFIX};
use std::env::var_os;
use std::path::PathBuf;

use rusqlite::Connection;

/// Returns the path of the built extension on this platform.
fn extension_path() -> PathBuf {
    let dir = var_os("LINDERA_SQLITE_LIB_DIR")
        .expect("LINDERA_SQLITE_LIB_DIR should name the directory of the built extension");
    PathBuf::from(dir).join(format!("{DLL_PREFIX}lindera_sqlite{DLL_SUFFIX}"))
}

/// Opens a database in memory with the extension loaded.
fn open_with_extension() -> Connection {
    let connection = Connection::open_in_memory().expect("SQLite should open a database");
    // SAFETY: The extension is the one built from this repository, and no SQL runs before it is
    // loaded.
    unsafe {
        connection
            .load_extension_enable()
            .expect("SQLite should allow loading extensions");
        connection
            .load_extension(extension_path(), Some("lindera_fts5_tokenizer_init"))
            .expect("SQLite should load the extension");
    }
    connection
        .load_extension_disable()
        .expect("SQLite should disallow loading extensions again");
    connection
}

#[test]
fn it_indexes_and_finds_japanese_text() {
    let connection = open_with_extension();
    connection
        .execute_batch(
            "CREATE VIRTUAL TABLE example USING fts5(content, tokenize='lindera_tokenizer');
             CREATE VIRTUAL TABLE terms USING fts5vocab(example, 'instance');
             INSERT INTO example(rowid, content) VALUES
               (1, 'Ｌｉｎｄｅｒａは形態素解析ｴﾝｼﾞﾝです。ユーザー辞書も利用可能です。'),
               (2, '東京都に住んでいます。');",
        )
        .expect("FTS5 should index the text with the tokenizer");

    let terms: Vec<String> = connection
        .prepare("SELECT term FROM terms WHERE doc = 1 ORDER BY offset")
        .expect("fts5vocab should be queried")
        .query_map([], |row| row.get(0))
        .expect("fts5vocab should return rows")
        .collect::<Result<_, _>>()
        .expect("the terms should be text");
    assert_eq!(
        terms,
        [
            "Lindera",
            "形態素",
            "解析",
            "エンジン",
            "ユーザ",
            "辞書",
            "利用",
            "可能"
        ]
    );

    let found: Vec<i64> = connection
        .prepare("SELECT rowid FROM example WHERE example MATCH '形態素解析'")
        .expect("MATCH should be queried")
        .query_map([], |row| row.get(0))
        .expect("MATCH should return rows")
        .collect::<Result<_, _>>()
        .expect("the row IDs should be integers");
    assert_eq!(found, [1]);
}
