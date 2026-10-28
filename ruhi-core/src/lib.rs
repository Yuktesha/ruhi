/// RuhI (如喜) Input Method Framework - Pure Rust Core
/// src/lib.rs - C-ABI 跨平台導出介面

pub mod fsm;
pub mod lexicon;
pub mod router;
pub mod templates;
pub mod symbols;
pub mod cangjie;
pub mod array30;
pub mod four_corner;
pub mod longkan;
pub mod japanese;
pub mod korean;
pub mod western;
pub mod reverse_coach;
pub mod entitlement;
pub mod engine;

pub use engine::{EngineResponse, RuhiEngine};
pub use entitlement::{
    EntitlementManager, EntitlementStatus, PricingTier, TIER2_TARGET_CHARS, TIER2_TARGET_DAYS,
    TRIAL_TARGET_CHARS, TRIAL_TARGET_DAYS,
};
pub use reverse_coach::{
    CharBreakdown, KeyStepBreakdown, KeyboardGeometry, PhraseBreakdown, ReverseLookupProvider,
    UNIVERSAL_REVERSE_PROVIDERS,
};
pub use symbols::SymbolsManager;
pub use cangjie::CangjieEngine;
pub use array30::Array30Engine;
pub use four_corner::FourCornerEngine;
pub use longkan::LongkanEngine;
pub use japanese::JapaneseEngine;
pub use korean::KoreanEngine;
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
pub extern "C" fn ruhi_engine_load_four_corner_tsv(engine: *mut RuhiEngine, path_cstr: *const c_char) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if engine.is_null() || path_cstr.is_null() {
            return -1;
        }
        let engine_ref = unsafe { &mut *engine };
        let c_str = unsafe { CStr::from_ptr(path_cstr) };
        if let Ok(path_str) = c_str.to_str() {
            let p = std::path::Path::new(path_str);
            match engine_ref.four_corner.load_tsv(p) {
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

/// 設定跨平台授權狀態檔路徑（例如 iOS / macOS App Group 共用容器路徑 `group.org.ruhos.ruhi/entitlement_state.json`）
#[no_mangle]
pub extern "C" fn ruhi_engine_set_entitlement_path(
    engine: *mut RuhiEngine,
    path_cstr: *const c_char,
) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if engine.is_null() || path_cstr.is_null() {
            return -1;
        }
        let engine_ref = unsafe { &mut *engine };
        let c_str = unsafe { CStr::from_ptr(path_cstr) };
        if let Ok(path_str) = c_str.to_str() {
            let p = std::path::Path::new(path_str);
            engine_ref.entitlement = crate::entitlement::EntitlementManager::load_or_init(p);
            engine_ref.longkan.grace_degraded = engine_ref.entitlement.is_trial_expired_both();
            0
        } else {
            -2
        }
    }))
    .unwrap_or(-99)
}

/// 取得當前九九圓滿（81 日 & 81,000 字）試用進度、三階段動態 SKU (`store_sku`) 與授權狀態 JSON
#[no_mangle]
pub extern "C" fn ruhi_engine_get_entitlement_json(engine: *mut RuhiEngine) -> *mut c_char {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if engine.is_null() {
            return std::ptr::null_mut();
        }
        let engine_ref = unsafe { &mut *engine };
        let ent = &engine_ref.entitlement;
        let tier = ent.current_pricing_tier();
        let payload = serde_json::json!({
            "is_licensed": ent.is_licensed(),
            "is_pro_unlocked": ent.is_pro_unlocked(),
            "is_trial_expired_both": ent.is_trial_expired_both(),
            "days_elapsed": ent.days_elapsed(),
            "target_days": crate::entitlement::TRIAL_TARGET_DAYS,
            "tier2_target_days": crate::entitlement::TIER2_TARGET_DAYS,
            "total_committed_chars": ent.total_committed_chars,
            "target_chars": crate::entitlement::TRIAL_TARGET_CHARS,
            "tier2_target_chars": crate::entitlement::TIER2_TARGET_CHARS,
            "store_sku": tier.store_sku(),
            "price_label": tier.price_label(),
            "tier_name": tier.tier_name(),
            "passcode": ent.license_passcode,
            "machine_fingerprint": crate::entitlement::EntitlementManager::machine_fingerprint(),
            "summary_badge": ent.summary_badge_text(),
        });
        if let Ok(c_string) = CString::new(payload.to_string()) {
            return c_string.into_raw();
        }
        std::ptr::null_mut()
    }))
    .unwrap_or(std::ptr::null_mut())
}

/// 驗證並啟用使用者輸入（或剪貼簿貼上）之跨平台終身通行碼，成功回傳 1，失敗回傳 0
#[no_mangle]
pub extern "C" fn ruhi_engine_activate_passcode(
    engine: *mut RuhiEngine,
    passcode_cstr: *const c_char,
) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if engine.is_null() || passcode_cstr.is_null() {
            return -1;
        }
        let engine_ref = unsafe { &mut *engine };
        let c_str = unsafe { CStr::from_ptr(passcode_cstr) };
        if let Ok(raw_text) = c_str.to_str() {
            let candidate = crate::entitlement::EntitlementManager::extract_passcode_from_text(raw_text)
                .unwrap_or_else(|| raw_text.trim().to_string());
            if engine_ref.entitlement.activate_passcode(&candidate) {
                engine_ref.longkan.grace_degraded = false;
                return 1;
            }
            0
        } else {
            -2
        }
    }))
    .unwrap_or(-99)
}

/// 供 iOS / macOS StoreKit 2 或 Google Play Billing 驗證購買成功後呼叫：
/// 傳入商店 `originalTransactionID` / `orderId`，決定性簽發並自動啟用 `RUHI-LK70-AAAA-BBBB-CCCC` 通行碼
#[no_mangle]
pub extern "C" fn ruhi_engine_issue_passcode_from_store_tx(
    engine: *mut RuhiEngine,
    tx_id_cstr: *const c_char,
) -> *mut c_char {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if engine.is_null() || tx_id_cstr.is_null() {
            return std::ptr::null_mut();
        }
        let engine_ref = unsafe { &mut *engine };
        let c_str = unsafe { CStr::from_ptr(tx_id_cstr) };
        if let Ok(tx_id) = c_str.to_str() {
            if tx_id.trim().is_empty() {
                return std::ptr::null_mut();
            }
            let passcode =
                crate::entitlement::EntitlementManager::derive_passcode_from_store_tx_id(tx_id);
            let _ = engine_ref.entitlement.activate_passcode(&passcode);
            engine_ref.longkan.grace_degraded = false;
            if let Ok(c_string) = CString::new(passcode) {
                return c_string.into_raw();
            }
        }
        std::ptr::null_mut()
    }))
    .unwrap_or(std::ptr::null_mut())
}
