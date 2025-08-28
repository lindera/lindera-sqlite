use core::ptr::null_mut;
use criterion::{Criterion, criterion_group, criterion_main};
use libc::{c_char, c_int, c_void};
use std::hint::black_box;

use lindera_sqlite::{Fts5Tokenizer, SQLITE_OK, lindera_fts5_tokenize, load_tokenizer};

extern "C" fn noop_callback(
    _ctx: *mut c_void,
    _flags: c_int,
    _token: *const c_char,
    _token_len: c_int,
    _start: c_int,
    _end: c_int,
) -> c_int {
    return SQLITE_OK;
}

#[inline(always)]
fn tokenize(tokenizer: &mut Fts5Tokenizer, input: &str) {
    lindera_fts5_tokenize(
        tokenizer,
        null_mut(),
        0,
        input.as_bytes().as_ptr() as *const c_char,
        input.len() as i32,
        noop_callback,
    );
}

fn fts5_benchmark(c: &mut Criterion) {
    // Initialize tokenizer once before benchmarking
    let mut tokenizer = Fts5Tokenizer {
        tokenizer: load_tokenizer().expect("Failed to load tokenizer"),
    };

    let latin_lower_60kb = "hello ".repeat(10 * 1024);
    let latin_upper_60kb = "HELLO ".repeat(10 * 1024);
    let diacritics_60kb = "öplö ".repeat(10 * 1024);
    let cjk_60kb = "你好".repeat(10 * 1024);

    c.bench_function("tokenize latin lowercase 60kb", |b| {
        return b.iter(|| tokenize(&mut tokenizer, black_box(&latin_lower_60kb)));
    });

    c.bench_function("tokenize latin uppercase 60kb", |b| {
        return b.iter(|| tokenize(&mut tokenizer, black_box(&latin_upper_60kb)));
    });

    c.bench_function("tokenize diacritics 60kb", |b| {
        return b.iter(|| tokenize(&mut tokenizer, black_box(&diacritics_60kb)));
    });

    c.bench_function("tokenize cjk 60kb", |b| {
        return b.iter(|| tokenize(&mut tokenizer, black_box(&cjk_60kb)));
    });
}

criterion_group!(benches, fts5_benchmark);
criterion_main!(benches);
