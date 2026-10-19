/// RuhI (如喜) Input Method Framework - Pure Rust Core
/// src/lib.rs - C-ABI 跨平台導出介面

pub mod fsm;
pub mod lexicon;
pub mod router;
pub mod templates;
pub mod symbols;
pub mod cangjie;
pub mod array30;
pub mod japanese;
pub mod western;
pub mod engine;

pub use engine::{EngineResponse, RuhiEngine};
pub use symbols::SymbolsManager;
pub use cangjie::CangjieEngine;
pub use array30::Array30Engine;
pub use japanese::JapaneseEngine;
pub use western::WesternEngine;

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

#[no_mangle]
pub extern "C" fn ruhi_engine_create() -> *mut RuhiEngine {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Box::into_raw(Box::new(RuhiEngine::new()))
    })).unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn ruhi_engine_create_with_dir(path_cstr: *const c_char) -> *mut RuhiEngine {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if path_cstr.is_null() {
            return std::ptr::null_mut();
        }
        let c_str = unsafe { CStr::from_ptr(path_cstr) };
        if let Ok(path_str) = c_str.to_str() {
            Box::into_raw(Box::new(RuhiEngine::new_with_data_dir(path_str)))
        } else {
            std::ptr::null_mut()
        }
    })).unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn ruhi_engine_destroy(engine: *mut RuhiEngine) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if !engine.is_null() {
            unsafe {
                drop(Box::from_raw(engine));
            }
        }
    }));
}

#[no_mangle]
pub extern "C" fn ruhi_engine_load_tsv(engine: *mut RuhiEngine, path_cstr: *const c_char) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if engine.is_null() || path_cstr.is_null() {
            return -1;
        }
        let engine_ref = unsafe { &mut *engine };
        let c_str = unsafe { CStr::from_ptr(path_cstr) };
        if let Ok(path_str) = c_str.to_str() {
            match engine_ref.lexicon.load_tsv(path_str, "moe") {
                Ok(count) => count as i32,
                Err(_) => -2,
            }
        } else {
            -3
        }
    })).unwrap_or(-99)
}

#[no_mangle]
pub extern "C" fn ruhi_engine_load_symbols_tsv(engine: *mut RuhiEngine, path_cstr: *const c_char) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if engine.is_null() || path_cstr.is_null() {
            return -1;
        }
        let engine_ref = unsafe { &mut *engine };
        let c_str = unsafe { CStr::from_ptr(path_cstr) };
        if let Ok(path_str) = c_str.to_str() {
            match engine_ref.symbols.load_tsv(path_str) {
                Ok(count) => count as i32,
                Err(_) => -2,
            }
        } else {
            -3
        }
    })).unwrap_or(-99)
}

#[no_mangle]
pub extern "C" fn ruhi_engine_load_transitions_tsv(engine: *mut RuhiEngine, path_cstr: *const c_char) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if engine.is_null() || path_cstr.is_null() {
            return -1;
        }
        let engine_ref = unsafe { &mut *engine };
        let c_str = unsafe { CStr::from_ptr(path_cstr) };
        if let Ok(path_str) = c_str.to_str() {
            match engine_ref.transitions.load_tsv(path_str) {
                Ok(count) => count as i32,
                Err(_) => -2,
            }
        } else {
            -3
        }
    })).unwrap_or(-99)
}

#[no_mangle]
pub extern "C" fn ruhi_engine_load_cangjie_tsv(engine: *mut RuhiEngine, path_cstr: *const c_char) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if engine.is_null() || path_cstr.is_null() {
            return -1;
        }
        let engine_ref = unsafe { &mut *engine };
        let c_str = unsafe { CStr::from_ptr(path_cstr) };
        if let Ok(path_str) = c_str.to_str() {
            let p = std::path::Path::new(path_str);
            match engine_ref.cangjie.table.load_tsv(p) {
                Ok(count) => count as i32,
                Err(_) => -2,
            }
        } else {
            -3
        }
    })).unwrap_or(-99)
}

#[no_mangle]
pub extern "C" fn ruhi_engine_load_array30_tsv(engine: *mut RuhiEngine, path_cstr: *const c_char) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if engine.is_null() || path_cstr.is_null() {
            return -1;
        }
        let engine_ref = unsafe { &mut *engine };
        let c_str = unsafe { CStr::from_ptr(path_cstr) };
        if let Ok(path_str) = c_str.to_str() {
            let p = std::path::Path::new(path_str);
            match engine_ref.array30.table.load_tsv(p) {
                Ok(count) => count as i32,
                Err(_) => -2,
            }
        } else {
            -3
        }
    })).unwrap_or(-99)
}

#[no_mangle]
pub extern "C" fn ruhi_engine_set_keyboard_mode(engine: *mut RuhiEngine, mode_cstr: *const c_char) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if engine.is_null() || mode_cstr.is_null() {
            return -1;
        }
        let engine_ref = unsafe { &mut *engine };
        let c_str = unsafe { CStr::from_ptr(mode_cstr) };
        if let Ok(mode_str) = c_str.to_str() {
            engine_ref.set_keyboard_mode(mode_str);
            0
        } else {
            -2
        }
    })).unwrap_or(-99)
}

#[no_mangle]
pub extern "C" fn ruhi_engine_process_key(engine: *mut RuhiEngine, key_code: c_char) -> *mut c_char {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if engine.is_null() {
            return std::ptr::null_mut();
        }
        let engine_ref = unsafe { &mut *engine };
        let ch = key_code as u8 as char;
        let resp = engine_ref.process_key(ch);

        if let Ok(json_str) = serde_json::to_string(&resp) {
            if let Ok(c_string) = CString::new(json_str) {
                return c_string.into_raw();
            }
        }
        std::ptr::null_mut()
    })).unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn ruhi_engine_process_key_str(engine: *mut RuhiEngine, key_cstr: *const c_char) -> *mut c_char {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if engine.is_null() || key_cstr.is_null() {
            return std::ptr::null_mut();
        }
        let engine_ref = unsafe { &mut *engine };
        let c_str = unsafe { CStr::from_ptr(key_cstr) };
        if let Ok(key_str) = c_str.to_str() {
            let resp = engine_ref.process_key_str(key_str);
            if let Ok(json_str) = serde_json::to_string(&resp) {
                if let Ok(c_string) = CString::new(json_str) {
                    return c_string.into_raw();
                }
            }
        }
        std::ptr::null_mut()
    })).unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn ruhi_engine_reset(engine: *mut RuhiEngine) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if !engine.is_null() {
            let engine_ref = unsafe { &mut *engine };
            engine_ref.reset_buffer();
        }
    }));
}

#[no_mangle]
pub extern "C" fn ruhi_free_string(ptr: *mut c_char) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if !ptr.is_null() {
            unsafe {
                drop(CString::from_raw(ptr));
            }
        }
    }));
}
