extern crate alloc;

mod common;
#[cfg(feature = "extension")]
mod extension;

use libc::{c_char, c_int, c_uchar, c_void};

use lindera::tokenizer::{Tokenizer, TokenizerBuilder};

pub use crate::common::*;

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

fn lindera_fts5_tokenize_internal(
    tokenizer: *mut Fts5Tokenizer,
    p_ctx: *mut c_void,
    p_text: *const c_char,
    n_text: c_int,
    x_token: TokenFunction,
) -> Result<(), c_int> {
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
                    token.text.as_bytes().as_ptr() as *const c_char,
                    token.text.len() as c_int,
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
