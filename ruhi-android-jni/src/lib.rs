pub mod session;

use jni::objects::{JClass, JString};
use jni::sys::{jint, jlong, jstring};
use jni::JNIEnv;
use session::{AndroidConfig, AndroidEngineSession, AndroidEngineSnapshot};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Mutex;

type SessionHandle = Mutex<AndroidEngineSession>;

fn to_jstring(env: &mut JNIEnv<'_>, snapshot: &AndroidEngineSnapshot) -> jstring {
    match serde_json::to_string(snapshot) {
        Ok(json) => env
            .new_string(json)
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

fn with_session<F>(env: &mut JNIEnv<'_>, handle: jlong, f: F) -> jstring
where
    F: FnOnce(&mut AndroidEngineSession) -> AndroidEngineSnapshot,
{
    if handle == 0 {
        return std::ptr::null_mut();
    }
    let ptr = handle as *const SessionHandle;
    let res = catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*ptr };
        let mut guard = match mutex.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        f(&mut guard)
    }));
    match res {
        Ok(snapshot) => to_jstring(env, &snapshot),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_org_ruhos_ruhi_ime_nativebridge_RuhiNativeBridge_nativeCreate(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    data_dir: JString<'_>,
    config_json: JString<'_>,
) -> jlong {
    let dir_str: String = match env.get_string(&data_dir) {
        Ok(s) => s.into(),
        Err(_) => return 0,
    };
    let cfg_str: String = match env.get_string(&config_json) {
        Ok(s) => s.into(),
        Err(_) => "{}".to_string(),
    };

    let res = catch_unwind(AssertUnwindSafe(|| {
        let session = AndroidEngineSession::from_config_json(&dir_str, &cfg_str);
        let boxed: Box<SessionHandle> = Box::new(Mutex::new(session));
        Box::into_raw(boxed) as jlong
    }));
    res.unwrap_or(0)
}

#[no_mangle]
pub extern "system" fn Java_org_ruhos_ruhi_ime_nativebridge_RuhiNativeBridge_nativeDestroy(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
) {
    if handle == 0 {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let ptr = handle as *mut SessionHandle;
        unsafe {
            drop(Box::from_raw(ptr));
        }
    }));
}

#[no_mangle]
pub extern "system" fn Java_org_ruhos_ruhi_ime_nativebridge_RuhiNativeBridge_nativeApplyConfig(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    config_json: JString<'_>,
) -> jstring {
    let cfg_str: String = match env.get_string(&config_json) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let cfg = serde_json::from_str::<AndroidConfig>(&cfg_str).unwrap_or_default();
    with_session(&mut env, handle, |sess| sess.apply_config(cfg))
}

#[no_mangle]
pub extern "system" fn Java_org_ruhos_ruhi_ime_nativebridge_RuhiNativeBridge_nativeProcessKeyStr(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    key_str: JString<'_>,
) -> jstring {
    let key: String = match env.get_string(&key_str) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    with_session(&mut env, handle, |sess| sess.process_key_str(&key))
}

#[no_mangle]
pub extern "system" fn Java_org_ruhos_ruhi_ime_nativebridge_RuhiNativeBridge_nativeToggleMode(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jstring {
    with_session(&mut env, handle, |sess| sess.toggle_mode())
}

#[no_mangle]
pub extern "system" fn Java_org_ruhos_ruhi_ime_nativebridge_RuhiNativeBridge_nativeToggleFullwidth(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jstring {
    with_session(&mut env, handle, |sess| sess.toggle_fullwidth())
}

#[no_mangle]
pub extern "system" fn Java_org_ruhos_ruhi_ime_nativebridge_RuhiNativeBridge_nativeSelectCandidate(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    index: jint,
) -> jstring {
    if index < 0 {
        return std::ptr::null_mut();
    }
    with_session(&mut env, handle, |sess| sess.select_candidate(index as usize))
}

#[no_mangle]
pub extern "system" fn Java_org_ruhos_ruhi_ime_nativebridge_RuhiNativeBridge_nativeCommitAll(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jstring {
    with_session(&mut env, handle, |sess| sess.commit_all())
}

#[no_mangle]
pub extern "system" fn Java_org_ruhos_ruhi_ime_nativebridge_RuhiNativeBridge_nativeReset(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jstring {
    with_session(&mut env, handle, |sess| sess.reset())
}

#[no_mangle]
pub extern "system" fn Java_org_ruhos_ruhi_ime_nativebridge_RuhiNativeBridge_nativeGetState(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jstring {
    with_session(&mut env, handle, |sess| sess.get_state())
}
