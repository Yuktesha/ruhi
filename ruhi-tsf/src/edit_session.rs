/// RuhI (如喜) Windows TSF Edit Session
use std::sync::{Arc, Mutex};
use windows_core::*;
use windows::Win32::UI::TextServices::*;

pub enum EditSessionAction {
    UpdateComposition {
        text: String,
        cursor_pos: usize,
        selection: Option<(usize, usize)>,
        is_commit: bool,
    },
    ProbeCaret,
    EndComposition,
}

#[implement(ITfEditSession)]
pub struct RuhiEditSession {
    context: ITfContext,
    action: EditSessionAction,
    composition_holder: Arc<Mutex<Option<ITfComposition>>>,
    sink: ITfCompositionSink,
}

impl RuhiEditSession {
    pub fn new(
        context: ITfContext,
        action: EditSessionAction,
        composition_holder: Arc<Mutex<Option<ITfComposition>>>,
        sink: ITfCompositionSink,
    ) -> Self {
        Self {
            context,
            action,
            composition_holder,
            sink,
        }
    }
}

impl ITfEditSession_Impl for RuhiEditSession_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        match &self.action {
            EditSessionAction::UpdateComposition { text, cursor_pos, selection, is_commit } => {
                let text_wide: Vec<u16> = text.encode_utf16().collect();
                let mut comp_lock = self.composition_holder.lock().unwrap();

                if *is_commit {
                    if let Some(comp) = comp_lock.take() {
                        unsafe {
                            if let Ok(range) = comp.GetRange() {
                                let _ = range.SetText(ec, 0, &text_wide);
                                // 【記事本與 RichEdit 相容修復】：結算上屏後必須將 Range 折疊至末尾並設置游標
                                // 否則 Notepad 會保留選取狀態，導致使用者鍵入下一字時將剛打好的字直接覆蓋刪除！
                                let _ = range.Collapse(ec, TF_ANCHOR_END);
                                let mut selections = [TF_SELECTION {
                                    range: std::mem::ManuallyDrop::new(range.Clone().ok()),
                                    style: TF_SELECTIONSTYLE {
                                        ase: TF_AE_NONE,
                                        fInterimChar: windows::Win32::Foundation::BOOL(0),
                                    },
                                }];
                                let _ = self.context.SetSelection(ec, &selections);
                                std::mem::ManuallyDrop::drop(&mut selections[0].range);
                            }
                            let _ = comp.EndComposition(ec);
                        }
                    } else {
                        // 直接上屏提交至游標位置（免除創建臨時 Composition，兼容 Modern Notepad 與 AppContainer）
                        let mut selections = [TF_SELECTION::default(); 1];
                        let mut fetched = 0u32;
                        unsafe {
                            if self.context.GetSelection(ec, TF_DEFAULT_SELECTION, &mut selections, &mut fetched).is_ok() && fetched > 0 {
                                if let Some(range) = &*selections[0].range {
                                    let _ = range.SetText(ec, 0, &text_wide);
                                    let _ = range.Collapse(ec, TF_ANCHOR_END);
                                    let mut commit_sel = [TF_SELECTION {
                                        range: std::mem::ManuallyDrop::new(range.Clone().ok()),
                                        style: TF_SELECTIONSTYLE {
                                            ase: TF_AE_NONE,
                                            fInterimChar: windows::Win32::Foundation::BOOL(0),
                                        },
                                    }];
                                    let _ = self.context.SetSelection(ec, &commit_sel);
                                    std::mem::ManuallyDrop::drop(&mut commit_sel[0].range);
                                }
                                std::mem::ManuallyDrop::drop(&mut selections[0].range);
                            }
                        }
                    }
                } else {
                    // 行內組字模式 (inline_underline)
                    // 1. Ensure active composition exists
                    if comp_lock.is_none() {
                        let mut selections = [TF_SELECTION::default(); 1];
                        let mut fetched = 0u32;
                        unsafe {
                            self.context.GetSelection(ec, TF_DEFAULT_SELECTION, &mut selections, &mut fetched)?;
                        }
                        if fetched > 0 {
                            if let Some(range) = &*selections[0].range {
                                let context_comp: ITfContextComposition = self.context.cast()?;
                                let comp = unsafe {
                                    context_comp.StartComposition(ec, range, &self.sink)?
                                };
                                *comp_lock = Some(comp);
                            }
                            unsafe { std::mem::ManuallyDrop::drop(&mut selections[0].range); }
                        }
                    }

                    // 2. Update text in composition
                    if let Some(comp) = comp_lock.as_ref() {
                        unsafe {
                            if let Ok(range) = comp.GetRange() {
                                let _ = range.SetText(ec, 0, &text_wide);

                                // 注入 TSF GUID_PROP_ATTRIBUTE 顯示屬性，確保 WinUI 3 Notepad 與全系統正確渲染組字底線
                                static ATOM_RUHI_ATTR: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                                let mut atom = ATOM_RUHI_ATTR.load(std::sync::atomic::Ordering::Relaxed);
                                if atom == 0 {
                                    if let Ok(cat_mgr) = windows::Win32::System::Com::CoCreateInstance::<_, ITfCategoryMgr>(
                                        &CLSID_TF_CategoryMgr,
                                        None,
                                        windows::Win32::System::Com::CLSCTX_INPROC_SERVER,
                                    ) {
                                        if let Ok(a) = cat_mgr.RegisterGUID(&crate::guid::GUID_RUHI_DISPLAY_ATTR) {
                                            atom = a;
                                            ATOM_RUHI_ATTR.store(atom, std::sync::atomic::Ordering::Relaxed);
                                        }
                                    }
                                }
                                if atom != 0 {
                                    if let Ok(prop) = self.context.GetProperty(&crate::guid::GUID_PROP_ATTRIBUTE) {
                                        if let Ok(attr_range) = range.Clone() {
                                            let var = windows_core::VARIANT::from(atom as i32);
                                            let _ = prop.SetValue(ec, &attr_range, &var);
                                        }
                                    }
                                }

                                // 在輸入緩衝區中精確定位光標與選取區 (Set Caret / Selection)
                                if let Ok(caret_range) = range.Clone() {
                                    let text_chars: Vec<char> = text.chars().collect();
                                    let (has_sel, u16_start, u16_end, active_at_start) = if let Some((s, e)) = *selection {
                                        if s < e && e <= text_chars.len() {
                                            let s_u16: usize = text_chars.iter().take(s).map(|c| c.len_utf16()).sum();
                                            let e_u16: usize = text_chars.iter().take(e).map(|c| c.len_utf16()).sum();
                                            (true, s_u16, e_u16, *cursor_pos == s)
                                        } else {
                                            (false, 0, 0, false)
                                        }
                                    } else {
                                        (false, 0, 0, false)
                                    };

                                    if has_sel && u16_start < u16_end && u16_end <= text_wide.len() {
                                        let _ = caret_range.Collapse(ec, TF_ANCHOR_START);
                                        let mut s1 = 0i32;
                                        let _ = caret_range.ShiftEnd(ec, u16_start as i32, &mut s1, std::ptr::null());
                                        let _ = caret_range.Collapse(ec, TF_ANCHOR_END);
                                        let mut s2 = 0i32;
                                        let _ = caret_range.ShiftEnd(ec, (u16_end - u16_start) as i32, &mut s2, std::ptr::null());

                                        // 利用 TSF 標準 ITfContextView 取得當前應用程式的光標螢幕坐標
                                        if let Ok(view) = self.context.GetActiveView() {
                                            if let Ok(probe_range) = caret_range.Clone() {
                                                let _ = probe_range.Collapse(ec, if active_at_start { TF_ANCHOR_START } else { TF_ANCHOR_END });
                                                let mut rc = windows::Win32::Foundation::RECT::default();
                                                let mut f_clipped = windows::Win32::Foundation::BOOL(0);
                                                if view.GetTextExt(ec, &probe_range, &mut rc, &mut f_clipped).is_ok() {
                                                    if rc.left != 0 || rc.top != 0 || rc.right != 0 {
                                                        crate::service::set_last_caret_pos(rc.left, rc.bottom + 4);
                                                    }
                                                }
                                            }
                                        }

                                        let mut caret_sel = [TF_SELECTION {
                                            range: std::mem::ManuallyDrop::new(Some(caret_range)),
                                            style: TF_SELECTIONSTYLE {
                                                ase: if active_at_start { TF_AE_START } else { TF_AE_END },
                                                fInterimChar: windows::Win32::Foundation::BOOL(0),
                                            },
                                        }];
                                        let _ = self.context.SetSelection(ec, &caret_sel);
                                        std::mem::ManuallyDrop::drop(&mut caret_sel[0].range);
                                    } else {
                                        let _ = caret_range.Collapse(ec, TF_ANCHOR_START);
                                        let mut shifted = 0i32;
                                        let target_shift = (*cursor_pos).min(text_wide.len()) as i32;
                                        let _ = caret_range.ShiftEnd(ec, target_shift, &mut shifted, std::ptr::null());
                                        let _ = caret_range.Collapse(ec, TF_ANCHOR_END);

                                        // 利用 TSF 標準 ITfContextView 取得當前應用程式的光標螢幕坐標
                                        if let Ok(view) = self.context.GetActiveView() {
                                            let mut rc = windows::Win32::Foundation::RECT::default();
                                            let mut f_clipped = windows::Win32::Foundation::BOOL(0);
                                            let mut probed = false;
                                            // 若 caret_range 為長度 0 之折疊點，部分編輯器（如 Notepad RichEdit）會回傳 TF_E_NOLAYOUT
                                            // 故先以延伸 1 字元探測邊界盒，若已在末尾則反向探測 1 字元
                                            if let Ok(probe_range) = caret_range.Clone() {
                                                let mut s = 0i32;
                                                let _ = probe_range.ShiftEnd(ec, 1, &mut s, std::ptr::null());
                                                if s > 0 && view.GetTextExt(ec, &probe_range, &mut rc, &mut f_clipped).is_ok() {
                                                    if rc.left != 0 || rc.top != 0 || rc.bottom != 0 {
                                                        crate::service::set_last_caret_pos(rc.left, rc.bottom + 4);
                                                        probed = true;
                                                    }
                                                }
                                            }
                                            if !probed {
                                                if let Ok(probe_range) = caret_range.Clone() {
                                                    let mut s = 0i32;
                                                    let _ = probe_range.ShiftStart(ec, -1, &mut s, std::ptr::null());
                                                    if s != 0 && view.GetTextExt(ec, &probe_range, &mut rc, &mut f_clipped).is_ok() {
                                                        if rc.left != 0 || rc.top != 0 || rc.bottom != 0 {
                                                            crate::service::set_last_caret_pos(rc.right, rc.bottom + 4);
                                                            probed = true;
                                                        }
                                                    }
                                                }
                                            }
                                            if !probed && view.GetTextExt(ec, &caret_range, &mut rc, &mut f_clipped).is_ok() {
                                                if rc.left != 0 || rc.top != 0 || rc.right != 0 {
                                                    crate::service::set_last_caret_pos(rc.left, rc.bottom + 4);
                                                }
                                            }
                                        }

                                        let mut caret_sel = [TF_SELECTION {
                                            range: std::mem::ManuallyDrop::new(Some(caret_range)),
                                            style: TF_SELECTIONSTYLE {
                                                ase: TF_AE_NONE,
                                                fInterimChar: windows::Win32::Foundation::BOOL(0),
                                            },
                                        }];
                                        let _ = self.context.SetSelection(ec, &caret_sel);
                                        std::mem::ManuallyDrop::drop(&mut caret_sel[0].range);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            EditSessionAction::ProbeCaret => {
                // 純只讀探測光標坐標，零副作用，絕不在宿主環境創建空白 Composition
                let comp_lock = self.composition_holder.lock().unwrap();
                let probed_range = if let Some(comp) = comp_lock.as_ref() {
                    unsafe { comp.GetRange().ok() }
                } else {
                    let mut selections = [TF_SELECTION::default(); 1];
                    let mut fetched = 0u32;
                    unsafe {
                        if self.context.GetSelection(ec, TF_DEFAULT_SELECTION, &mut selections, &mut fetched).is_ok() && fetched > 0 {
                            let r = (*selections[0].range).clone();
                            std::mem::ManuallyDrop::drop(&mut selections[0].range);
                            r
                        } else {
                            None
                        }
                    }
                };

                if let Some(range) = probed_range {
                    if let Ok(view) = unsafe { self.context.GetActiveView() } {
                        let mut rc = windows::Win32::Foundation::RECT::default();
                        let mut f_clipped = windows::Win32::Foundation::BOOL(0);
                        let mut probed = false;
                        if let Ok(probe_range) = unsafe { range.Clone() } {
                            let mut s = 0i32;
                            let _ = unsafe { probe_range.ShiftEnd(ec, 1, &mut s, std::ptr::null()) };
                            if s > 0 && unsafe { view.GetTextExt(ec, &probe_range, &mut rc, &mut f_clipped) }.is_ok() {
                                if rc.left != 0 || rc.top != 0 || rc.bottom != 0 {
                                    crate::service::set_last_caret_pos(rc.left, rc.bottom + 4);
                                    probed = true;
                                }
                            }
                        }
                        if !probed {
                            if let Ok(probe_range) = unsafe { range.Clone() } {
                                let mut s = 0i32;
                                let _ = unsafe { probe_range.ShiftStart(ec, -1, &mut s, std::ptr::null()) };
                                if s != 0 && unsafe { view.GetTextExt(ec, &probe_range, &mut rc, &mut f_clipped) }.is_ok() {
                                    if rc.left != 0 || rc.top != 0 || rc.bottom != 0 {
                                        crate::service::set_last_caret_pos(rc.right, rc.bottom + 4);
                                        probed = true;
                                    }
                                }
                            }
                        }
                        if !probed && unsafe { view.GetTextExt(ec, &range, &mut rc, &mut f_clipped) }.is_ok() {
                            if rc.left != 0 || rc.top != 0 || rc.bottom != 0 {
                                crate::service::set_last_caret_pos(rc.left, rc.bottom + 4);
                            }
                        }
                    }
                }
            }
            EditSessionAction::EndComposition => {
                let mut comp_lock = self.composition_holder.lock().unwrap();
                if let Some(comp) = comp_lock.take() {
                    unsafe {
                        // 結束未完成組字時，必須先將組字區段文字清空，否則 TSF 會直接將未結算的殘留字上屏！
                        if let Ok(range) = comp.GetRange() {
                            let _ = range.SetText(ec, 0, &[]);
                        }
                        let _ = comp.EndComposition(ec);
                    }
                }
            }
        }
        Ok(())
    }
}
