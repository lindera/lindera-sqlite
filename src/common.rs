//! Common types and constants shared across the extension.
//!
//! This module defines the fundamental types and constants used for FFI communication
//! between Rust and SQLite's C API.

use libc::{c_char, c_int, c_void};

use lindera::tokenizer::Tokenizer;

// sqlite3.h

/// SQLite success status code.
///
/// Returned by functions to indicate successful completion.
/// Value: 0
pub const SQLITE_OK: c_int = 0;

/// SQLite internal error status code.
///
/// Indicates an internal error in SQLite or the extension.
/// Used when unexpected errors occur during tokenization or initialization.
/// Value: 2
pub const SQLITE_INTERNAL: c_int = 2;

/// SQLite misuse error status code.
///
/// Indicates the library is being used incorrectly.
/// Used when version requirements are not met.
/// Value: 21
pub const SQLITE_MISUSE: c_int = 21;

/// Wrapper for Lindera tokenizer used in FTS5.
///
/// This structure wraps the Lindera [`Tokenizer`] for use in the FTS5 tokenizer API.
/// Each FTS5 table using the Lindera tokenizer will have its own instance of this struct.
///
/// # Memory Management
///
/// Instances are heap-allocated in [`fts5_create_lindera_tokenizer`](crate::extension::fts5_create_lindera_tokenizer)
/// and deallocated in [`fts5_delete_lindera_tokenizer`](crate::extension::fts5_delete_lindera_tokenizer).
pub struct Fts5Tokenizer {
    /// The underlying Lindera tokenizer instance.
    pub tokenizer: Tokenizer,
}

/// Token callback function type.
///
/// This type represents the callback function provided by SQLite FTS5 for each token
/// produced during tokenization. The extension calls this function once per token.
///
/// # Parameters
///
/// - `p_ctx` - Context pointer passed through from the tokenization call
/// - `t_flags` - Token flags (currently always 0 in this implementation)
/// - `p_token` - Pointer to the token text (UTF-8 encoded)
/// - `n_token` - Length of the token in bytes
/// - `i_start` - Byte offset where the token starts in the original text
/// - `i_end` - Byte offset where the token ends in the original text
///
/// # Returns
///
/// - [`SQLITE_OK`] - Token processed successfully, continue tokenization
/// - Other codes - Error occurred, stop tokenization
///
/// # Example Flow
///
/// ```text
/// Input: "日本語" (9 bytes in UTF-8)
///
/// Callback 1: token="日本", n_token=6, i_start=0, i_end=6
/// Callback 2: token="語",   n_token=3, i_start=6, i_end=9
/// ```
pub type TokenFunction = extern "C" fn(
    p_ctx: *mut c_void,
    t_flags: c_int,
    p_token: *const c_char,
    n_token: c_int,
    i_start: c_int,
    i_end: c_int,
) -> c_int;
