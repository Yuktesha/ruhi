/// RuhI (如喜) Windows 11 TSF Native COM In-Process Text Service
pub mod guid;
pub mod registry;
pub mod edit_session;
pub mod service;
pub mod class_factory;
pub mod logger;
pub mod candidate_window;
pub mod display_attribute;
pub mod user_word_dialog;
use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows_core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;

use crate::guid::CLSID_RUHI_TEXT_SERVICE;
use crate::class_factory::RuhiClassFactory;

const DLL_PROCESS_ATTACH: u32 = 1;

static DLL_HINSTANCE: AtomicIsize = AtomicIsize::new(0);

#[no_mangle]
pub unsafe extern "system" fn DllMain(
    hinstance: HINSTANCE,
    dw_reason: u32,
    _reserved: *mut c_void,
) -> BOOL {
    if dw_reason == DLL_PROCESS_ATTACH {
        DLL_HINSTANCE.store(hinstance.0 as isize, Ordering::SeqCst);
        logger::log("DllMain: DLL_PROCESS_ATTACH");
    }
    BOOL::from(true)
}

#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    if rclsid.is_null() || riid.is_null() || ppv.is_null() {
        return E_INVALIDARG;
    }

    logger::log(&format!("DllGetClassObject: rclsid={:?}, riid={:?}", *rclsid, *riid));

    if *rclsid == CLSID_RUHI_TEXT_SERVICE {
        let factory: IClassFactory = RuhiClassFactory.into();
        let hr = factory.query(riid, ppv);
        logger::log(&format!("DllGetClassObject: factory.query returned {:?}", hr));
        hr
    } else {
        logger::log("DllGetClassObject: CLASS_E_CLASSNOTAVAILABLE");
        CLASS_E_CLASSNOTAVAILABLE
    }
}

#[no_mangle]
pub unsafe extern "system" fn DllCanUnloadNow() -> HRESULT {
    // 鐵律：TSF 輸入法 DLL 若回傳 S_OK，Windows COM / 宿主程序 (如 Explorer.EXE、SearchHost.exe)
    // 會在 ctfmon 重啟或空閒時透過 CoFreeUnusedLibraries 強制卸載 (FreeLibrary) 本 DLL，
    // 導致既有視窗鉤子或回呼指針在被調用時發生 0xc0000005 (ruhi_tsf.dll_unloaded) 嚴重崩潰！
    // 依據 Windows TSF 架構規範，回傳 S_FALSE 阻止隨意卸載，配合 NTFS 重命名熱更新機制達成零崩潰熱重載。
    S_FALSE
}

#[no_mangle]
pub unsafe extern "system" fn DllRegisterServer() -> HRESULT {
    let hinst = HINSTANCE(DLL_HINSTANCE.load(Ordering::SeqCst) as *mut c_void);
    match registry::register_server(hinst.into()) {
        Ok(_) => S_OK,
        Err(e) => e.code(),
    }
}

#[no_mangle]
pub unsafe extern "system" fn DllUnregisterServer() -> HRESULT {
    match registry::unregister_server() {
        Ok(_) => S_OK,
        Err(e) => e.code(),
    }
}
