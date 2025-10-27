//! SQLite FTS5 extension integration.
//!
//! This module provides the C ABI implementation for integrating Lindera tokenizer
//! as a SQLite FTS5 extension. It handles the low-level communication between SQLite's
//! extension system and the Lindera tokenizer.
//!
//! # Architecture
//!
//! The extension follows SQLite's FTS5 tokenizer API specification:
//!
//! 1. **Initialization** ([`lindera_fts5_tokenizer_init`]): Entry point when loading the extension
//! 2. **Tokenizer Creation** ([`fts5_create_lindera_tokenizer`]): Creates a new tokenizer instance
//! 3. **Tokenization** (in `lib.rs`): Processes text and produces tokens
//! 4. **Cleanup** ([`fts5_delete_lindera_tokenizer`], [`fts5_destroy_icu_module`]): Resource deallocation
//!
//! # Safety
//!
//! This module contains extensive unsafe code to interface with SQLite's C API.
//! All public functions follow FFI safety conventions and use panic catching to
//! prevent unwinding across the FFI boundary.

use core::ptr::null_mut;
use libc::{c_char, c_int, c_uchar, c_void};

use crate::common::*;
use crate::lindera_fts5_tokenize;
use crate::load_tokenizer;

/// FTS5 API version supported by this extension.
///
/// This constant indicates compatibility with SQLite FTS5 API version 2.
/// The extension will verify this version matches at runtime during initialization.
pub const FTS5_API_VERSION: c_int = 2;

/// Opaque SQLite database handle.
///
/// This represents a SQLite database connection used in FFI calls.
/// It's an opaque type - the actual structure is defined in SQLite's C code.
pub struct Sqlite3 {}

/// Opaque SQLite prepared statement handle.
///
/// Represents a compiled SQL statement. Used internally for querying the FTS5 API pointer.
struct Sqlite3Stmt {}

/// FTS5 tokenizer API structure.
///
/// Defines the function pointers that implement a custom FTS5 tokenizer.
/// This matches the structure defined in SQLite's `fts5.h` header file.
///
/// # Fields
///
/// - `x_create` - Creates a new tokenizer instance
/// - `x_delete` - Deletes a tokenizer instance
/// - `x_tokenize` - Tokenizes input text
// fts5.h
#[repr(C)]
struct Fts5TokenizerApi {
    x_create: extern "C" fn(
        p_context: *mut c_void,
        az_arg: *const *const c_uchar,
        n_arg: c_int,
        fts5_tokenizer: *mut *mut Fts5Tokenizer,
    ) -> c_int,
    x_delete: extern "C" fn(fts5_tokenizer: *mut Fts5Tokenizer),
    x_tokenize: extern "C" fn(
        tokenizer: *mut Fts5Tokenizer,
        p_ctx: *mut c_void,
        flags: c_int,
        p_text: *const c_char,
        n_text: c_int,
        x_token: TokenFunction,
    ) -> c_int,
}

/// FTS5 API structure.
///
/// Provides access to FTS5 functionality, primarily for registering custom tokenizers.
/// This structure is obtained from SQLite at runtime via a special query.
///
/// # Fields
///
/// - `i_version` - API version number (must be 2)
/// - `x_create_tokenizer` - Function to register a new tokenizer with FTS5
#[repr(C)]
struct FTS5API {
    i_version: c_int, // Currently always set to 2

    /* Create a new tokenizer */
    x_create_tokenizer: extern "C" fn(
        fts5_api: *const FTS5API,
        z_name: *const c_uchar,
        p_context: *mut c_void,
        fts5_tokenizer: *mut Fts5TokenizerApi,
        x_destroy: extern "C" fn(module: *mut c_void),
    ) -> c_int,
}

/// SQLite extension API function table.
///
/// This massive structure contains function pointers to all SQLite C API functions
/// available to extensions. It's defined in `sqlite3ext.h` and provided by SQLite
/// when the extension is loaded.
///
/// Only a subset of these functions are actually used by this extension:
/// - `prepare` - Compiles SQL statements
/// - `step` - Executes prepared statements
/// - `finalize` - Frees prepared statements
/// - `bind_pointer` - Binds pointer values (used to retrieve FTS5 API)
/// - `libversion_number` - Gets SQLite version number
///
/// Most fields are prefixed with `_` as they're unused but required for struct layout.
// sqlite3ext.h
#[repr(C)]
pub struct Sqlite3APIRoutines {
    _aggregate_context: extern "C" fn(),
    _aggregate_count: extern "C" fn(),
    _bind_blob: extern "C" fn(),
    _bind_double: extern "C" fn(),
    _bind_int: extern "C" fn(),
    _bind_int64: extern "C" fn(),
    _bind_null: extern "C" fn(),
    _bind_parameter_count: extern "C" fn(),
    _bind_parameter_index: extern "C" fn(),
    _bind_parameter_name: extern "C" fn(),
    _bind_text: extern "C" fn(),
    _bind_text16: extern "C" fn(),
    _bind_value: extern "C" fn(),
    _busy_handler: extern "C" fn(),
    _busy_timeout: extern "C" fn(),
    _changes: extern "C" fn(),
    _close: extern "C" fn(),
    _collation_needed: extern "C" fn(),
    _collation_needed16: extern "C" fn(),
    _column_blob: extern "C" fn(),
    _column_bytes: extern "C" fn(),
    _column_bytes16: extern "C" fn(),
    _column_count: extern "C" fn(),
    _column_database_name: extern "C" fn(),
    _column_database_name16: extern "C" fn(),
    _column_decltype: extern "C" fn(),
    _column_decltype16: extern "C" fn(),
    _column_double: extern "C" fn(),
    _column_int: extern "C" fn(),
    _column_int64: extern "C" fn(),
    _column_name: extern "C" fn(),
    _column_name16: extern "C" fn(),
    _column_origin_name: extern "C" fn(),
    _column_origin_name16: extern "C" fn(),
    _column_table_name: extern "C" fn(),
    _column_table_name16: extern "C" fn(),
    _column_text: extern "C" fn(),
    _column_text16: extern "C" fn(),
    _column_type: extern "C" fn(),
    _column_value: extern "C" fn(),
    _commit_hook: extern "C" fn(),
    _complete: extern "C" fn(),
    _complete16: extern "C" fn(),
    _create_collation: extern "C" fn(),
    _create_collation16: extern "C" fn(),
    _create_function: extern "C" fn(),
    _create_function16: extern "C" fn(),
    _create_module: extern "C" fn(),
    _data_count: extern "C" fn(),
    _db_handle: extern "C" fn(),
    _declare_vtab: extern "C" fn(),
    _enable_shared_cache: extern "C" fn(),
    _errcode: extern "C" fn(),
    _errmsg: extern "C" fn(),
    _errmsg16: extern "C" fn(),
    _exec: extern "C" fn(),
    _expired: extern "C" fn(),
    finalize: extern "C" fn(stmt: *mut Sqlite3Stmt) -> c_int,
    _free: extern "C" fn(),
    _free_table: extern "C" fn(),
    _get_autocommit: extern "C" fn(),
    _get_auxdata: extern "C" fn(),
    _get_table: extern "C" fn(),
    _global_recover: extern "C" fn(),
    _interruptx: extern "C" fn(),
    _last_insert_rowid: extern "C" fn(),
    _libversion: extern "C" fn(),
    libversion_number: extern "C" fn() -> c_int,
    _malloc: extern "C" fn(),
    _mprintf: extern "C" fn(),
    _open: extern "C" fn(),
    _open16: extern "C" fn(),
    prepare: extern "C" fn(
        db: *mut Sqlite3,
        query: *const c_uchar,
        query_len: c_int,
        stmt: *mut *mut Sqlite3Stmt,
        pz_tail: *mut *mut c_uchar,
    ) -> c_int,
    _prepare16: extern "C" fn(),
    _profile: extern "C" fn(),
    _progress_handler: extern "C" fn(),
    _realloc: extern "C" fn(),
    _reset: extern "C" fn(),
    _result_blob: extern "C" fn(),
    _result_double: extern "C" fn(),
    _result_error: extern "C" fn(),
    _result_error16: extern "C" fn(),
    _result_int: extern "C" fn(),
    _result_int64: extern "C" fn(),
    _result_null: extern "C" fn(),
    _result_text: extern "C" fn(),
    _result_text16: extern "C" fn(),
    _result_text16be: extern "C" fn(),
    _result_text16le: extern "C" fn(),
    _result_value: extern "C" fn(),
    _rollback_hook: extern "C" fn(),
    _set_authorizer: extern "C" fn(),
    _set_auxdata: extern "C" fn(),
    _xsnprintf: extern "C" fn(),
    step: extern "C" fn(stmt: *mut Sqlite3Stmt) -> c_int,
    _table_column_metadata: extern "C" fn(),
    _thread_cleanup: extern "C" fn(),
    _total_changes: extern "C" fn(),
    _trace: extern "C" fn(),
    _transfer_bindings: extern "C" fn(),
    _update_hook: extern "C" fn(),
    _user_data: extern "C" fn(),
    _value_blob: extern "C" fn(),
    _value_bytes: extern "C" fn(),
    _value_bytes16: extern "C" fn(),
    _value_double: extern "C" fn(),
    _value_int: extern "C" fn(),
    _value_int64: extern "C" fn(),
    _value_numeric_type: extern "C" fn(),
    _value_text: extern "C" fn(),
    _value_text16: extern "C" fn(),
    _value_text16be: extern "C" fn(),
    _value_text16le: extern "C" fn(),
    _value_type: extern "C" fn(),
    _vmprintf: extern "C" fn(),
    /* Added ??? */
    _overload_function: extern "C" fn(),
    /* Added by 3.3.13 */
    _prepare_v2: extern "C" fn(),
    _prepare16_v2: extern "C" fn(),
    _clear_bindings: extern "C" fn(),
    /* Added by 3.4.1 */
    _create_module_v2: extern "C" fn(),
    /* Added by 3.5.0 */
    _bind_zeroblob: extern "C" fn(),
    _blob_bytes: extern "C" fn(),
    _blob_close: extern "C" fn(),
    _blob_open: extern "C" fn(),
    _blob_read: extern "C" fn(),
    _blob_write: extern "C" fn(),
    _create_collation_v2: extern "C" fn(),
    _file_control: extern "C" fn(),
    _memory_highwater: extern "C" fn(),
    _memory_used: extern "C" fn(),
    _mutex_alloc: extern "C" fn(),
    _mutex_enter: extern "C" fn(),
    _mutex_free: extern "C" fn(),
    _mutex_leave: extern "C" fn(),
    _mutex_try: extern "C" fn(),
    _open_v2: extern "C" fn(),
    _release_memory: extern "C" fn(),
    _result_error_nomem: extern "C" fn(),
    _result_error_toobig: extern "C" fn(),
    _sleep: extern "C" fn(),
    _soft_heap_limit: extern "C" fn(),
    _vfs_find: extern "C" fn(),
    _vfs_register: extern "C" fn(),
    _vfs_unregister: extern "C" fn(),
    _xthreadsafe: extern "C" fn(),
    _result_zeroblob: extern "C" fn(),
    _result_error_code: extern "C" fn(),
    _test_control: extern "C" fn(),
    _randomness: extern "C" fn(),
    _context_db_handle: extern "C" fn(),
    _extended_result_codes: extern "C" fn(),
    _limit: extern "C" fn(),
    _next_stmt: extern "C" fn(),
    _sql: extern "C" fn(),
    _status: extern "C" fn(),
    _backup_finish: extern "C" fn(),
    _backup_init: extern "C" fn(),
    _backup_pagecount: extern "C" fn(),
    _backup_remaining: extern "C" fn(),
    _backup_step: extern "C" fn(),
    _compileoption_get: extern "C" fn(),
    _compileoption_used: extern "C" fn(),
    _create_function_v2: extern "C" fn(),
    _db_config: extern "C" fn(),
    _db_mutex: extern "C" fn(),
    _db_status: extern "C" fn(),
    _extended_errcode: extern "C" fn(),
    _log: extern "C" fn(),
    _soft_heap_limit64: extern "C" fn(),
    _sourceid: extern "C" fn(),
    _stmt_status: extern "C" fn(),
    _strnicmp: extern "C" fn(),
    _unlock_notify: extern "C" fn(),
    _wal_autocheckpoint: extern "C" fn(),
    _wal_checkpoint: extern "C" fn(),
    _wal_hook: extern "C" fn(),
    _blob_reopen: extern "C" fn(),
    _vtab_config: extern "C" fn(),
    _vtab_on_conflict: extern "C" fn(),
    /* Version 3.7.16 and later */
    _close_v2: extern "C" fn(),
    _db_filename: extern "C" fn(),
    _db_readonly: extern "C" fn(),
    _db_release_memory: extern "C" fn(),
    _errstr: extern "C" fn(),
    _stmt_busy: extern "C" fn(),
    _stmt_readonly: extern "C" fn(),
    _stricmp: extern "C" fn(),
    _uri_boolean: extern "C" fn(),
    _uri_int64: extern "C" fn(),
    _uri_parameter: extern "C" fn(),
    _xvsnprintf: extern "C" fn(),
    _wal_checkpoint_v2: extern "C" fn(),
    /* Version 3.8.7 and later */
    _auto_extension: extern "C" fn(),
    _bind_blob64: extern "C" fn(),
    _bind_text64: extern "C" fn(),
    _cancel_auto_extension: extern "C" fn(),
    _load_extension: extern "C" fn(),
    _malloc64: extern "C" fn(),
    _msize: extern "C" fn(),
    _realloc64: extern "C" fn(),
    _reset_auto_extension: extern "C" fn(),
    _result_blob64: extern "C" fn(),
    _result_text64: extern "C" fn(),
    _strglob: extern "C" fn(),
    /* Version 3.8.11 and later */
    _value_dup: extern "C" fn(),
    _value_free: extern "C" fn(),
    _result_zeroblob64: extern "C" fn(),
    _bind_zeroblob64: extern "C" fn(),
    /* Version 3.9.0 and later */
    _value_subtype: extern "C" fn(),
    _result_subtype: extern "C" fn(),
    /* Version 3.10.0 and later */
    _status64: extern "C" fn(),
    _strlike: extern "C" fn(),
    _db_cacheflush: extern "C" fn(),
    /* Version 3.12.0 and later */
    _system_errno: extern "C" fn(),
    /* Version 3.14.0 and later */
    _trace_v2: extern "C" fn(),
    _expanded_sql: extern "C" fn(),
    /* Version 3.18.0 and later */
    _set_last_insert_rowid: extern "C" fn(),
    /* Version 3.20.0 and later */
    _prepare_v3: extern "C" fn(),
    _prepare16_v3: extern "C" fn(),
    bind_pointer: extern "C" fn(
        stmt: *mut Sqlite3Stmt,
        index: c_int,
        ptr: *mut *mut FTS5API,
        name: *const c_uchar,
        cb: *mut c_void,
    ) -> c_int,
}

/// Extension initialization entry point.
///
/// This is the main entry point called by SQLite when loading the extension via
/// `.load` command or `sqlite3_load_extension()`. It registers the Lindera tokenizer
/// with the FTS5 subsystem.
///
/// # Parameters
///
/// - `db` - SQLite database handle
/// - `_pz_err_msg` - Pointer to error message string (unused)
/// - `p_api` - Pointer to [`Sqlite3APIRoutines`] function table
///
/// # Returns
///
/// - [`SQLITE_OK`] - Extension loaded successfully
/// - [`SQLITE_INTERNAL`] - Internal error or panic occurred
/// - [`SQLITE_MISUSE`] - SQLite version too old or API version mismatch
///
/// # Safety
///
/// This function is marked `unsafe(no_mangle)` and `extern "C"` for FFI compatibility.
/// It catches panics to prevent unwinding across the FFI boundary.
///
/// # C API Usage
///
/// ```c
/// // In SQLite
/// .load ./liblindera_sqlite lindera_fts5_tokenizer_init
/// ```
///
/// Or programmatically:
///
/// ```c
/// sqlite3_load_extension(db, "./liblindera_sqlite.so",
///                       "lindera_fts5_tokenizer_init", &err);
/// ```
#[unsafe(no_mangle)]
pub extern "C" fn lindera_fts5_tokenizer_init(
    db: *mut Sqlite3,
    _pz_err_msg: *mut *mut c_uchar,
    p_api: *const c_void,
) -> c_int {
    std::panic::catch_unwind(|| match lindera_fts_tokenizer_internal_init(db, p_api) {
        Ok(_) => SQLITE_OK,
        Err(code) => code,
    })
    .unwrap_or(SQLITE_INTERNAL)
}

/// Internal initialization implementation.
///
/// Performs the actual initialization steps:
/// 1. Validates SQLite version (requires >= 3.20.0)
/// 2. Retrieves FTS5 API pointer via SQL query
/// 3. Validates FTS5 API version
/// 4. Registers the Lindera tokenizer with FTS5
///
/// # Parameters
///
/// - `db` - SQLite database handle
/// - `p_api` - Pointer to SQLite API function table
///
/// # Returns
///
/// - `Ok(())` - Initialization successful
/// - `Err(SQLITE_INTERNAL)` - Null pointer or internal error
/// - `Err(SQLITE_MISUSE)` - Version mismatch
/// - `Err(code)` - SQLite error code from API calls
///
/// # Implementation Details
///
/// Uses a special SQLite query `SELECT fts5(?1)` with a bound pointer to retrieve
/// the FTS5 API structure. This is the standard method for extensions to access FTS5.
fn lindera_fts_tokenizer_internal_init(
    db: *mut Sqlite3,
    p_api: *const c_void,
) -> Result<(), c_int> {
    let api = unsafe { (p_api as *const Sqlite3APIRoutines).as_ref() }.ok_or(SQLITE_INTERNAL)?;

    if (api.libversion_number)() < 302000 {
        return Err(SQLITE_MISUSE);
    }

    let mut stmt = null_mut::<Sqlite3Stmt>();
    let rc = (api.prepare)(
        db,
        c"SELECT fts5(?1)".as_ptr() as *const u8,
        -1,
        &mut stmt,
        null_mut(),
    );

    if rc != SQLITE_OK {
        return Err(rc);
    }

    let mut p_fts5_api = null_mut::<FTS5API>();
    let rc = (api.bind_pointer)(
        stmt,
        1,
        &mut p_fts5_api,
        c"fts5_api_ptr".as_ptr() as *const u8,
        null_mut(),
    );
    if rc != SQLITE_OK {
        (api.finalize)(stmt);
        return Err(rc);
    }

    // Intentionally ignore return value, sqlite3 returns SQLITE_ROW
    (api.step)(stmt);

    let rc = (api.finalize)(stmt);
    if rc != SQLITE_OK {
        return Err(rc);
    }

    let fts5_api = unsafe { p_fts5_api.as_ref() }.ok_or(SQLITE_INTERNAL)?;

    if fts5_api.i_version != FTS5_API_VERSION {
        return Err(SQLITE_MISUSE);
    }

    // Add custom tokenizer
    let mut tokenizer = Fts5TokenizerApi {
        x_create: fts5_create_lindera_tokenizer,
        x_delete: fts5_delete_lindera_tokenizer,
        x_tokenize: lindera_fts5_tokenize,
    };

    (fts5_api.x_create_tokenizer)(
        fts5_api,
        c"lindera_tokenizer".as_ptr() as *const u8,
        null_mut(),
        &mut tokenizer,
        fts5_destroy_icu_module,
    );

    Ok(())
}

/// Creates a new Lindera tokenizer instance.
///
/// Called by SQLite FTS5 when creating a table with `tokenize='lindera_tokenizer'`.
/// Allocates and initializes a new [`Fts5Tokenizer`] instance.
///
/// # Parameters
///
/// - `_p_context` - Context pointer (unused)
/// - `_az_arg` - Tokenizer arguments array (unused - configuration comes from environment)
/// - `_n_arg` - Number of arguments (unused)
/// - `fts5_tokenizer` - Output pointer to receive the new tokenizer instance
///
/// # Returns
///
/// - [`SQLITE_OK`] - Tokenizer created successfully
/// - [`SQLITE_INTERNAL`] - Failed to load tokenizer (e.g., missing configuration)
///
/// # Memory Management
///
/// The tokenizer is allocated on the heap using `Box` and converted to a raw pointer.
/// It will be freed later by [`fts5_delete_lindera_tokenizer`].
///
/// # Safety
///
/// Writes to the raw pointer `fts5_tokenizer`. The caller (SQLite) must ensure
/// the pointer is valid and properly aligned.
#[unsafe(no_mangle)]
pub extern "C" fn fts5_create_lindera_tokenizer(
    _p_context: *mut c_void,
    _az_arg: *const *const c_uchar,
    _n_arg: c_int,
    fts5_tokenizer: *mut *mut Fts5Tokenizer,
) -> c_int {
    let tokenizer = match load_tokenizer() {
        Ok(tokenizer) => Box::new(Fts5Tokenizer { tokenizer }),
        Err(_) => return SQLITE_INTERNAL,
    };
    unsafe {
        *fts5_tokenizer = Box::into_raw(tokenizer);
    }

    SQLITE_OK
}

/// Deletes a Lindera tokenizer instance.
///
/// Called by SQLite FTS5 when dropping a table or closing the database.
/// Properly deallocates the tokenizer instance created by [`fts5_create_lindera_tokenizer`].
///
/// # Parameters
///
/// - `fts5_tokenizer` - Pointer to the tokenizer instance to delete
///
/// # Safety
///
/// This function reconstructs a `Box` from the raw pointer and drops it, which
/// deallocates the memory. The pointer must be:
/// - Previously created by [`fts5_create_lindera_tokenizer`]
/// - Not already freed
/// - Not used after this call
#[unsafe(no_mangle)]
pub extern "C" fn fts5_delete_lindera_tokenizer(fts5_tokenizer: *mut Fts5Tokenizer) {
    let tokenizer = unsafe { Box::from_raw(fts5_tokenizer) };
    drop(tokenizer);
}

/// Module destruction callback (no-op).
///
/// Called by FTS5 when unregistering the tokenizer module. Since this extension
/// has no module-level state to clean up, this function does nothing.
///
/// # Parameters
///
/// - `_module` - Module context pointer (unused)
///
/// # Note
///
/// The function name references "icu" for historical reasons but applies to
/// the Lindera tokenizer module.
#[unsafe(no_mangle)]
pub extern "C" fn fts5_destroy_icu_module(_module: *mut c_void) {
    // no-op
}
