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
/// Use the type provided by sqlite-loadable for ABI compatibility.
pub type Sqlite3 = sqlite_loadable::prelude::sqlite3;

/// Opaque SQLite prepared statement handle.
///
/// Use the type provided by sqlite3ext-sys for ABI compatibility.
type Sqlite3Stmt = sqlite3ext_sys::sqlite3_stmt;

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

/// SQLite extension API function table provided by SQLite.
///
/// Reuse the `sqlite-loadable` FFI definition instead of maintaining a massive
/// hand-written struct. Its layout matches SQLite's `sqlite3_api_routines` ABI,
/// so this alias is sufficient.
pub type Sqlite3APIRoutines = sqlite_loadable::prelude::sqlite3_api_routines;

struct SqliteApi<'a> {
    raw: &'a Sqlite3APIRoutines,
}

impl<'a> SqliteApi<'a> {
    unsafe fn new(p_api: *const c_void) -> Result<Self, c_int> {
        let raw =
            unsafe { (p_api as *const Sqlite3APIRoutines).as_ref() }.ok_or(SQLITE_INTERNAL)?;
        Ok(Self { raw })
    }

    fn ensure_supported_version(&self) -> Result<(), c_int> {
        let libversion_number = self.raw.libversion_number.ok_or(SQLITE_INTERNAL)?;

        if unsafe { libversion_number() } < 302000 {
            Err(SQLITE_MISUSE)
        } else {
            Ok(())
        }
    }

    fn prepare_statement(
        &'a self,
        db: *mut Sqlite3,
        sql: *const c_uchar,
    ) -> Result<PreparedStatement<'a>, c_int> {
        let mut stmt = null_mut::<Sqlite3Stmt>();
        let prepare = self.raw.prepare.ok_or(SQLITE_INTERNAL)?;
        let rc = unsafe { prepare(db, sql.cast::<c_char>(), -1, &mut stmt, null_mut()) };
        if rc != SQLITE_OK {
            return Err(rc);
        }
        Ok(PreparedStatement::new(self, stmt))
    }

    fn bind_fts5_pointer(
        &self,
        stmt: *mut Sqlite3Stmt,
        target: &mut *mut FTS5API,
    ) -> Result<(), c_int> {
        let bind_pointer = self.raw.bind_pointer.ok_or(SQLITE_INTERNAL)?;
        let rc = unsafe {
            bind_pointer(
                stmt,
                1,
                target.cast::<c_void>(),
                c"fts5_api_ptr".as_ptr() as *const c_char,
                None,
            )
        };
        if rc == SQLITE_OK { Ok(()) } else { Err(rc) }
    }

    fn step(&self, stmt: *mut Sqlite3Stmt) -> c_int {
        let step = match self.raw.step {
            Some(f) => f,
            None => return SQLITE_INTERNAL,
        };
        unsafe { step(stmt) }
    }

    fn finalize(&self, stmt: *mut Sqlite3Stmt) -> c_int {
        let finalize = match self.raw.finalize {
            Some(f) => f,
            None => return SQLITE_INTERNAL,
        };
        unsafe { finalize(stmt) }
    }
}

struct PreparedStatement<'api> {
    stmt: *mut Sqlite3Stmt,
    api: &'api SqliteApi<'api>,
    finalized: bool,
}

impl<'api> PreparedStatement<'api> {
    fn new(api: &'api SqliteApi<'api>, stmt: *mut Sqlite3Stmt) -> Self {
        Self {
            stmt,
            api,
            finalized: false,
        }
    }

    fn bind_fts5_pointer(&mut self, target: &mut *mut FTS5API) -> Result<(), c_int> {
        self.api.bind_fts5_pointer(self.stmt, target)
    }

    fn step(&mut self) {
        self.api.step(self.stmt);
    }

    fn finalize(mut self) -> Result<(), c_int> {
        let rc = self.api.finalize(self.stmt);
        self.finalized = true;
        if rc == SQLITE_OK { Ok(()) } else { Err(rc) }
    }
}

impl Drop for PreparedStatement<'_> {
    fn drop(&mut self) {
        if !self.finalized && !self.stmt.is_null() {
            if let Some(finalize) = self.api.raw.finalize {
                unsafe {
                    finalize(self.stmt);
                }
            }
        }
    }
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
    crate::common::ffi_panic_boundary(|| {
        lindera_fts_tokenizer_internal_init(db, p_api)?;
        Ok(())
    })
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
    let api = unsafe { SqliteApi::new(p_api)? };
    api.ensure_supported_version()?;

    let mut stmt = api.prepare_statement(db, c"SELECT fts5(?1)".as_ptr() as *const u8)?;
    let mut p_fts5_api = null_mut::<FTS5API>();
    stmt.bind_fts5_pointer(&mut p_fts5_api)?;
    stmt.step();
    stmt.finalize()?;

    let fts5_api = unsafe { p_fts5_api.as_ref() }.ok_or(SQLITE_INTERNAL)?;
    ensure_fts5_api_version(fts5_api)?;
    register_lindera_tokenizer(fts5_api);

    Ok(())
}

fn ensure_fts5_api_version(fts5_api: &FTS5API) -> Result<(), c_int> {
    if fts5_api.i_version == FTS5_API_VERSION {
        Ok(())
    } else {
        Err(SQLITE_MISUSE)
    }
}

fn register_lindera_tokenizer(fts5_api: &FTS5API) {
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
