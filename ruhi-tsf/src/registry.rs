/// RuhI (如喜) Windows TSF COM & Category Registration
use windows_core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::*;
use windows::Win32::UI::TextServices::{
    CLSID_TF_CategoryMgr, CLSID_TF_InputProcessorProfiles, ITfCategoryMgr, ITfInputProcessorProfiles, ITfInputProcessorProfileMgr,
};

use crate::guid::*;

const DESCRIPTION_ZH: &str = "RuhI如喜通用智慧型輸入法";
const DESCRIPTION_EN: &str = "RuhI Universal Smart IME";
const DESCRIPTION_JA: &str = "RuhI 汎用スマート入力 (如喜)";
const DESCRIPTION_KO: &str = "RuhI 범용 스마트 입력기 (如喜)";

/// 支援穿透安裝之多語系 TSF 語言設定檔清單 `(LANGID, 顯示名稱)`
pub const SUPPORTED_LANG_PROFILES: &[(u16, &str)] = &[
    (0x0404, DESCRIPTION_ZH), // zh-TW (繁體中文 - 台灣)
    (0x0C04, DESCRIPTION_ZH), // zh-HK (繁體中文 - 香港)
    (0x0409, DESCRIPTION_EN), // en-US (English - United States)
    (0x0809, DESCRIPTION_EN), // en-GB (English - United Kingdom)
    (0x0411, DESCRIPTION_JA), // ja-JP (日本語)
    (0x0412, DESCRIPTION_KO), // ko-KR (한국어)
    (0x040C, DESCRIPTION_EN), // fr-FR (Français)
    (0x0407, DESCRIPTION_EN), // de-DE (Deutsch)
    (0x0C0A, DESCRIPTION_EN), // es-ES (Español)
];

/// 偵測指定 `LANGID` 是否已安裝於使用者目前之 Windows 系統鍵盤佈局或介面語系中
unsafe fn is_langid_installed_on_system(target_langid: u16) -> bool {
    if std::env::var("RUHI_REGISTER_ALL_LANGS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return true;
    }
    let sys_ui_lang = GetUserDefaultUILanguage();
    if sys_ui_lang == target_langid || (sys_ui_lang & 0x03FF) == (target_langid & 0x03FF) {
        return true;
    }
    // 預設確保繁體中文 (0x0404) 與英文 (0x0409) 若存在於系統輸入清單即啟用；若系統為華語環境保底啟用 0x0404
    if target_langid == 0x0404 && (sys_ui_lang & 0x03FF) == 0x04 {
        return true;
    }
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayoutList, HKL};
    let count = GetKeyboardLayoutList(None);
    if count > 0 {
        let mut hkls = vec![HKL::default(); count as usize];
        let fetched = GetKeyboardLayoutList(Some(&mut hkls));
        for hkl in hkls.into_iter().take(fetched as usize) {
            let hkl_langid = (hkl.0 as usize & 0xFFFF) as u16;
            if hkl_langid == target_langid {
                return true;
            }
            // 主語言相符且為該語系代表 LANGID (0x0404, 0x0409, 0x0411, 0x0412, 0x040C, 0x0407, 0x0C0A)
            if (hkl_langid & 0x03FF) == (target_langid & 0x03FF)
                && matches!(target_langid, 0x0404 | 0x0409 | 0x0411 | 0x0412 | 0x040C | 0x0407 | 0x0C0A)
            {
                return true;
            }
        }
    }
    target_langid == 0x0404
}
const LANG_CHINESE_TRADITIONAL: u16 = 0x0404; // zh-TW
const LANG_ENGLISH_US: u16 = 0x0409;          // en-US

extern "system" {
    fn GetUserDefaultUILanguage() -> u16;
}

fn get_current_ui_desc() -> &'static str {
    let lang = unsafe { GetUserDefaultUILanguage() };
    match lang {
        0x0404 | 0x0c04 | 0x1404 => DESCRIPTION_ZH,
        0x0411 => DESCRIPTION_JA,
        0x0412 => DESCRIPTION_KO,
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

    // 1. Write HKCU\Software\Classes\CLSID\{CLSID} (免管理員權限必成) 與 HKCR\CLSID\{CLSID}
    let current_desc = get_current_ui_desc();
    let desc_wide: Vec<u16> = current_desc.encode_utf16().chain(Some(0)).collect();
    let inproc_path: Vec<u16> = "InprocServer32".encode_utf16().chain(Some(0)).collect();
    let dll_path_wide: Vec<u16> = dll_path.encode_utf16().chain(Some(0)).collect();
    let threading_model: Vec<u16> = "Apartment".encode_utf16().chain(Some(0)).collect();
    let thread_val_name: Vec<u16> = "ThreadingModel".encode_utf16().chain(Some(0)).collect();

    let hkcu_clsid_path = format!("Software\\Classes\\CLSID\\{}", clsid_str);
    for (root_hkey, sub_path) in [
        (HKEY_CURRENT_USER, hkcu_clsid_path.as_str()),
        (HKEY_CLASSES_ROOT, clsid_key_path.as_str()),
    ] {
        let mut clsid_key = HKEY::default();
        let wide_key: Vec<u16> = sub_path.encode_utf16().chain(Some(0)).collect();
        if RegCreateKeyW(root_hkey, PCWSTR(wide_key.as_ptr()), &mut clsid_key).is_ok() {
            let _ = RegSetValueExW(
                clsid_key,
                PCWSTR::null(),
                0,
                REG_SZ,
                Some(std::slice::from_raw_parts(desc_wide.as_ptr() as *const u8, desc_wide.len() * 2)),
            );
            let mut inproc_key = HKEY::default();
            if RegCreateKeyW(clsid_key, PCWSTR(inproc_path.as_ptr()), &mut inproc_key).is_ok() {
                let _ = RegSetValueExW(
                    inproc_key,
                    PCWSTR::null(),
                    0,
                    REG_SZ,
                    Some(std::slice::from_raw_parts(dll_path_wide.as_ptr() as *const u8, dll_path_wide.len() * 2)),
                );
                let _ = RegSetValueExW(
                    inproc_key,
                    PCWSTR(thread_val_name.as_ptr()),
                    0,
                    REG_SZ,
                    Some(std::slice::from_raw_parts(threading_model.as_ptr() as *const u8, threading_model.len() * 2)),
                );
                let _ = RegCloseKey(inproc_key);
            }
            let _ = RegCloseKey(clsid_key);
        }
    }

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
            let parent1 = dll_dir.and_then(|p| p.parent());
            let parent2 = parent1.and_then(|p| p.parent());
            let candidates = [
                dll_dir.map(|p| p.join("ruhi.ico")).unwrap_or_default(),
                dll_dir.map(|p| p.join("assets").join("ruhi.ico")).unwrap_or_default(),
                parent1.map(|p| p.join("ruhi.ico")).unwrap_or_default(),
                parent1.map(|p| p.join("assets").join("ruhi.ico")).unwrap_or_default(),
                parent2.map(|p| p.join("ruhi.ico")).unwrap_or_default(),
                parent2.map(|p| p.join("assets").join("ruhi.ico")).unwrap_or_default(),
            ];
            candidates
                .into_iter()
                .find(|p| p.exists())
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| dll_path.clone())
        };
        let icon_u16: Vec<u16> = icon_path.encode_utf16().chain(Some(0)).collect();

        // 1. 穿透註冊多語系 TSF Language Profiles（zh-TW / en-US / ja-JP / ko-KR / fr-FR / de-DE / es-ES）
        //    - 對所有支援語系寫入 TSF Profile Catalog，讓任何語系使用者皆可於 Windows 語言選項中選取 RuhI；
        //    - 對使用者系統目前已安裝之語言自動啟用 (EnableLanguageProfile = true)，未安裝之語言則設為 false 避免強制塞入工作列語言列。
        let mgr_opt = profiles.cast::<ITfInputProcessorProfileMgr>().ok();
        let icon_slice = if !icon_u16.is_empty() && icon_u16.last() == Some(&0) {
            &icon_u16[..icon_u16.len() - 1]
        } else {
            &icon_u16[..]
        };
        for &(lang_id, profile_desc) in SUPPORTED_LANG_PROFILES {
            let _ = profiles.RemoveLanguageProfile(
                &CLSID_RUHI_TEXT_SERVICE,
                lang_id,
                &GUID_RUHI_PROFILE,
            );
            let enable_for_user = is_langid_installed_on_system(lang_id);
            let desc_u16: Vec<u16> = profile_desc.encode_utf16().chain(Some(0)).collect();
            let _ = profiles.AddLanguageProfile(
                &CLSID_RUHI_TEXT_SERVICE,
                lang_id,
                &GUID_RUHI_PROFILE,
                &desc_u16,
                &icon_u16,
                0,
            );
            let _ = profiles.EnableLanguageProfile(
                &CLSID_RUHI_TEXT_SERVICE,
                lang_id,
                &GUID_RUHI_PROFILE,
                enable_for_user,
            );
            if let Some(ref mgr) = mgr_opt {
                use windows::Win32::UI::Input::KeyboardAndMouse::HKL;
                let desc_slice = if !desc_u16.is_empty() && desc_u16.last() == Some(&0) {
                    &desc_u16[..desc_u16.len() - 1]
                } else {
                    &desc_u16[..]
                };
                let _ = mgr.RegisterProfile(
                    &CLSID_RUHI_TEXT_SERVICE,
                    lang_id,
                    &GUID_RUHI_PROFILE,
                    desc_slice,
                    icon_slice,
                    0,
                    HKL(0 as _),
                    0,
                    BOOL::from(enable_for_user),
                    0,
                );
            }
        }

        // 保底相容原繁體中文 (zh-TW) 變數結構
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

        // 現代 Windows 8 / 10 / 11 ProfileMgr 註冊（核心：綁定 HKL 替代佈局 0x04040404 以完全支援 LibreOffice 等 Win32 應用程式！）
        if let Ok(mgr) = profiles.cast::<ITfInputProcessorProfileMgr>() {
            use windows::Win32::UI::Input::KeyboardAndMouse::HKL;
            let hkl_sub = HKL(0 as _);
            let desc_slice = if !desc_zh_u16.is_empty() && desc_zh_u16.last() == Some(&0) {
                &desc_zh_u16[..desc_zh_u16.len() - 1]
            } else {
                &desc_zh_u16[..]
            };
            let icon_slice = if !icon_u16.is_empty() && icon_u16.last() == Some(&0) {
                &icon_u16[..icon_u16.len() - 1]
            } else {
                &icon_u16[..]
            };
            let _ = mgr.RegisterProfile(
                &CLSID_RUHI_TEXT_SERVICE,
                LANG_CHINESE_TRADITIONAL,
                &GUID_RUHI_PROFILE,
                desc_slice,
                icon_slice,
                0,
                hkl_sub,
                0,
                BOOL::from(true),
                0,
            );
        }
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

        // 註冊組字底線與選取高亮顯示屬性 (Display Attributes)
        let desc_display_attr: Vec<u16> = "RuhI Display Attribute".encode_utf16().chain(Some(0)).collect();
        let _ = cat_mgr.RegisterGUIDDescription(&CLSID_RUHI_TEXT_SERVICE, &GUID_RUHI_DISPLAY_ATTR, &desc_display_attr);
        let _ = cat_mgr.RegisterCategory(
            &CLSID_RUHI_TEXT_SERVICE,
            &GUID_TFCAT_DISPLAYATTRIBUTEPROPERTY,
            &GUID_RUHI_DISPLAY_ATTR,
        );

        let desc_sel_attr: Vec<u16> = "RuhI Selection Display Attribute".encode_utf16().chain(Some(0)).collect();
        let _ = cat_mgr.RegisterGUIDDescription(&CLSID_RUHI_TEXT_SERVICE, &GUID_RUHI_SELECTION_DISPLAY_ATTR, &desc_sel_attr);
        let _ = cat_mgr.RegisterCategory(
            &CLSID_RUHI_TEXT_SERVICE,
            &GUID_TFCAT_DISPLAYATTRIBUTEPROPERTY,
            &GUID_RUHI_SELECTION_DISPLAY_ATTR,
        );
    }

    // 4. 確保 HKCU\Software\Microsoft\CTF\Assemblies 關聯如喜為繁體中文預設 TIP (徹底根除 LibreOffice 等 Win32 應用程式切換問題)
    let asm_path: Vec<u16> = "Software\\Microsoft\\CTF\\Assemblies\\0x00000404\\{34745C63-B2F0-4784-8B67-5E12C8701A31}".encode_utf16().chain(Some(0)).collect();
    let mut asm_key = HKEY::default();
    if RegCreateKeyW(HKEY_CURRENT_USER, PCWSTR(asm_path.as_ptr()), &mut asm_key).is_ok() {
        let clsid_val: Vec<u16> = clsid_str.encode_utf16().chain(Some(0)).collect();
        let prof_str = format!("{{{:?}}}", GUID_RUHI_PROFILE);
        let prof_val: Vec<u16> = prof_str.encode_utf16().chain(Some(0)).collect();
        let def_name: Vec<u16> = "Default".encode_utf16().chain(Some(0)).collect();
        let prof_name: Vec<u16> = "Profile".encode_utf16().chain(Some(0)).collect();
        let kl_name: Vec<u16> = "KeyboardLayout".encode_utf16().chain(Some(0)).collect();
        let kl_val: u32 = 0x04040404;

        let _ = RegSetValueExW(asm_key, PCWSTR(def_name.as_ptr()), 0, REG_SZ, Some(std::slice::from_raw_parts(clsid_val.as_ptr() as *const u8, clsid_val.len() * 2)));
        let _ = RegSetValueExW(asm_key, PCWSTR(prof_name.as_ptr()), 0, REG_SZ, Some(std::slice::from_raw_parts(prof_val.as_ptr() as *const u8, prof_val.len() * 2)));
        let _ = RegSetValueExW(asm_key, PCWSTR(kl_name.as_ptr()), 0, REG_DWORD, Some(std::slice::from_raw_parts(&kl_val as *const u32 as *const u8, 4)));
        let _ = RegCloseKey(asm_key);
    }

    // 5. 確保 HKCU\Software\Microsoft\CTF\SortOrder\AssemblyItem 也寫入有效 KeyboardLayout 0x04040404
    let sort_path: Vec<u16> = "Software\\Microsoft\\CTF\\SortOrder\\AssemblyItem\\0x00000404\\{34745C63-B2F0-4784-8B67-5E12C8701A31}\\00000000".encode_utf16().chain(Some(0)).collect();
    let mut sort_key = HKEY::default();
    if RegCreateKeyW(HKEY_CURRENT_USER, PCWSTR(sort_path.as_ptr()), &mut sort_key).is_ok() {
        let clsid_val: Vec<u16> = clsid_str.encode_utf16().chain(Some(0)).collect();
        let prof_str = format!("{{{:?}}}", GUID_RUHI_PROFILE);
        let prof_val: Vec<u16> = prof_str.encode_utf16().chain(Some(0)).collect();
        let clsid_name: Vec<u16> = "CLSID".encode_utf16().chain(Some(0)).collect();
        let prof_name: Vec<u16> = "Profile".encode_utf16().chain(Some(0)).collect();
        let kl_name: Vec<u16> = "KeyboardLayout".encode_utf16().chain(Some(0)).collect();
        let kl_val: u32 = 0x04040404;

        let _ = RegSetValueExW(sort_key, PCWSTR(clsid_name.as_ptr()), 0, REG_SZ, Some(std::slice::from_raw_parts(clsid_val.as_ptr() as *const u8, clsid_val.len() * 2)));
        let _ = RegSetValueExW(sort_key, PCWSTR(prof_name.as_ptr()), 0, REG_SZ, Some(std::slice::from_raw_parts(prof_val.as_ptr() as *const u8, prof_val.len() * 2)));
        let _ = RegSetValueExW(sort_key, PCWSTR(kl_name.as_ptr()), 0, REG_DWORD, Some(std::slice::from_raw_parts(&kl_val as *const u32 as *const u8, 4)));
        let _ = RegCloseKey(sort_key);
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
        for &(lang_id, _) in SUPPORTED_LANG_PROFILES {
            let _ = profiles.RemoveLanguageProfile(
                &CLSID_RUHI_TEXT_SERVICE,
                lang_id,
                &GUID_RUHI_PROFILE,
            );
        }
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
