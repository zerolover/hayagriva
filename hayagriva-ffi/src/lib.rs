//! C ABI over hayagriva for citation formatting (see
//! `include/hayagriva.h` and `docs/ffi-design.md`).
//!
//! This does not guard against Rust panics unwinding across the boundary
//! (no `catch_unwind`): a bug that panics here uses this crate's own
//! release profile, `panic = "abort"`, so it aborts the host process
//! rather than surfacing as a recoverable error.

mod ctx;
mod json;

use std::ffi::{CStr, CString, c_char};
use std::ptr;

use ctx::HayagrivaCtx;

unsafe fn cstr_to_str<'a>(ptr: *const c_char) -> Result<&'a str, String> {
    if ptr.is_null() {
        return Err("unexpected null string argument".to_string());
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|_| "argument is not valid UTF-8".to_string())
}

fn string_to_c(s: String) -> *mut c_char {
    match CString::new(s) {
        Ok(cstr) => cstr.into_raw(),
        // Only reachable if the payload contains an embedded NUL byte, which
        // real bibliography/citation text should never do.
        Err(_) => ptr::null_mut(),
    }
}

fn guard<T>(out_error: *mut *mut c_char, default: T, result: Result<T, String>) -> T {
    match result {
        Ok(value) => value,
        Err(message) => {
            if !out_error.is_null() {
                unsafe { *out_error = string_to_c(message) };
            }
            default
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn hayagriva_new(_out_error: *mut *mut c_char) -> *mut HayagrivaCtx {
    Box::into_raw(Box::new(HayagrivaCtx::new()))
}

#[unsafe(no_mangle)]
pub extern "C" fn hayagriva_free(ctx: *mut HayagrivaCtx) {
    if ctx.is_null() {
        return;
    }
    unsafe { drop(Box::from_raw(ctx)) };
}

#[unsafe(no_mangle)]
pub extern "C" fn hayagriva_set_bib(
    ctx: *mut HayagrivaCtx,
    bib_str: *const c_char,
    out_error: *mut *mut c_char,
) -> bool {
    let result = (|| {
        let ctx = unsafe { ctx.as_mut() }.ok_or("null context")?;
        let bib_str = unsafe { cstr_to_str(bib_str) }?;
        ctx.set_bib(bib_str)?;
        Ok(true)
    })();
    guard(out_error, false, result)
}

#[unsafe(no_mangle)]
pub extern "C" fn hayagriva_set_style(
    ctx: *mut HayagrivaCtx,
    style_name: *const c_char,
    out_error: *mut *mut c_char,
) -> bool {
    let result = (|| {
        let ctx = unsafe { ctx.as_mut() }.ok_or("null context")?;
        let style_name = unsafe { cstr_to_str(style_name) }?;
        ctx.set_style(style_name)?;
        Ok(true)
    })();
    guard(out_error, false, result)
}

#[unsafe(no_mangle)]
pub extern "C" fn hayagriva_set_locale(
    ctx: *mut HayagrivaCtx,
    locale: *const c_char,
    out_error: *mut *mut c_char,
) -> bool {
    let result = (|| {
        let ctx = unsafe { ctx.as_mut() }.ok_or("null context")?;
        let locale =
            if locale.is_null() { None } else { Some(unsafe { cstr_to_str(locale) }?) };
        ctx.set_locale(locale)?;
        Ok(true)
    })();
    guard(out_error, false, result)
}

#[unsafe(no_mangle)]
pub extern "C" fn hayagriva_list_styles() -> *mut c_char {
    string_to_c(ctx::list_styles())
}

#[unsafe(no_mangle)]
pub extern "C" fn hayagriva_list_locales() -> *mut c_char {
    string_to_c(ctx::list_locales())
}

#[unsafe(no_mangle)]
pub extern "C" fn hayagriva_list_entries(ctx: *const HayagrivaCtx) -> *mut c_char {
    let Some(ctx) = (unsafe { ctx.as_ref() }) else {
        return ptr::null_mut();
    };
    match ctx.list_entries() {
        Ok(json) => string_to_c(json),
        Err(_) => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn hayagriva_render(
    ctx: *const HayagrivaCtx,
    citation_groups_json: *const c_char,
    out_error: *mut *mut c_char,
) -> *mut c_char {
    let result = (|| {
        let ctx = unsafe { ctx.as_ref() }.ok_or("null context")?;
        let citation_groups_json = unsafe { cstr_to_str(citation_groups_json) }?;
        ctx.render(citation_groups_json).map(string_to_c)
    })();
    guard(out_error, ptr::null_mut(), result)
}

#[unsafe(no_mangle)]
pub extern "C" fn hayagriva_free_string(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    unsafe { drop(CString::from_raw(s)) };
}
