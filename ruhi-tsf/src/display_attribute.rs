/// RuhI (如喜) Windows TSF Display Attribute Provider
/// 實現 ITfDisplayAttributeProvider 與 ITfDisplayAttributeInfo
/// 為 WinUI 3 記事本 (Notepad)、LibreOffice Writer 及 Windows 11 現代應用程式提供標準組字底線與 Shift+方向鍵選取範圍高亮渲染
use windows_core::*;
use windows::Win32::Foundation::*;
use windows::Win32::UI::TextServices::*;
use crate::guid::{GUID_RUHI_DISPLAY_ATTR, GUID_RUHI_SELECTION_DISPLAY_ATTR};

#[implement(ITfDisplayAttributeInfo)]
pub struct RuhiDisplayAttributeInfo;

impl RuhiDisplayAttributeInfo {
    pub fn new() -> Self {
        Self
    }
}

impl ITfDisplayAttributeInfo_Impl for RuhiDisplayAttributeInfo_Impl {
    fn GetGUID(&self) -> Result<GUID> {
        Ok(GUID_RUHI_DISPLAY_ATTR)
    }

    fn GetDescription(&self) -> Result<BSTR> {
        Ok(BSTR::from("RuhI Composition Underline Display Attribute"))
    }

    fn GetAttributeInfo(&self, pda: *mut TF_DISPLAYATTRIBUTE) -> Result<()> {
        if pda.is_null() {
            return Err(E_INVALIDARG.into());
        }
        unsafe {
            *pda = TF_DISPLAYATTRIBUTE {
                crText: TF_DA_COLOR {
                    r#type: TF_CT_NONE,
                    Anonymous: Default::default(),
                },
                crBk: TF_DA_COLOR {
                    r#type: TF_CT_NONE,
                    Anonymous: Default::default(),
                },
                lsStyle: TF_LS_SOLID,
                fBoldLine: BOOL(0),
                crLine: TF_DA_COLOR {
                    r#type: TF_CT_NONE,
                    Anonymous: Default::default(),
                },
                bAttr: TF_ATTR_INPUT,
            };
        }
        Ok(())
    }

    fn SetAttributeInfo(&self, _pda: *const TF_DISPLAYATTRIBUTE) -> Result<()> {
        Ok(())
    }

    fn Reset(&self) -> Result<()> {
        Ok(())
    }
}

#[implement(ITfDisplayAttributeInfo)]
pub struct RuhiSelectionDisplayAttributeInfo;

impl RuhiSelectionDisplayAttributeInfo {
    pub fn new() -> Self {
        Self
    }
}

impl ITfDisplayAttributeInfo_Impl for RuhiSelectionDisplayAttributeInfo_Impl {
    fn GetGUID(&self) -> Result<GUID> {
        Ok(GUID_RUHI_SELECTION_DISPLAY_ATTR)
    }

    fn GetDescription(&self) -> Result<BSTR> {
        Ok(BSTR::from("RuhI Composition Selection Highlight Display Attribute"))
    }

    fn GetAttributeInfo(&self, pda: *mut TF_DISPLAYATTRIBUTE) -> Result<()> {
        if pda.is_null() {
            return Err(E_INVALIDARG.into());
        }
        unsafe {
            *pda = TF_DISPLAYATTRIBUTE {
                crText: TF_DA_COLOR {
                    r#type: TF_CT_COLORREF,
                    Anonymous: TF_DA_COLOR_0 {
                        cr: COLORREF(0x00FFFFFF),
                    },
                },
                crBk: TF_DA_COLOR {
                    r#type: TF_CT_COLORREF,
                    Anonymous: TF_DA_COLOR_0 {
                        cr: COLORREF(0x00EB6325), // BGR for #2563EB (鮮明皇家藍高亮底色)
                    },
                },
                lsStyle: TF_LS_SOLID,
                fBoldLine: BOOL(1),
                crLine: TF_DA_COLOR {
                    r#type: TF_CT_COLORREF,
                    Anonymous: TF_DA_COLOR_0 {
                        cr: COLORREF(0x00EB6325),
                    },
                },
                bAttr: TF_ATTR_TARGET_CONVERTED,
            };
        }
        Ok(())
    }

    fn SetAttributeInfo(&self, _pda: *const TF_DISPLAYATTRIBUTE) -> Result<()> {
        Ok(())
    }

    fn Reset(&self) -> Result<()> {
        Ok(())
    }
}

#[implement(IEnumTfDisplayAttributeInfo)]
pub struct RuhiDisplayAttributeEnum {
    current: std::sync::atomic::AtomicU32,
}

impl RuhiDisplayAttributeEnum {
    pub fn new() -> Self {
        Self {
            current: std::sync::atomic::AtomicU32::new(0),
        }
    }
}

impl IEnumTfDisplayAttributeInfo_Impl for RuhiDisplayAttributeEnum_Impl {
    fn Clone(&self) -> Result<IEnumTfDisplayAttributeInfo> {
        let cur = self.current.load(std::sync::atomic::Ordering::Relaxed);
        let cloned = RuhiDisplayAttributeEnum::new();
        cloned.current.store(cur, std::sync::atomic::Ordering::Relaxed);
        Ok(cloned.into())
    }

    fn Next(&self, ulcount: u32, rginfo: *mut Option<ITfDisplayAttributeInfo>, pcfetched: *mut u32) -> Result<()> {
        if rginfo.is_null() {
            return Err(E_INVALIDARG.into());
        }
        let mut fetched = 0u32;
        let mut cur = self.current.load(std::sync::atomic::Ordering::Relaxed);
        while fetched < ulcount && cur < 2 {
            let item: ITfDisplayAttributeInfo = if cur == 0 {
                RuhiDisplayAttributeInfo::new().into()
            } else {
                RuhiSelectionDisplayAttributeInfo::new().into()
            };
            unsafe {
                *rginfo.add(fetched as usize) = Some(item);
            }
            cur += 1;
            fetched += 1;
        }
        self.current.store(cur, std::sync::atomic::Ordering::Relaxed);
        if !pcfetched.is_null() {
            unsafe {
                *pcfetched = fetched;
            }
        }
        if fetched == ulcount {
            Ok(())
        } else {
            Err(S_FALSE.into())
        }
    }

    fn Reset(&self) -> Result<()> {
        self.current.store(0, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    fn Skip(&self, ulcount: u32) -> Result<()> {
        self.current.fetch_add(ulcount, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
}
