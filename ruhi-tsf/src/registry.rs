/// RuhI (如喜) Windows TSF COM & Category Registration
use windows_core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::*;
use windows::Win32::UI::TextServices::{
    CLSID_TF_CategoryMgr, CLSID_TF_InputProcessorProfiles, ITfCategoryMgr, ITfInputProcessorProfiles,
};

use crate::guid::*;

const DESCRIPTION_ZH: &str = "RuhI如喜通用智慧型輸入法";
const DESCRIPTION_EN: &str = "RuhI Universal Smart IME";
const LANG_CHINESE_TRADITIONAL: u16 = 0x0404; // zh-TW
const LANG_ENGLISH_US: u16 = 0x0409;          // en-US

extern "system" {
    fn GetUserDefaultUILanguage() -> u16;
}

fn get_current_ui_desc() -> &'static str {
    let lang = unsafe { GetUserDefaultUILanguage() };
    match lang {
        0x0404 | 0x0c04 | 0x1404 => DESCRIPTION_ZH,
        _ => DESCRIPTION_EN,
    }
}

pub fn get_module_path(hinstance: HMODULE) -> String {
    let mut buf = vec![0u16; 1024];
    let len = unsafe { GetModuleFileNameW(hinstance, &mut buf) };
    if len > 0 {
        String::from_utf16_lossy(&buf[..len as usize])
    } else {
        String::new()
    }
}

pub unsafe fn register_server(hinstance: HMODULE) -> Result<()> {
    let dll_path = get_module_path(hinstance);
    let clsid_str = format!("{{{:?}}}", CLSID_RUHI_TEXT_SERVICE);
    let clsid_key_path = format!("CLSID\\{}", clsid_str);

    // 1. Write HKCR\CLSID\{CLSID}
    let mut clsid_key = HKEY::default();
    let wide_key: Vec<u16> = clsid_key_path.encode_utf16().chain(Some(0)).collect();
    RegCreateKeyW(
        HKEY_CLASSES_ROOT,
        PCWSTR(wide_key.as_ptr()),
        &mut clsid_key,
    ).ok()?;

    let current_desc = get_current_ui_desc();
    let desc_wide: Vec<u16> = current_desc.encode_utf16().chain(Some(0)).collect();
    RegSetValueExW(
        clsid_key,
        PCWSTR::null(),
        0,
        REG_SZ,
        Some(std::slice::from_raw_parts(desc_wide.as_ptr() as *const u8, desc_wide.len() * 2)),
    ).ok()?;

    // 2. Write HKCR\CLSID\{CLSID}\InprocServer32
    let mut inproc_key = HKEY::default();
    let inproc_path: Vec<u16> = "InprocServer32".encode_utf16().chain(Some(0)).collect();
    RegCreateKeyW(
        clsid_key,
        PCWSTR(inproc_path.as_ptr()),
        &mut inproc_key,
    ).ok()?;

    let dll_path_wide: Vec<u16> = dll_path.encode_utf16().chain(Some(0)).collect();
    RegSetValueExW(
        inproc_key,
        PCWSTR::null(),
        0,
        REG_SZ,
        Some(std::slice::from_raw_parts(dll_path_wide.as_ptr() as *const u8, dll_path_wide.len() * 2)),
    ).ok()?;

    let threading_model: Vec<u16> = "Apartment".encode_utf16().chain(Some(0)).collect();
    let thread_val_name: Vec<u16> = "ThreadingModel".encode_utf16().chain(Some(0)).collect();
    RegSetValueExW(
        inproc_key,
        PCWSTR(thread_val_name.as_ptr()),
        0,
        REG_SZ,
        Some(std::slice::from_raw_parts(threading_model.as_ptr() as *const u8, threading_model.len() * 2)),
    ).ok()?;

    let _ = RegCloseKey(inproc_key);
    let _ = RegCloseKey(clsid_key);

    // 3. Register with TSF Input Processor Profiles & Category Manager
    let _ = CoInitialize(None);

    let profiles: Result<ITfInputProcessorProfiles> = CoCreateInstance(
        &CLSID_TF_InputProcessorProfiles,
        None,
        CLSCTX_INPROC_SERVER,
    );

    if let Ok(profiles) = profiles {
        let _ = profiles.Register(&CLSID_RUHI_TEXT_SERVICE);
        // Remove existing profile first to ensure fresh registry overwrite
        let _ = profiles.RemoveLanguageProfile(
            &CLSID_RUHI_TEXT_SERVICE,
            LANG_CHINESE_TRADITIONAL,
            &GUID_RUHI_PROFILE,
        );
        let _ = profiles.RemoveLanguageProfile(
            &CLSID_RUHI_TEXT_SERVICE,
            LANG_ENGLISH_US,
            &GUID_RUHI_PROFILE,
        );

        let icon_path = {
            let dll_dir = std::path::Path::new(&dll_path).parent();
            let p1 = dll_dir.map(|p| p.join("ruhi.ico")).unwrap_or_default();
            let p2 = dll_dir.map(|p| p.join("..").join("ruhi.ico")).unwrap_or_default();
            let p3 = dll_dir.map(|p| p.join("..").join("..").join("ruhi.ico")).unwrap_or_default();
            if p1.exists() {
                p1.to_string_lossy().into_owned()
            } else if p2.exists() {
                p2.to_string_lossy().into_owned()
            } else if p3.exists() {
                p3.to_string_lossy().into_owned()
            } else {
                dll_path.clone()
            }
        };
        let icon_u16: Vec<u16> = icon_path.encode_utf16().chain(Some(0)).collect();

        // 1. 繁體中文環境 (zh-TW)：自適應正體國際通用名稱
        let desc_zh_u16: Vec<u16> = DESCRIPTION_ZH.encode_utf16().chain(Some(0)).collect();
        let _ = profiles.AddLanguageProfile(
            &CLSID_RUHI_TEXT_SERVICE,
            LANG_CHINESE_TRADITIONAL,
            &GUID_RUHI_PROFILE,
            &desc_zh_u16,
            &icon_u16,
            0,
        );
        let _ = profiles.EnableLanguageProfile(
            &CLSID_RUHI_TEXT_SERVICE,
            LANG_CHINESE_TRADITIONAL,
            &GUID_RUHI_PROFILE,
            true,
        );
    }

    let cat_mgr: Result<ITfCategoryMgr> = CoCreateInstance(
        &CLSID_TF_CategoryMgr,
        None,
        CLSCTX_INPROC_SERVER,
    );

    if let Ok(cat_mgr) = cat_mgr {
        let _ = cat_mgr.RegisterCategory(
            &CLSID_RUHI_TEXT_SERVICE,
            &GUID_TFCAT_TIP_KEYBOARD,
            &CLSID_RUHI_TEXT_SERVICE,
        );
        let _ = cat_mgr.RegisterCategory(
            &CLSID_RUHI_TEXT_SERVICE,
            &GUID_TFCAT_DISPLAYATTRIBUTEPROVIDER,
            &CLSID_RUHI_TEXT_SERVICE,
        );
        let _ = cat_mgr.RegisterCategory(
            &CLSID_RUHI_TEXT_SERVICE,
            &GUID_TFCAT_IMMERSIVE_INPUTMETHOD,
            &CLSID_RUHI_TEXT_SERVICE,
        );
        let _ = cat_mgr.RegisterCategory(
            &CLSID_RUHI_TEXT_SERVICE,
            &GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT,
            &CLSID_RUHI_TEXT_SERVICE,
        );
        let _ = cat_mgr.RegisterCategory(
            &CLSID_RUHI_TEXT_SERVICE,
            &GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT,
            &CLSID_RUHI_TEXT_SERVICE,
        );
        let _ = cat_mgr.RegisterCategory(
            &CLSID_RUHI_TEXT_SERVICE,
            &GUID_TFCAT_TIPCAP_SECUREMODE,
            &CLSID_RUHI_TEXT_SERVICE,
        );
        let _ = cat_mgr.RegisterCategory(
            &CLSID_RUHI_TEXT_SERVICE,
            &GUID_TFCAT_TIPCAP_INPUTMODECOMPARTMENT,
            &CLSID_RUHI_TEXT_SERVICE,
        );
        let _ = cat_mgr.RegisterCategory(
            &CLSID_RUHI_TEXT_SERVICE,
            &GUID_TFCAT_TIPCAP_UIELEMENTENABLED,
            &CLSID_RUHI_TEXT_SERVICE,
        );

        // 註冊組字底線顯示屬性 (Display Attribute)
        let desc_display_attr: Vec<u16> = "RuhI Display Attribute".encode_utf16().chain(Some(0)).collect();
        let _ = cat_mgr.RegisterGUIDDescription(&CLSID_RUHI_TEXT_SERVICE, &GUID_RUHI_DISPLAY_ATTR, &desc_display_attr);
        let _ = cat_mgr.RegisterCategory(
            &CLSID_RUHI_TEXT_SERVICE,
            &GUID_TFCAT_DISPLAYATTRIBUTEPROPERTY,
            &GUID_RUHI_DISPLAY_ATTR,
        );
    }

    Ok(())
}

pub unsafe fn unregister_server() -> Result<()> {
    let _ = CoInitialize(None);

    let cat_mgr: Result<ITfCategoryMgr> = CoCreateInstance(
        &CLSID_TF_CategoryMgr,
        None,
        CLSCTX_INPROC_SERVER,
    );

    if let Ok(cat_mgr) = cat_mgr {
        let _ = cat_mgr.UnregisterCategory(
            &CLSID_RUHI_TEXT_SERVICE,
            &GUID_TFCAT_DISPLAYATTRIBUTEPROPERTY,
            &GUID_RUHI_DISPLAY_ATTR,
        );
        let _ = cat_mgr.UnregisterGUIDDescription(&CLSID_RUHI_TEXT_SERVICE, &GUID_RUHI_DISPLAY_ATTR);
        let _ = cat_mgr.UnregisterCategory(
            &CLSID_RUHI_TEXT_SERVICE,
            &GUID_TFCAT_TIPCAP_UIELEMENTENABLED,
            &CLSID_RUHI_TEXT_SERVICE,
        );
        let _ = cat_mgr.UnregisterCategory(
            &CLSID_RUHI_TEXT_SERVICE,
            &GUID_TFCAT_TIPCAP_INPUTMODECOMPARTMENT,
            &CLSID_RUHI_TEXT_SERVICE,
        );
        let _ = cat_mgr.UnregisterCategory(
            &CLSID_RUHI_TEXT_SERVICE,
            &GUID_TFCAT_TIPCAP_SECUREMODE,
            &CLSID_RUHI_TEXT_SERVICE,
        );
        let _ = cat_mgr.UnregisterCategory(
            &CLSID_RUHI_TEXT_SERVICE,
            &GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT,
            &CLSID_RUHI_TEXT_SERVICE,
        );
        let _ = cat_mgr.UnregisterCategory(
            &CLSID_RUHI_TEXT_SERVICE,
            &GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT,
            &CLSID_RUHI_TEXT_SERVICE,
        );
        let _ = cat_mgr.UnregisterCategory(
            &CLSID_RUHI_TEXT_SERVICE,
            &GUID_TFCAT_IMMERSIVE_INPUTMETHOD,
            &CLSID_RUHI_TEXT_SERVICE,
        );
        let _ = cat_mgr.UnregisterCategory(
            &CLSID_RUHI_TEXT_SERVICE,
            &GUID_TFCAT_DISPLAYATTRIBUTEPROVIDER,
            &CLSID_RUHI_TEXT_SERVICE,
        );
        let _ = cat_mgr.UnregisterCategory(
            &CLSID_RUHI_TEXT_SERVICE,
            &GUID_TFCAT_TIP_KEYBOARD,
            &CLSID_RUHI_TEXT_SERVICE,
        );
    }

    let profiles: Result<ITfInputProcessorProfiles> = CoCreateInstance(
        &CLSID_TF_InputProcessorProfiles,
        None,
        CLSCTX_INPROC_SERVER,
    );

    if let Ok(profiles) = profiles {
        let _ = profiles.RemoveLanguageProfile(
            &CLSID_RUHI_TEXT_SERVICE,
            LANG_CHINESE_TRADITIONAL,
            &GUID_RUHI_PROFILE,
        );
        let _ = profiles.RemoveLanguageProfile(
            &CLSID_RUHI_TEXT_SERVICE,
            LANG_ENGLISH_US,
            &GUID_RUHI_PROFILE,
        );
        let _ = profiles.Unregister(&CLSID_RUHI_TEXT_SERVICE);
    }

    let clsid_str = format!("{{{:?}}}", CLSID_RUHI_TEXT_SERVICE);
    let clsid_key_path = format!("CLSID\\{}", clsid_str);
    let wide_key: Vec<u16> = clsid_key_path.encode_utf16().chain(Some(0)).collect();
    let inproc_path: Vec<u16> = format!("CLSID\\{}\\InprocServer32", clsid_str).encode_utf16().chain(Some(0)).collect();

    let _ = RegDeleteKeyW(HKEY_CLASSES_ROOT, PCWSTR(inproc_path.as_ptr()));
    let _ = RegDeleteKeyW(HKEY_CLASSES_ROOT, PCWSTR(wide_key.as_ptr()));

    Ok(())
}
