//! # lindera-sqlite
//!
//! A SQLite FTS5 (Full-Text Search 5) tokenizer extension that provides support for
//! Chinese, Japanese, and Korean (CJK) text analysis using the Lindera morphological analyzer.
//!
//! ## Features
//!
//! - **CJK Language Support**: Tokenizes Chinese, Japanese, and Korean text using Lindera
//! - **Multiple Dictionaries**: Supports various embedded dictionaries (IPADIC, UniDic, ko-dic, CC-CEDICT)
//! - **Configurable**: Uses YAML configuration for character filters and token filters
//! - **SQLite Integration**: Seamlessly integrates with SQLite's FTS5 full-text search
//!
//! ## Usage
//!
//! ### Building the Extension
//!
//! ```bash
//! cargo build --release --features=embedded-cjk
//! ```
//!
//! ### Setting Up Configuration
//!
//! Set the `LINDERA_CONFIG_PATH` environment variable to point to your Lindera configuration file:
//!
//! ```bash
//! export LINDERA_CONFIG_PATH=./resources/lindera.yml
//! ```
//!
//! ### Loading in SQLite
//!
//! ```sql
//! .load ./target/release/liblindera_sqlite lindera_fts5_tokenizer_init
//! ```
//!
//! ### Creating an FTS5 Table
//!
//! ```sql
//! CREATE VIRTUAL TABLE example USING fts5(content, tokenize='lindera_tokenizer');
//! ```
//!
//! ### Searching
//!
//! ```sql
//! INSERT INTO example(content) VALUES ('日本語の全文検索');
//! SELECT * FROM example WHERE content MATCH '検索';
//! ```
//!
//! ## Architecture
//!
//! This library provides a C ABI interface for SQLite to use Lindera as a custom FTS5 tokenizer.
//! The main components are:
//!
//! - [`load_tokenizer`]: Initializes a Lindera tokenizer with configuration
//! - [`lindera_fts5_tokenize`]: C-compatible entry point for tokenization (called by SQLite)
//! - Internal tokenization logic that converts text to tokens and calls back to SQLite

extern crate alloc;

mod common;
#[cfg(feature = "extension")]
mod extension;

use libc::{c_char, c_int, c_uchar, c_void};

use lindera::tokenizer::{Tokenizer, TokenizerBuilder};

pub use crate::common::*;

/// Loads and initializes a Lindera tokenizer.
///
/// This function creates a new Lindera tokenizer using the configuration specified
/// by the `LINDERA_CONFIG_PATH` environment variable. The configuration file controls
/// segmentation mode, character filters, and token filters.
///
/// # Returns
///
/// - `Ok(Tokenizer)` - Successfully initialized tokenizer
/// - `Err(c_int)` - Returns [`SQLITE_INTERNAL`] if tokenizer creation fails
///
/// # Errors
///
/// This function will return an error if:
/// - The tokenizer builder cannot be created (e.g., missing or invalid configuration)
/// - The tokenizer cannot be built from the builder
///
/// Error messages are written to stderr for debugging purposes.
///
/// # Examples
///
/// ```no_run
/// # use lindera_sqlite::load_tokenizer;
/// std::env::set_var("LINDERA_CONFIG_PATH", "./resources/lindera.yml");
/// let tokenizer = load_tokenizer().expect("Failed to load tokenizer");
/// ```
#[inline]
pub fn load_tokenizer() -> Result<Tokenizer, c_int> {
    let builder = TokenizerBuilder::new().map_err(|e| {
        eprintln!("Failed to create tokenizer builder: {e}");
        SQLITE_INTERNAL
    })?;
    let tokenizer = builder.build().map_err(|e| {
        eprintln!("Failed to create tokenizer: {e}");
        SQLITE_INTERNAL
    })?;

    Ok(tokenizer)
}

/// C-compatible FTS5 tokenization function.
///
/// This is the main entry point called by SQLite's FTS5 extension to tokenize text.
/// It follows the FTS5 tokenizer API specification and provides panic safety by catching
/// any Rust panics that might occur during tokenization.
///
/// # Parameters
///
/// - `tokenizer` - Pointer to the [`Fts5Tokenizer`] instance
/// - `p_ctx` - Context pointer passed to the token callback function
/// - `_flags` - Tokenization flags (currently unused)
/// - `p_text` - Pointer to the input text buffer (UTF-8 encoded)
/// - `n_text` - Length of the input text in bytes
/// - `x_token` - Callback function invoked for each token found
///
/// # Returns
///
/// - [`SQLITE_OK`] - Tokenization completed successfully
/// - [`SQLITE_INTERNAL`] - An internal error occurred (including panics)
/// - Other SQLite error codes propagated from the token callback
///
/// # Safety
///
/// This function is marked as `unsafe(no_mangle)` and `extern "C"` for FFI compatibility.
/// It wraps the internal tokenization logic with panic catching to prevent unwinding
/// across the FFI boundary, which would be undefined behavior.
///
/// The caller must ensure:
/// - `tokenizer` points to a valid [`Fts5Tokenizer`] instance
/// - `p_text` points to valid UTF-8 data of length `n_text`
/// - `x_token` is a valid function pointer
///
/// # C API Example
///
/// ```c
/// // Called by SQLite FTS5 when tokenizing text
/// int rc = lindera_fts5_tokenize(
///     tokenizer,
///     context,
///     0,
///     "日本語テキスト",
///     strlen("日本語テキスト"),
///     my_token_callback
/// );
/// ```
#[unsafe(no_mangle)]
pub extern "C" fn lindera_fts5_tokenize(
    tokenizer: *mut Fts5Tokenizer,
    p_ctx: *mut c_void,
    _flags: c_int,
    p_text: *const c_char,
    n_text: c_int,
    x_token: TokenFunction,
) -> c_int {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || match lindera_fts5_tokenize_internal(tokenizer, p_ctx, p_text, n_text, x_token) {
            Ok(()) => SQLITE_OK,
            Err(code) => code,
        },
    ))
    .unwrap_or(SQLITE_INTERNAL)
}

/// Internal tokenization implementation.
///
/// Performs the actual tokenization of input text and invokes the callback function
/// for each token produced. This function handles UTF-8 validation, tokenization,
/// and error propagation.
///
/// # Parameters
///
/// - `tokenizer` - Pointer to the [`Fts5Tokenizer`] instance
/// - `p_ctx` - Context pointer to pass to the token callback
/// - `p_text` - Raw pointer to UTF-8 encoded text
/// - `n_text` - Length of text in bytes
/// - `x_token` - Callback function to invoke for each token
///
/// # Returns
///
/// - `Ok(())` - All tokens processed successfully
/// - `Err(SQLITE_OK)` - Invalid UTF-8 input (treated as success to keep database accessible)
/// - `Err(SQLITE_INTERNAL)` - Tokenization failed
/// - `Err(code)` - Error code returned by the token callback
///
/// # Safety
///
/// This function performs unsafe operations:
/// - Dereferences raw pointers (`tokenizer`, `p_text`)
/// - Creates slices from raw pointer and length
///
/// The caller must ensure all pointers are valid and properly aligned.
///
/// # Error Handling
///
/// - **UTF-8 Errors**: Mapped to [`SQLITE_OK`] to prevent database inaccessibility
/// - **Tokenization Errors**: Return [`SQLITE_INTERNAL`]
/// - **Callback Errors**: Propagated immediately, stopping tokenization
///
/// # Token Callback Protocol
///
/// For each token, the callback is invoked with:
/// - `p_ctx` - Context pointer (unchanged)
/// - `0` - Flags (currently always 0)
/// - Token surface as C string pointer
/// - Token length in bytes
/// - Byte offset of token start in original text
/// - Byte offset of token end in original text
#[inline]
fn lindera_fts5_tokenize_internal(
    tokenizer: *mut Fts5Tokenizer,
    p_ctx: *mut c_void,
    p_text: *const c_char,
    n_text: c_int,
    x_token: TokenFunction,
) -> Result<(), c_int> {
    if n_text <= 0 {
        return Ok(());
    }

    let slice = unsafe { core::slice::from_raw_parts(p_text as *const c_uchar, n_text as usize) };

    // Map errors to SQLITE_OK because failing here means that the database
    // wouldn't accessible.
    let input = core::str::from_utf8(slice).map_err(|_| SQLITE_OK)?;

    match unsafe { (*tokenizer).tokenizer.tokenize(input) } {
        Ok(tokens) => {
            for token in tokens {
                let rc = x_token(
                    p_ctx,
                    0,
                    token.surface.as_bytes().as_ptr() as *const c_char,
                    token.surface.len() as c_int,
                    token.byte_start as c_int,
                    token.byte_end as c_int,
                );
                if rc != SQLITE_OK {
                    return Err(rc);
                }
            }
        }
        Err(_) => {
            return Err(SQLITE_INTERNAL);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "C" fn token_callback(
        ctx: *mut c_void,
        flags: c_int,
        token: *const c_char,
        token_len: c_int,
        start: c_int,
        end: c_int,
    ) -> c_int {
        assert_eq!(flags, 0);

        let tokens_ptr = ctx as *mut _ as *mut Vec<(String, c_int, c_int)>;
        let tokens = unsafe { tokens_ptr.as_mut() }.expect("tokens pointer");
        let slice =
            unsafe { core::slice::from_raw_parts(token as *const c_uchar, token_len as usize) };
        let token = String::from_utf8(slice.to_vec()).expect("Expected utf-8 token");

        tokens.push((token, start, end));

        return SQLITE_OK;
    }

    #[test]
    fn it_emits_segments() {
        let input = "Ｌｉｎｄｅｒａは形態素解析ｴﾝｼﾞﾝです。ユーザー辞書も利用可能です。";
        let mut tokens: Vec<(String, c_int, c_int)> = vec![];

        let mut tokenizer = Fts5Tokenizer {
            tokenizer: load_tokenizer().unwrap(),
        };
        lindera_fts5_tokenize_internal(
            &mut tokenizer,
            &mut tokens as *mut _ as *mut c_void,
            input.as_bytes().as_ptr() as *const c_char,
            input.len() as i32,
            token_callback,
        )
        .expect("tokenize internal should not fail");

        assert_eq!(
            tokens,
            [
                ("Lindera", 0, 21),
                ("形態素", 24, 33),
                ("解析", 33, 39),
                ("エンジン", 39, 54),
                ("ユーザ", 63, 75),
                ("辞書", 75, 81),
                ("利用", 84, 90),
                ("可能", 90, 96)
            ]
            .map(|(s, start, end)| (s.to_owned(), start, end))
        );
    }

    #[test]
    fn it_ignores_invalid_utf8() {
        let input = b"\xc3\x28";
        let mut tokens: Vec<(String, c_int, c_int)> = vec![];

        let mut tokenizer = Fts5Tokenizer {
            tokenizer: load_tokenizer().unwrap(),
        };
        assert_eq!(
            lindera_fts5_tokenize_internal(
                &mut tokenizer,
                &mut tokens as *mut _ as *mut c_void,
                input.as_ptr() as *const c_char,
                input.len() as i32,
                token_callback,
            )
            .expect_err("tokenize internal should not fail"),
            SQLITE_OK
        );

        assert_eq!(tokens, []);
    }
}
