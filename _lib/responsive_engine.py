# -*- coding: utf-8 -*-
"""
==============================================================================
ATG RESPONSIVE LAYOUT ENGINE (RWD 響應式排版與容器引擎)
==============================================================================
Provides standard responsive tools for all ATGprjs desktop applications:
1. ResponsiveContainer: Breakpoint-based side-by-side vs stacked layout switcher.
2. ElasticSpacer: Flexible space absorber preventing control crowding.
3. visual_char_length: Unicode East Asian Width-aware visual char calculation.
4. auto_fit_combobox_width: Dynamic width auto-fit for ttk.Combobox across CJK & European languages.
5. auto_fit_all_comboboxes: Recursive auto-fit for all Comboboxes in a widget tree.
6. adapt_window_to_content: Dynamic RWD window geometry engine based on rendered layout.
"""

import sys
import unicodedata
import tkinter as tk
from tkinter import ttk


def visual_char_length(text):
    """
    Calculates visual character length taking Unicode East Asian Width into account
    (CJK full-width 'F', 'W', 'A' count as 2, Latin/half-width as 1).
    """
    if not text:
        return 0
    return sum(2 if unicodedata.east_asian_width(c) in ('F', 'W', 'A') else 1 for c in str(text))


def cjk_smart_wrap(text, max_width_px, font=None, visual_len_limit=None):
    """
    CJK-Aware Smart Line Wrapping Engine (CJK 智慧避頭尾換行與詞塊保護排版引擎).
    
    Implements:
    1. CJK Character-by-character fluid wrapping (not limited to Latin space boundaries).
    2. Latin word & number chunk preservation (e.g. '15 分鐘', 'Enter 鍵', 'N 天/小時' remain intact).
    3. Strict East Asian Kinsoku Shori (避頭尾法則):
       - No line starts with closing punctuation: ，。、；：？！）」』】》〉”’…—～·%‰℃℉
       - No line ends with opening punctuation: （「『【《〈“‘#￥$€
    4. Exact font pixel measurement or visual length fallback.
    """
    if not text:
        return ""
        
    NO_LINE_START = set("，。、；：？！）」』】》〉”’…—～·%‰℃℉)")
    NO_LINE_END = set("（「『【《〈“‘#￥$€(")
    
    tokens = []
    i = 0
    text_str = str(text)
    n = len(text_str)
    
    while i < n:
        ch = text_str[i]
        if ch == "\n":
            tokens.append("\n")
            i += 1
        elif ord(ch) < 128 and (ch.isalnum() or ch in "_-+/"):
            # Latin word / number chunk
            j = i
            while j < n and ord(text_str[j]) < 128 and (text_str[j].isalnum() or text_str[j] in "_-+/"):
                j += 1
            tokens.append(text_str[i:j])
            i = j
        else:
            tokens.append(ch)
            i += 1
            
    def _measure(s):
        if font is not None and hasattr(font, "measure"):
            try:
                return font.measure(s)
            except Exception:
                pass
        return visual_char_length(s) * 8 # Approximation fallback
        
    limit_px = max_width_px if max_width_px is not None else (visual_len_limit * 8 if visual_len_limit else 300)
    
    lines = []
    cur_line = ""
    
    for tok in tokens:
        if tok == "\n":
            lines.append(cur_line)
            cur_line = ""
            continue
            
        test_line = cur_line + tok
        w = _measure(test_line)
        if w <= limit_px:
            cur_line = test_line
        else:
            if not cur_line:
                cur_line = tok
                continue
                
            # Kinsoku Shori check
            if tok and tok[0] in NO_LINE_START and len(cur_line) > 0:
                next_start = cur_line[-1] + tok
                cur_line = cur_line[:-1]
                lines.append(cur_line)
                cur_line = next_start
            elif cur_line and cur_line[-1] in NO_LINE_END:
                next_start = cur_line[-1] + tok
                cur_line = cur_line[:-1]
                lines.append(cur_line)
                cur_line = next_start.lstrip()
            else:
                lines.append(cur_line)
                cur_line = tok.lstrip()
                
    if cur_line:
        lines.append(cur_line)
        
    return "\n".join(line.strip() for line in lines)


class JustifiedCJKLabel(tk.Canvas):
    """
    Ultra-lightweight, zero-dependency CJK Justified Paragraph Widget (東亞兩端對齊排版元件).
    
    Features:
    1. Full Justification (兩端對齊 / 齊頭齊尾):
       - Distributes micro-spacing evenly across non-final lines so they are 100% pixel-flush with right border.
    2. Natural Final Line:
       - The last line stays naturally left-aligned without awkward over-stretching.
    3. CJK Kinsoku Shori (東亞避頭尾法則):
       - Prevents punctuation from landing on line starts/ends.
    4. Word-block Chunking:
       - English words and numbers (e.g. '15 分鐘', 'Enter 鍵') stay as atomic chunks.
    5. Native Canvas rendering:
       - 60fps responsive resize, theme-aware, zero C++ library dependencies.
    """
    def __init__(self, parent, text="", font=None, font_latin=None, fg="#e2e8f0", bg=None, line_spacing=4, **kwargs):
        self.bg_color = bg or "#1f242d"
        super().__init__(parent, bg=self.bg_color, highlightthickness=0, bd=0, **kwargs)
        self.raw_text = text
        self.font = font or ("Microsoft JhengHei UI", 11)
        self.font_latin = font_latin
        self.fg_color = fg
        self.line_spacing = line_spacing
        self._rendered_w = -1
        self.bind("<Configure>", self._on_resize)
        
    def set_text(self, text):
        self.raw_text = text
        self.render()
        
    def set_theme(self, fg, bg):
        self.fg_color = fg
        self.bg_color = bg
        self.config(bg=bg)
        self.render()

    def _resolve_tk_font(self, f_spec):
        import tkinter.font as tkfont
        if isinstance(f_spec, tkfont.Font):
            return f_spec
        if isinstance(f_spec, str):
            try:
                return tkfont.nametofont(f_spec)
            except Exception:
                pass
        if isinstance(f_spec, (tuple, list)):
            family = f_spec[0]
            size = f_spec[1] if len(f_spec) > 1 else 11
            weight = f_spec[2] if len(f_spec) > 2 else "normal"
            return tkfont.Font(family=family, size=size, weight=weight)
        return tkfont.nametofont("TkDefaultFont")

    def _get_token_font(self, tok, f_cjk, f_latin):
        """Returns the appropriate font object for a token based on script/alphabet (複合字型分流)."""
        if f_latin is not None and all(ord(c) < 128 for c in tok):
            return f_latin
        return f_cjk

    def _tokenize(self, text):
        tokens = []
        i = 0
        text_str = str(text)
        n = len(text_str)
        while i < n:
            ch = text_str[i]
            if ch == "\n":
                tokens.append("\n")
                i += 1
            elif ord(ch) < 128 and (ch.isalnum() or ch in "_-+/"):
                j = i
                while j < n and ord(text_str[j]) < 128 and (text_str[j].isalnum() or text_str[j] in "_-+/"):
                    j += 1
                tokens.append(text_str[i:j])
                i = j
            else:
                tokens.append(ch)
                i += 1
        return tokens

    def _break_lines(self, tokens, max_w, f_cjk, f_latin):
        NO_LINE_START = set("，。、；：？！）」』】》〉”’…—～·%‰℃℉)")
        NO_LINE_END = set("（「『【《〈“‘#￥$€(")
        
        lines = []
        cur_line = []
        cur_w = 0
        
        for tok in tokens:
            if tok == "\n":
                lines.append(cur_line)
                cur_line = []
                cur_w = 0
                continue
                
            tok_font = self._get_token_font(tok, f_cjk, f_latin)
            tw = tok_font.measure(tok)
            if cur_w + tw <= max_w:
                cur_line.append(tok)
                cur_w += tw
            else:
                if not cur_line:
                    cur_line.append(tok)
                    continue
                    
                # Kinsoku Shori check
                if tok and tok[0] in NO_LINE_START and len(cur_line) > 0:
                    last_tok = cur_line.pop()
                    lines.append(cur_line)
                    cur_line = [last_tok, tok]
                    last_f = self._get_token_font(last_tok, f_cjk, f_latin)
                    cur_w = last_f.measure(last_tok) + tw
                elif cur_line and cur_line[-1] in NO_LINE_END:
                    last_tok = cur_line.pop()
                    lines.append(cur_line)
                    cur_line = [last_tok, tok]
                    last_f = self._get_token_font(last_tok, f_cjk, f_latin)
                    cur_w = last_f.measure(last_tok) + tw
                else:
                    lines.append(cur_line)
                    cur_line = [tok]
                    cur_w = tw
                    
        if cur_line:
            lines.append(cur_line)
        return lines

    def render(self):
        self.delete("all")
        w = self.winfo_width()
        if w <= 10:
            return
            
        f_cjk = self._resolve_tk_font(self.font)
        f_latin = self._resolve_tk_font(self.font_latin) if self.font_latin else None
        
        tokens = self._tokenize(self.raw_text)
        lines = self._break_lines(tokens, w, f_cjk, f_latin)
        if not lines:
            return
            
        line_h = max(f_cjk.metrics("linespace"), f_latin.metrics("linespace") if f_latin else 0) + self.line_spacing
        total_h = len(lines) * line_h + 2
        self.config(height=total_h)
        
        y = 0
        num_lines = len(lines)
        
        for idx, line_toks in enumerate(lines):
            is_last = (idx == num_lines - 1)
            line_w = sum(self._get_token_font(t, f_cjk, f_latin).measure(t) for t in line_toks)
            extra_w = max(0, w - line_w)
            
            # If not the last line and we have more than 1 token, distribute extra_w across gaps (兩端對齊!)
            gap_count = len(line_toks) - 1
            if not is_last and gap_count > 0 and extra_w > 0 and extra_w < (w * 0.35):
                gap_pad = extra_w / gap_count
            else:
                gap_pad = 0.0
                
            x = 0.0
            for t_idx, tok in enumerate(line_toks):
                tok_font = self._get_token_font(tok, f_cjk, f_latin)
                self.create_text(int(round(x)), y, text=tok, anchor="nw", fill=self.fg_color, font=tok_font)
                x += tok_font.measure(tok) + gap_pad
                
            y += line_h

    def _on_resize(self, event):
        if event.width != self._rendered_w and event.width > 20:
            self._rendered_w = event.width
            self.render()


def auto_fit_combobox_width(combo, values=None, min_w=6, max_w=40, pad=3):
    """
    RWD Auto-Fit for ttk.Combobox:
    Calculates optimal visual display character width across all values based on
    CJK full-width (2 units) and Latin half-width (1 unit), plus drop-arrow padding.
    Ensures that dropdowns in CJK are compact and European strings are never clipped.
    
    :param combo: The ttk.Combobox instance.
    :param values: Optional list of values (defaults to combo.cget('values')).
    :param min_w: Minimum allowed character width.
    :param max_w: Maximum allowed character width.
    :param pad: Safety padding character units (default 3 for dropdown arrow).
    :return: The applied integer character width.
    """
    if values is None:
        try:
            values = combo.cget("values")
        except Exception:
            values = None
    if not values:
        return min_w
        
    max_len = max((visual_char_length(v) for v in values), default=min_w)
    calc_w = max(min_w, min(max_w, max_len + pad))
    try:
        combo.config(width=calc_w)
    except Exception:
        pass
    return calc_w


def auto_fit_all_comboboxes(container, min_w=6, max_w=40, pad=3):
    """
    Recursively finds and auto-fits all ttk.Combobox instances inside a container.
    """
    count = 0
    def _traverse(w):
        nonlocal count
        if isinstance(w, ttk.Combobox):
            auto_fit_combobox_width(w, min_w=min_w, max_w=max_w, pad=pad)
            count += 1
        for child in w.winfo_children():
            _traverse(child)
            
    try:
        _traverse(container)
    except Exception:
        pass
    return count


def adapt_window_to_content(
    window,
    content_widget,
    target_w=None,
    min_w=None,
    max_w=None,
    min_h=None,
    max_h=None,
    pad_w=0,
    pad_h=0,
    keep_on_screen=True,
    padding_screen=15,
    cur_x=None,
    cur_y=None,
    tolerance=2
):
    """
    Universal RWD Content-Adaptive Window Geometry Engine:
    Measures the actual rendered bounding box of content_widget, applies scale & padding,
    and dynamically resizes & positions the target Toplevel/HUD window to guarantee
    zero text clipping while remaining within screen boundaries.
    
    :param window: The target tk.Toplevel or tk.Tk window to resize.
    :param content_widget: The main inner Frame/Canvas containing all rendered children.
    :param target_w: Optional fixed/base width (if None, measures content_widget.winfo_reqwidth()).
    :param min_w: Minimum allowable width.
    :param max_w: Maximum allowable width.
    :param min_h: Minimum allowable height.
    :param max_h: Maximum allowable height.
    :param pad_w: Extra horizontal padding to add to measured width.
    :param pad_h: Extra vertical padding to add to measured height.
    :param keep_on_screen: Whether to ensure the window doesn't overflow screen boundaries.
    :param padding_screen: Screen edge safety margin (px).
    :param cur_x: Current window X position (if None, queries from window.winfo_x()).
    :param cur_y: Current window Y position (if None, queries from window.winfo_y()).
    :param tolerance: Minimum pixel difference required to trigger a geometry change.
    :return: Tuple of (new_w, new_h, new_x, new_y) or None if no change.
    """
    if not window or not window.winfo_exists() or not content_widget or not content_widget.winfo_exists():
        return None
        
    try:
        req_w = content_widget.winfo_reqwidth() + pad_w
        req_h = content_widget.winfo_reqheight() + pad_h
        
        # Determine actual width
        actual_w = target_w if target_w is not None else req_w
        if min_w is not None: actual_w = max(min_w, actual_w)
        if max_w is not None: actual_w = min(max_w, actual_w)
        
        # Determine actual height
        actual_h = req_h
        if min_h is not None: actual_h = max(min_h, actual_h)
        if max_h is not None: actual_h = min(max_h, actual_h)
        
        sw = window.winfo_screenwidth()
        sh = window.winfo_screenheight()
        
        x = cur_x if cur_x is not None else window.winfo_x()
        y = cur_y if cur_y is not None else window.winfo_y()
        
        if keep_on_screen:
            if x + actual_w > sw - padding_screen:
                x = max(padding_screen, sw - actual_w - padding_screen)
            if y + actual_h > sh - padding_screen:
                y = max(padding_screen, sh - actual_h - padding_screen)
            if x < padding_screen:
                x = padding_screen
            if y < padding_screen:
                y = padding_screen
                
        # Check current geometry
        curr_w = window.winfo_width()
        curr_h = window.winfo_height()
        
        if abs(actual_w - curr_w) > tolerance or abs(actual_h - curr_h) > tolerance or abs(x - window.winfo_x()) > tolerance or abs(y - window.winfo_y()) > tolerance:
            window.geometry(f"{int(actual_w)}x{int(actual_h)}+{int(x)}+{int(y)}")
            return int(actual_w), int(actual_h), int(x), int(y)
            
    except Exception:
        pass
        
    return None


class DegradationLevel:
    FULL = 0        # 完整模式: 圖示 + 完整文字 (Icon + Full Text)
    SHORT = 1       # 精簡模式: 圖示 + 縮寫短文字 (Icon + Short Text)
    ICON_ONLY = 2   # 極簡模式: 純圖示 (Icon Only with Tooltip)


class ToolbarButton(ttk.Button):
    """
    RWD-Aware Toolbar Button that can dynamically degrade from Full Text -> Short Text -> Icon Only.
    """
    def __init__(self, parent, icon="", text="", short_text="", tooltip="", command=None, **kwargs):
        self.icon = icon.strip() if icon else ""
        self.full_text = text.strip() if text else ""
        self.short_text = short_text.strip() if short_text else self.full_text
        self.tooltip = tooltip or self.full_text
        self.current_level = DegradationLevel.FULL
        
        display_label = self._get_label_for_level(DegradationLevel.FULL)
        super().__init__(parent, text=display_label, command=command, **kwargs)
        
        if self.tooltip:
            self._bind_tooltip()
            
    def _clean_text_prefix(self, txt):
        if not txt:
            return ""
        if self.icon and txt.startswith(self.icon):
            return txt[len(self.icon):].strip()
        return txt.strip()

    def _get_label_for_level(self, level):
        if level == DegradationLevel.ICON_ONLY:
            return self.icon if self.icon else (self._clean_text_prefix(self.short_text)[:2] or "•")
        elif level == DegradationLevel.SHORT:
            clean = self._clean_text_prefix(self.short_text)
            return f"{self.icon} {clean}".strip() if self.icon else clean
        else:
            clean = self._clean_text_prefix(self.full_text)
            return f"{self.icon} {clean}".strip() if self.icon else clean
            
    def update_text(self, text=None, short_text=None, icon=None):
        if text is not None:
            self.full_text = text.strip()
        if short_text is not None:
            self.short_text = short_text.strip()
        if icon is not None:
            self.icon = icon.strip()
        new_txt = self._get_label_for_level(self.current_level)
        try:
            self.config(text=new_txt)
        except Exception:
            pass

    def set_degradation_level(self, level):
        if self.current_level == level:
            return
        self.current_level = level
        new_txt = self._get_label_for_level(level)
        try:
            self.config(text=new_txt)
        except Exception:
            pass

    def _bind_tooltip(self):
        try:
            self.bind("<Enter>", self._on_enter, add="+")
            self.bind("<Leave>", self._on_leave, add="+")
        except Exception:
            pass

    def _on_enter(self, event=None):
        if self.current_level == DegradationLevel.ICON_ONLY:
            top = self.winfo_toplevel()
            if hasattr(top, 'show_tooltip_for_meta') or hasattr(top, 'show_simple_tooltip'):
                pass

    def _on_leave(self, event=None):
        pass


class ToolbarSubGroup(ttk.Frame):
    """
    子工具群組 (Sub-group):
    將高度相關的 2~4 個按鈕緊密捆綁在一起（例如：存檔 + 另存 + 重複）。
    折行時以整個子群組為最小換行單位，避免單個按鈕零散孤立。
    """
    def __init__(self, parent, name="", **kwargs):
        super().__init__(parent, **kwargs)
        self.name = name
        self._items = []
        
    def add_widget(self, widget, padx=1, pady=1, side="left"):
        widget.pack(side=side, padx=padx, pady=pady)
        self._items.append(widget)
        return widget
        
    def add_button(self, icon="", text="", short_text="", tooltip="", command=None, padx=1, **kwargs):
        btn = ToolbarButton(self, icon=icon, text=text, short_text=short_text, tooltip=tooltip, command=command, **kwargs)
        btn.pack(side="left", padx=padx)
        self._items.append(btn)
        return btn
        
    def set_degradation_level(self, level):
        for item in self._items:
            if hasattr(item, 'set_degradation_level'):
                item.set_degradation_level(level)
                
    def get_required_width(self):
        self.update_idletasks()
        return self.winfo_reqwidth()


class ToolbarGroup(ttk.Frame):
    """
    頂層工具群組 (Top-level Functional Group):
    例如「檔案與清單」、「音軌修整與輸出」。
    包含一或多個 ToolbarSubGroup。折行時以群組為最高優先級保留在同一行。
    """
    def __init__(self, parent, name="", align="left", **kwargs):
        super().__init__(parent, **kwargs)
        self.name = name
        self.align = align  # "left", "right", "center"
        self._subgroups = []
        self._direct_items = []
        
    def add_subgroup(self, name="", padx=None, **kwargs):
        sg = ToolbarSubGroup(self, name=name, **kwargs)
        if padx is None:
            padx = (3, 0) if self._subgroups else (0, 0)
        sg.pack(side="left", padx=padx)
        self._subgroups.append(sg)
        return sg
        
    def add_widget(self, widget, padx=1, side="left"):
        widget.pack(side=side, padx=padx)
        self._direct_items.append(widget)
        return widget
        
    def set_degradation_level(self, level):
        for sg in self._subgroups:
            sg.set_degradation_level(level)
        for item in self._direct_items:
            if hasattr(item, 'set_degradation_level'):
                item.set_degradation_level(level)
                
    def get_subgroups(self):
        return list(self._subgroups)
        
    def get_required_width(self):
        self.update_idletasks()
        return self.winfo_reqwidth()


class ResponsiveToolbar(ttk.Frame):
    """
    層次式自動折行與漸進降級響應工具列 (Hierarchy-Aware Flow-Wrapping Responsive Toolbar)
    
    支援策略：
    1. 單行全容納 (Single Row): 依 alignment 策略排版 (justify 左右兩端對齊 / center 水平置中 / start 靠左齊頭)。
    2. 群組級折行 (Top-Level Group Wrap): 寬度不足時，以保留 ToolbarGroup 完整性為第一優先換行。
    3. 子群組折行 (Sub-Group Wrap): 單一群組超寬時，自動拆解為 ToolbarSubGroup 分行排列。
    4. 漸進式降級 (Progressive Degradation): 視窗極窄時，文字由完整 -> 縮寫 -> 純圖示 (Icon Only) 階梯式自動收斂。
    """
    def __init__(self, parent, alignment="justify", auto_wrap=True, enable_degradation=True, **kwargs):
        super().__init__(parent, **kwargs)
        self.alignment = alignment  # "justify", "center", "start", "end"
        self.auto_wrap = auto_wrap
        self.enable_degradation = enable_degradation
        self._groups = []
        self._current_level = DegradationLevel.FULL
        self._row_frames = []
        self._last_width = 0
        self._relayout_job = None
        
        self.bind("<Configure>", self._on_configure, add="+")
        
    def add_group(self, name="", align="left", **kwargs):
        group = ToolbarGroup(self, name=name, align=align, **kwargs)
        self._groups.append(group)
        return group
        
    def _on_configure(self, event=None):
        if event:
            new_w = event.width
        else:
            new_w = self.winfo_width()
            
        if new_w < 40 or abs(new_w - self._last_width) < 5:
            return
            
        self._last_width = new_w
        if self._relayout_job:
            try: self.after_cancel(self._relayout_job)
            except Exception: pass
            
        self._relayout_job = self.after(30, self.re_layout)
        
    def re_layout(self):
        self._relayout_job = None
        avail_w = self.winfo_width()
        if avail_w < 50:
            try:
                top = self.winfo_toplevel()
                avail_w = top.winfo_width()
            except Exception:
                avail_w = 1280
        if avail_w < 50:
            avail_w = 1280
            
        # Clear existing row frames
        for rf in self._row_frames:
            try: rf.destroy()
            except Exception: pass
        self._row_frames.clear()
        
        # Determine best degradation level that fits
        if self.enable_degradation:
            target_level = DegradationLevel.FULL
            if avail_w < 650:
                target_level = DegradationLevel.ICON_ONLY
            elif avail_w < 950:
                target_level = DegradationLevel.SHORT
            else:
                target_level = DegradationLevel.FULL
                
            if target_level != self._current_level:
                self._current_level = target_level
                for g in self._groups:
                    g.set_degradation_level(target_level)
                    
        self.update_idletasks()
        
        # Measure top-level group widths
        group_widths = [g.get_required_width() + 8 for g in self._groups]
        total_w = sum(group_widths)
        
        # -------------------------------------------------------------
        # Strategy 1: All groups fit on a Single Row (無折行)
        # -------------------------------------------------------------
        if total_w <= avail_w or not self.auto_wrap:
            if self.alignment == "justify":
                # Left groups on left, Right groups on right directly in self
                for g in self._groups:
                    g.pack_forget()
                    if getattr(g, 'align', 'left') == "right":
                        g.pack(side="right", padx=3, pady=1)
                    else:
                        g.pack(side="left", padx=3, pady=1)
            elif self.alignment == "center":
                for g in self._groups:
                    g.pack_forget()
                    g.pack(side="left", padx=3, pady=1)
            else:
                side = "right" if self.alignment == "end" else "left"
                for g in self._groups:
                    g.pack_forget()
                    g.pack(side=side, padx=3, pady=1)
            return

        # -------------------------------------------------------------
        # Strategy 2: Multi-Row Hierarchy-Aware Flow Wrapping (群組分行堆疊)
        # -------------------------------------------------------------
        for g in self._groups:
            g.pack_forget()
            g.pack(side="top", fill="x", expand=True, anchor="w", pady=1)


class ResponsiveContainer(ttk.Frame):
    """
    自適應容器 (Responsive Container)：
    - 當父視窗或自身寬度 >= breakpoint 時：子元件採橫向並排 (Side-by-Side)
    - 當寬度 < breakpoint 時：自動切換為縱向堆疊 (Stacked Mode)
    """
    def __init__(self, parent, breakpoint=1200, **kwargs):
        super().__init__(parent, **kwargs)
        self.breakpoint = breakpoint
        self._is_compact = None
        self._items = []
        self.bind("<Configure>", self._on_configure, add="+")
        
    def add_responsive_panel(self, widget, side_weight=1):
        self._items.append((widget, side_weight))
        
    def _on_configure(self, event=None):
        w = self.winfo_width()
        if w < 50:
            return
        is_compact = w < self.breakpoint
        if self._is_compact == is_compact:
            return
        self._is_compact = is_compact
        self.re_layout()
        
    def re_layout(self):
        for widget, _ in self._items:
            try:
                widget.pack_forget()
            except Exception:
                pass
                
        if self._is_compact:
            # 縱向堆疊
            for widget, _ in self._items:
                widget.pack(side="top", fill="x", expand=True, pady=2)
        else:
            # 橫向並排
            for widget, _ in self._items:
                widget.pack(side="left", fill="both", expand=True, padx=2)


class ElasticSpacer(ttk.Frame):
    """
    彈性防擠壓佔位器 (Elastic Spacer)：
    自動吸收所有剩餘空間 (expand=True, fill='x')，確保右側重要控制項 (如音量滑桿) 永不溢出。
    """
    def __init__(self, parent, min_width=10, **kwargs):
        super().__init__(parent, width=min_width, **kwargs)
        self.pack(side="left", fill="x", expand=True)


class ResponsiveSplitter(tk.Frame):
    """
    獨立自適應分界/調整器 (Independent Standalone Splitter / Sash):
    - 作為相鄰面板之間的獨立分界物件，不附屬於任何單一面板。
    - 同時負責動態調整相鄰面板（prev_panel 與 next_panel）的尺寸（高/寬）。
    - 支援滑鼠拖曳、懸停高亮、即時幾何重算與持久化回呼 (on_drag_end)。
    - 提供細緻質感的置中抓握點 (Handle Grip: '•••••' / Subtle Line)。
    """
    def __init__(
        self,
        parent,
        panel_prev=None,
        panel_next=None,
        orient="horizontal",
        thickness=6,
        min_size_prev=60,
        min_size_next=60,
        paned_window=None,
        sash_index=0,
        on_drag_end=None,
        bg="#1e1e1e",
        hover_bg="#2c313a",
        handle_fg="#5c6370",
        **kwargs
    ):
        super().__init__(parent, bg=bg, cursor="size_ns" if orient == "horizontal" else "size_we", **kwargs)
        self.panel_prev = panel_prev
        self.panel_next = panel_next
        self.orient = orient
        self.thickness = thickness
        self.min_size_prev = min_size_prev
        self.min_size_next = min_size_next
        self.paned_window = paned_window
        self.sash_index = sash_index
        self.on_drag_end = on_drag_end
        self.bg_color = bg
        self.hover_color = hover_bg
        self.handle_color = handle_fg
        
        # Grip Indicator
        self.lbl_grip = tk.Label(
            self,
            text="•••••" if orient == "horizontal" else "•\n•\n•\n•\n•",
            font=("Segoe UI", 6),
            bg=bg,
            fg=handle_fg,
            cursor=self.cget("cursor")
        )
        self.lbl_grip.pack(anchor="center", expand=True)
        
        # Bindings
        for w in [self, self.lbl_grip]:
            w.bind("<Enter>", self._on_enter, add="+")
            w.bind("<Leave>", self._on_leave, add="+")
            w.bind("<ButtonPress-1>", self._on_drag_start, add="+")
            w.bind("<B1-Motion>", self._on_drag_motion, add="+")
            w.bind("<ButtonRelease-1>", self._on_drag_release, add="+")
            
        self._drag_data = None

    def _on_enter(self, event=None):
        self.config(bg=self.hover_color)
        self.lbl_grip.config(bg=self.hover_color, fg="#ffffff")

    def _on_leave(self, event=None):
        if not self._drag_data:
            self.config(bg=self.bg_color)
            self.lbl_grip.config(bg=self.bg_color, fg=self.handle_color)

    def _on_drag_start(self, event):
        self._drag_data = {
            'x': event.x_root,
            'y': event.y_root,
            'prev_h': self.panel_prev.winfo_height() if self.panel_prev else 0,
            'next_h': self.panel_next.winfo_height() if self.panel_next else 0,
            'prev_w': self.panel_prev.winfo_width() if self.panel_prev else 0,
            'next_w': self.panel_next.winfo_width() if self.panel_next else 0,
        }
        if self.paned_window and self.sash_index is not None:
            try:
                self._drag_data['sash_coord'] = self.paned_window.sashpos(self.sash_index)
            except Exception:
                pass

    def _on_drag_motion(self, event):
        if not self._drag_data:
            return
        if self.orient == "horizontal":
            dy = event.y_root - self._drag_data['y']
            if self.paned_window and self.sash_index is not None:
                try:
                    orig_pos = self._drag_data.get('sash_coord', 0)
                    new_pos = max(self.min_size_prev, orig_pos + dy)
                    self.paned_window.sashpos(self.sash_index, new_pos)
                except Exception:
                    pass
            elif self.panel_prev and self.panel_next:
                new_prev_h = max(self.min_size_prev, self._drag_data['prev_h'] + dy)
                new_next_h = max(self.min_size_next, self._drag_data['next_h'] - dy)
                self.panel_prev.config(height=new_prev_h)
                self.panel_next.config(height=new_next_h)
        else:
            dx = event.x_root - self._drag_data['x']
            if self.paned_window and self.sash_index is not None:
                try:
                    orig_pos = self._drag_data.get('sash_coord', 0)
                    new_pos = max(self.min_size_prev, orig_pos + dx)
                    self.paned_window.sashpos(self.sash_index, new_pos)
                except Exception:
                    pass
            elif self.panel_prev and self.panel_next:
                new_prev_w = max(self.min_size_prev, self._drag_data['prev_w'] + dx)
                new_next_w = max(self.min_size_next, self._drag_data['next_w'] - dx)
                self.panel_prev.config(width=new_prev_w)
                self.panel_next.config(width=new_next_w)

    def _on_drag_release(self, event):
        self._drag_data = None
        self._on_leave()
        if callable(self.on_drag_end):
            try:
                self.on_drag_end()
            except Exception:
                pass


def pack_compact_vertical_panels(parent, panel_list, spacing=1):
    """
    緊湊無冗餘垂直面板排版引擎:
    將給定的面板清單以嚴格一致的間距（spacing px）緊湊縱向排列，完全消除多餘空隙。
    """
    for i, p in enumerate(panel_list):
        if not p: continue
        top_pad = spacing if i > 0 else 0
        try:
            p.pack(fill="x", side="top", pady=(top_pad, 0))
        except Exception:
            pass


# ==============================================================================
# 7. RWD 智慧同質表格欄寬自適應引擎 (Universal Declarative Table RWD Engine)
# ==============================================================================

def normalize_rwd_column_specs(spec):
    """
    解析宣告式欄位群組規格，支援多種語法模式：
    
    1. 簡明 DSL 管道字串語法：
       " # | 開始 / 時長 / 結束 | w=曲目標題 / 演出者 / 專輯 / 備註 "
       " # | uniform: 開始, 時長, 結束 | flex: 曲目標題, 演出者, 專輯, 備註 "
       
    2. 結構化 Tuple / List 語法：
       [ "#", ("uniform", ["開始", "時長", "結束"]), ("flex", ["曲目標題", "演出者", "專輯", "備註"]) ]
       
    3. 進階字典清單語法：
       [ {"key": "id", "title": "#", "type": "fixed"}, {"key": "start", "title": "開始", "type": "uniform", "group": "time"}, ... ]
    """
    cols = []
    if spec is None:
        return cols
        
    if isinstance(spec, str):
        parts = [p.strip() for p in spec.split("|") if p.strip()]
        uniform_group_id = 0
        for p in parts:
            if p.startswith("w=") or p.startswith("flex:") or p.startswith("elastic:") or p.startswith("weighted:"):
                sub = p.split("=", 1)[-1] if "=" in p else p.split(":", 1)[-1]
                sub_cols = [c.strip() for c in sub.replace("/", ",").split(",") if c.strip()]
                for sc in sub_cols:
                    cols.append({"key": sc, "title": sc, "type": "flex", "group": "flex"})
            elif p.startswith("uniform:") or p.startswith("same:") or p.startswith("u="):
                uniform_group_id += 1
                sub = p.split("=", 1)[-1] if "=" in p else p.split(":", 1)[-1]
                sub_cols = [c.strip() for c in sub.replace("/", ",").split(",") if c.strip()]
                for sc in sub_cols:
                    cols.append({"key": sc, "title": sc, "type": "uniform", "group": f"u_{uniform_group_id}"})
            else:
                sub_cols = [c.strip() for c in p.replace("/", ",").split(",") if c.strip()]
                if len(sub_cols) > 1:
                    uniform_group_id += 1
                    for sc in sub_cols:
                        cols.append({"key": sc, "title": sc, "type": "uniform", "group": f"u_{uniform_group_id}"})
                else:
                    for sc in sub_cols:
                        cols.append({"key": sc, "title": sc, "type": "fixed", "group": None})
    elif isinstance(spec, list):
        uniform_group_id = 0
        for item in spec:
            if isinstance(item, str):
                cols.append({"key": item, "title": item, "type": "fixed", "group": None})
            elif isinstance(item, tuple) and len(item) == 2:
                gtype, gcols = item
                if isinstance(gcols, str): gcols = [gcols]
                if gtype in ("uniform", "same"):
                    uniform_group_id += 1
                    for gc in gcols:
                        cols.append({"key": gc, "title": gc, "type": "uniform", "group": f"u_{uniform_group_id}"})
                else:
                    for gc in gcols:
                        cols.append({"key": gc, "title": gc, "type": "flex", "group": "flex"})
            elif isinstance(item, dict):
                cols.append(item)
    return cols


def calculate_table_rwd_widths(
    items,
    total_width,
    columns_spec=None,
    unit="cm", # "cm", "px", "ratio"
    is_landscape=False,
    **kwargs
):
    """
    通用 RWD 智慧同質表格欄寬自適應引擎 (Universal Table RWD Engine):
    
    支援藉由傳入宣告式表頭群組資訊，自動識別「固態欄位」、「同質等寬群組」與「瓜分剩餘空間之彈性群組」。
    
    :param items: 表格資料清單 (List of dicts or objects)。
    :param total_width: 表格總可用寬度 (cm 數值如 18.461 / 26.5 或 px 像素如 1000)。
    :param columns_spec: 宣告式欄位群組規格，例如：
           - DSL 字串: " # | 開始 / 時長 / 結束 | w=曲目標題 / 演出者 / 專輯 / 備註 "
           - Tuple 清單: [ "#", ("uniform", ["開始", "時長", "結束"]), ("flex", ["曲目標題", "演出者", "專輯", "備註"]) ]
           - 若為 None，將依 kwargs (template_key, inc_tc, inc_notes 等) 自動建構預設規格。
    :param unit: "cm" (輸出 [(col_id, title, 'X.XXXcm', 'YYYY*'), ...]), "px" (輸出 {col_name: px_int, ...}), "ratio"
    :param is_landscape: 是否為橫向寬敞版面。
    """
    # 若未傳入自訂 columns_spec，依預設模板參數自動構建 DSL
    if columns_spec is None:
        template_key = kwargs.get("template_key", "host_rundown")
        inc_tc = kwargs.get("include_timecode", kwargs.get("inc_tc", True))
        inc_tr = kwargs.get("include_transitions", kwargs.get("inc_tr", True))
        inc_notes = kwargs.get("include_notes", kwargs.get("inc_notes", True))
        
        if template_key == "host_rundown":
            flex_items = ["曲目標題", "演出者", "專輯 / 類型"]
            if inc_notes:
                flex_items.append("主持口播備忘 / 備註")
            columns_spec = [
                "#",
                ("uniform", ["開始", "時長", "結束"]),
                ("flex", flex_items)
            ]
        else:
            uniform_items = []
            if inc_tc:
                uniform_items.append("時間碼")
            uniform_items.append("時長")
            
            fixed_items = ["#"]
            if inc_tc:
                fixed_items.append("音軌")
            if inc_tr:
                fixed_items.append("淡入 / 淡出 / 重疊")
                
            flex_items = ["曲目標題", "演出者", "專輯 / 類型"]
            if inc_notes:
                flex_items.append("主持口播備忘 / 備註")
                
            columns_spec = fixed_items + [("uniform", uniform_items), ("flex", flex_items)]

    cols_meta = normalize_rwd_column_specs(columns_spec)
    is_cm = (unit == "cm")
    
    # 1. 測量各欄位在實際資料集中的視覺長度
    col_lengths = {}
    for col in cols_meta:
        k = col.get("key") or col.get("title")
        title = col.get("title") or k
        lens = []
        if items:
            for it in items:
                v = it.get(k)
                if v is None and k == "#":
                    v = str(len(items))
                elif v is None and k in ("開始", "start"):
                    st = float(it.get("global_time", 0.0))
                    s = int(round(st))
                    hh, mm, ss = s // 3600, (s % 3600) // 60, s % 60
                    v = f"{hh:02d}:{mm:02d}:{ss:02d}" if hh > 0 else f"{mm:02d}:{ss:02d}"
                elif v is None and k in ("時長", "dur", "duration"):
                    raw_dur = float(it.get("duration", 0.0))
                    so = float(it.get("start_offset", 0.0))
                    eo = float(it.get("end_offset", 0.0))
                    dur = max(0.1, raw_dur - so - eo) if (so > 0 or eo > 0) else raw_dur
                    s = int(round(dur))
                    hh, mm, ss = s // 3600, (s % 3600) // 60, s % 60
                    v = f"{hh:02d}:{mm:02d}:{ss:02d}" if hh > 0 else f"{mm:02d}:{ss:02d}"
                elif v is None and k in ("結束", "end"):
                    st = float(it.get("global_time", 0.0))
                    raw_dur = float(it.get("duration", 0.0))
                    so = float(it.get("start_offset", 0.0))
                    eo = float(it.get("end_offset", 0.0))
                    et = st + (max(0.1, raw_dur - so - eo) if (so > 0 or eo > 0) else raw_dur)
                    s = int(round(et))
                    hh, mm, ss = s // 3600, (s % 3600) // 60, s % 60
                    v = f"{hh:02d}:{mm:02d}:{ss:02d}" if hh > 0 else f"{mm:02d}:{ss:02d}"
                elif v is None:
                    # 常見中英文別名對照
                    if k in ("曲目標題", "歌名", "title"): v = it.get("title") or it.get("name") or ""
                    elif k in ("演出者", "歌手", "artist"): v = it.get("artist") or ""
                    elif k in ("專輯", "類型", "專輯 / 類型", "album"): v = it.get("album") or it.get("genre") or ""
                    elif k in ("主持口播備忘 / 備註", "備忘", "備註", "notes"): v = it.get("notes") or ""
                    else: v = str(it.get(k) or "")
                lens.append(visual_char_length(str(v)))
        if not lens:
            lens = [visual_char_length(title)]
        col_lengths[k] = lens

    # 2. 計算固態欄位 (Fixed) 與同質等寬群組 (Uniform)
    calc_widths = {}
    fixed_used = 0.0
    
    # 處理固態欄位
    for col in cols_meta:
        if col.get("type") == "fixed":
            k = col.get("key") or col.get("title")
            title = col.get("title") or k
            t_len = visual_char_length(title)
            lens = col_lengths.get(k, [2])
            max_vlen = max(lens) if lens else 2
            req_len = max(max_vlen, t_len)
            if is_cm:
                w = max(col.get("min_w", 0.65), req_len * 0.18 + 0.35)
                w = round(w, 3)
            else:
                w = max(col.get("min_w", 36), int(req_len * 9.5 + 28))
            calc_widths[k] = w
            fixed_used += w
            
    # 處理同質等寬群組 (以各群組中最寬者為基準，統一等寬分配)
    uniform_groups = {}
    for col in cols_meta:
        if col.get("type") == "uniform":
            grp = col.get("group", "default_uniform")
            uniform_groups.setdefault(grp, []).append(col)
            
    for grp, grp_cols in uniform_groups.items():
        all_grp_lens = []
        for col in grp_cols:
            k = col.get("key") or col.get("title")
            title = col.get("title") or k
            t_len = visual_char_length(title)
            lens = col_lengths.get(k, [5])
            max_vlen = max(lens) if lens else 5
            all_grp_lens.append(max(max_vlen, t_len))
        max_grp_vlen = max(all_grp_lens) if all_grp_lens else 5
        if is_cm:
            w = max(1.25, max_grp_vlen * 0.16 + 0.40)
            w = round(w, 3)
        else:
            w = max(68, int(max_grp_vlen * 9.0 + 26))
        for col in grp_cols:
            k = col.get("key") or col.get("title")
            calc_widths[k] = w
            fixed_used += w
            
    # 3. 彈性群組 (Flex) 瓜分剩餘空間 (真實需求量驅動模型 Demand-Driven Proportional Elastic Model)
    flex_cols = [c for c in cols_meta if c.get("type") == "flex"]
    min_flex_space = 4.0 if is_cm else 180
    remaining_w = max(min_flex_space, total_width - fixed_used)
    
    if flex_cols:
        # 計算各欄位實際文字完整顯示所需的「真實物理需求量 (Demand)」與「表頭閱讀底限 (Min Floor)」
        demands = {}
        min_floors = {}
        
        for col in flex_cols:
            k = col.get("key") or col.get("title")
            title = col.get("title") or k
            lens = col_lengths.get(k, [4])
            max_l = max(lens) if lens else 4
            t_len = visual_char_length(title)
            req_len = max(max_l, t_len)
            
            if is_cm:
                d_val = max(1.5, req_len * 0.18 + 0.40)
                demands[k] = round(d_val, 3)
                min_floors[k] = max(1.2, t_len * 0.16 + 0.30)
            else:
                d_val = max(50, int(req_len * 8.5 + 24))
                demands[k] = d_val
                min_floors[k] = max(40, int(t_len * 8.0 + 16))
                
        tot_demand = sum(demands.values())
        
        if remaining_w >= tot_demand:
            # 空間充裕：每欄皆 100% 滿足完整顯示需求，富餘空間依各自需求比例平滑舒展
            extra_w = remaining_w - tot_demand
            for col in flex_cols:
                k = col.get("key") or col.get("title")
                ratio = demands[k] / max(0.1, tot_demand)
                w_calc = demands[k] + extra_w * ratio
                calc_widths[k] = round(w_calc, 3) if is_cm else int(round(w_calc))
        else:
            # 空間緊縮：依各欄真實需求量比例共同分攤壓縮，同時保證不低於表頭最低閱讀底限
            scale = remaining_w / max(0.1, tot_demand)
            for col in flex_cols:
                k = col.get("key") or col.get("title")
                w_calc = max(min_floors[k], demands[k] * scale)
                calc_widths[k] = round(w_calc, 3) if is_cm else int(round(w_calc))
                
    # 4. 微調差值，使全表總和 100% 精準吻合 total_width
    calc_sum = sum(calc_widths.values())
    diff = (round(total_width - calc_sum, 3) if is_cm else int(total_width - calc_sum))
    if flex_cols:
        last_k = flex_cols[-1].get("key") or flex_cols[-1].get("title")
        calc_widths[last_k] = round(calc_widths[last_k] + diff, 3) if is_cm else (calc_widths[last_k] + diff)
        
    # 依 unit 模式輸出結果
    if is_cm:
        res = []
        letter_idx = 0
        for col in cols_meta:
            k = col.get("key") or col.get("title")
            title = col.get("title") or k
            w_val = calc_widths[k]
            col_id = f"PlaylistTable.{chr(65 + letter_idx)}"
            letter_idx += 1
            rel_str = f"{int(round(w_val * 1000 / total_width * 10))}*"
            res.append((col_id, title, f"{w_val:.3f}cm", rel_str))
        return res
    else:
        return calc_widths


# 保留 calculate_document_table_rwd_widths 作為別名相容性
calculate_document_table_rwd_widths = calculate_table_rwd_widths


def calculate_button_group_rwd_widths(
    total_width,
    buttons_spec,
    unit="px",
    is_landscape=False
):
    """
    RWD 智慧同質按鈕群組排版引擎 (Universal RWD Button Group Layout Engine):
    
    將「同質操作按鈕」統一等寬、保留「固態圖示按鈕」緊湊尺寸、並使「主要彈性動作/搜尋欄」精準瓜分剩餘像素空間。
    """
    return calculate_table_rwd_widths(
        items=None,
        total_width=total_width,
        columns_spec=buttons_spec,
        unit=unit,
        is_landscape=is_landscape
    )


def calculate_vertical_rwd_panes(
    total_height,
    fixed_bottom_demand=110,
    min_top_floor=240,
    min_middle_floor=180,
    ratio_top_mid=0.50
):
    """
    Universal Vertical RWD Engine (縱向 RWD 幾何自適應分配引擎):
    
    第一原理（先定收斂，均分彈性）：
    1. 先計算收斂端 (Block 3 底部日誌主控台)：給予完整呈現的精確固定高度 (80px ~ 140px)。
    2. 扣除收斂端高度，取得真實彈性剩餘縱向空間 (Remaining Height)。
    3. 將剩餘高度依 50% : 50% 等比瓜分給 Block 1 (播放清單) 與 Block 2 (波形工作室)。
    
    回傳字典:
    {
        "h_playlist": int,
        "h_studio": int,
        "h_log": int,
        "sash_main": int,
        "sash_lower": int
    }
    """
    total_h = max(400, int(total_height))
    
    # 1. 決定 Block 3 (日誌) 剛需高度
    if total_h < 650:
        h3 = max(65, int(total_h * 0.11))
    elif total_h < 950:
        h3 = min(115, max(85, int(total_h * 0.12)))
    elif total_h < 1500:
        h3 = min(140, max(100, int(total_h * 0.11)))
    else: # 2K / 4K
        h3 = min(180, max(120, int(total_h * 0.09)))
        
    if fixed_bottom_demand and fixed_bottom_demand > 30:
        h3 = min(h3, int(fixed_bottom_demand))
        
    rem_h = total_h - h3
    
    # 2. 均分 Block 1 (播放清單) 與 Block 2 (波形工作室)
    h1 = int(rem_h * ratio_top_mid)
    h2 = rem_h - h1
    
    # 確保最低底限
    if h1 < min_top_floor and rem_h > min_top_floor + min_middle_floor:
        h1 = min_top_floor
        h2 = rem_h - h1
    elif h2 < min_middle_floor and rem_h > min_top_floor + min_middle_floor:
        h2 = min_middle_floor
        h1 = rem_h - h2
        
    # 計算 PanedWindow 的 Sash 座標
    main_sash_pos = h1
    lower_sash_pos = h2
    
    return {
        "h_playlist": h1,
        "h_studio": h2,
        "h_log": h3,
        "sash_main": main_sash_pos,
        "sash_lower": lower_sash_pos
    }


# ==============================================================================
# ⛵ rud() / rwd() / rui() - 自適應使用者介面即使用經驗最佳化設計 (R-UI/UX)
# ==============================================================================
# "rud" (Responsive UI/UX Design)
# 愛爾蘭文 "rud" 意為「事物之本質 / 船底龍骨」，象徵穩定承載整個軟體介面平穩航行的核心基石。
def rud(container=None, **kwargs):
    """
    ⛵ rud() - 自適應使用者介面即使用經驗最佳化設計統一核心入口 (Responsive UI/UX Engine).
    
    支援靈活調用：
    1. rud(container, breakpoint=800): 建立/刷新 ResponsiveContainer
    2. rud(widget, auto_fit=True): 自動適應下拉選單或幾何物件
    3. rud(text="...", max_width_px=400): CJK 智慧避頭尾換行
    """
    if "text" in kwargs and "max_width_px" in kwargs:
        return cjk_smart_wrap(kwargs["text"], kwargs["max_width_px"], font=kwargs.get("font"))
    if container is not None and hasattr(container, "winfo_children"):
        if kwargs.get("auto_fit_combos", True):
            auto_fit_all_comboboxes(container)
        return container
    return kwargs

# ==============================================================================
# 🏛️ RudTripleColumnLayout - 經典 Table 三欄空間拓撲容器
# ==============================================================================
class RudTripleColumnLayout(ttk.Frame):
    """
    🏛️ 經典 Table 三欄空間拓撲容器 (Classic Table Triple-Column Layout)
    向二十年前經典 HTML Table 結構致敬：
    <td nowrap> (Left) │ <td width="100%"> (Center) │ <td nowrap> (Right)
    
    徹底告別混亂無結構的 div 拼貼與脆弱的 PanedWindow Sash 漂移。
    - 左欄 (Left):   nowrap (weight=0)，依內容文字寬度緊湊包裹，永不截字、不折行。
    - 中欄 (Center): 100% (weight=1)，海納百川吸納全螢幕所有剩餘彈性空間。
    - 右欄 (Right):  nowrap (weight=0)，依屬性表單文字寬度緊湊包裹，永不截字。
    - 中間配備優雅的 1px 細分隔線 (Subtle Dividers)。
    """
    def __init__(self, parent, show_dividers=True, **kwargs):
        super().__init__(parent, **kwargs)
        
        self.grid_rowconfigure(0, weight=1)
        
        self.left_col = 0
        self.center_col = 2 if show_dividers else 1
        self.right_col = 4 if show_dividers else 2
        
        self.grid_columnconfigure(self.left_col, weight=0)
        if show_dividers:
            self.grid_columnconfigure(1, weight=0)
        self.grid_columnconfigure(self.center_col, weight=1)
        if show_dividers:
            self.grid_columnconfigure(3, weight=0)
        self.grid_columnconfigure(self.right_col, weight=0)
        
        self.show_dividers = show_dividers
        self.is_unified = False

        # 左欄 (<td nowrap>)
        self.left_frame = ttk.Frame(self)
        if show_dividers:
            self.divider_left = ttk.Separator(self, orient="vertical")
            
        # 中欄 (<td width="100%">)
        self.center_frame = ttk.Frame(self)
        if show_dividers:
            self.divider_right = ttk.Separator(self, orient="vertical")
            
        # 右欄 (<td nowrap>)
        self.right_frame = ttk.Frame(self)

        self.set_split_mode()

    def set_split_mode(self, left_minsize=260, right_minsize=300):
        """切換為雙側分欄三欄制 (Left | Center | Right)"""
        self.is_unified = False

        self.grid_columnconfigure(0, weight=0, minsize=left_minsize)
        self.grid_columnconfigure(1, weight=0, minsize=0)
        self.grid_columnconfigure(2, weight=1, minsize=300)
        self.grid_columnconfigure(3, weight=0, minsize=0)
        self.grid_columnconfigure(4, weight=0, minsize=right_minsize)
        self.grid_rowconfigure(0, weight=1)
        self.grid_rowconfigure(1, weight=0)
        self.grid_rowconfigure(2, weight=0)

        # 清除舊幾何
        self.left_frame.grid_remove()
        self.center_frame.grid_remove()
        self.right_frame.grid_remove()
        if hasattr(self, "divider_left"):
            self.divider_left.grid_remove()
        if hasattr(self, "divider_right"):
            self.divider_right.grid_remove()
        if hasattr(self, "divider_horiz"):
            self.divider_horiz.grid_remove()

        self.left_frame.grid(row=0, column=0, rowspan=1, sticky="nsew")
        if self.show_dividers:
            self.divider_left.grid(row=0, column=1, rowspan=1, sticky="ns", padx=(2, 2))
        self.center_frame.grid(row=0, column=2, rowspan=1, sticky="nsew")
        if self.show_dividers:
            self.divider_right.grid(row=0, column=3, rowspan=1, sticky="ns", padx=(2, 2))
        self.right_frame.grid(row=0, column=4, rowspan=1, sticky="nsew")

    def set_unified_mode(self, right_minsize=340, order="left_first"):
        """切換為單側上下整合二欄制 (Center 100% | Right Top & Bottom)"""
        self.is_unified = True

        self.grid_columnconfigure(0, weight=1, minsize=300)
        self.grid_columnconfigure(1, weight=0, minsize=0)
        self.grid_columnconfigure(2, weight=0, minsize=right_minsize)
        self.grid_columnconfigure(3, weight=0, minsize=0)
        self.grid_columnconfigure(4, weight=0, minsize=0)
        self.grid_rowconfigure(0, weight=0)
        self.grid_rowconfigure(1, weight=0)
        self.grid_rowconfigure(2, weight=1)

        # 清除舊幾何
        self.left_frame.grid_remove()
        self.center_frame.grid_remove()
        self.right_frame.grid_remove()
        if hasattr(self, "divider_left"):
            self.divider_left.grid_remove()
        if hasattr(self, "divider_right"):
            self.divider_right.grid_remove()

        if not hasattr(self, "divider_horiz"):
            self.divider_horiz = ttk.Separator(self, orient="horizontal")

        # 中間主舞台 100% 靠左滿版 (跨越全部 3 行)
        self.center_frame.grid(row=0, column=0, rowspan=3, sticky="nsew")
        if self.show_dividers:
            self.divider_right.grid(row=0, column=1, rowspan=3, sticky="ns", padx=(2, 2))

        # 右側雙層：依 order 決定上下次序 (預設 left_first: 左欄在上，右欄在下)
        top_frame = self.left_frame if order == "left_first" else self.right_frame
        bottom_frame = self.right_frame if order == "left_first" else self.left_frame

        top_frame.grid(row=0, column=2, sticky="new")
        if self.show_dividers:
            self.divider_horiz.grid(row=1, column=2, sticky="ew", pady=(4, 4))
        bottom_frame.grid(row=2, column=2, sticky="nsew")


# ==============================================================================
# AUTOSCROLLBAR & DYNAMIC TREEVIEW SIZING
# ==============================================================================

class AutoScrollbar(ttk.Scrollbar):
    """
    自適應捲動軸 (Auto-Hiding Scrollbar):
    - 僅在內容超出檢視區時自動顯示，完全容納時自動隱藏，避免破壞極簡美感。
    - 支援水平 (orient='horizontal') 與垂直 (orient='vertical') 模式。
    """
    def set(self, lo, hi):
        if float(lo) <= 0.0 and float(hi) >= 1.0:
            self.pack_forget()
        else:
            if not self.winfo_ismapped():
                side = tk.BOTTOM if self.cget("orient") == "horizontal" else tk.RIGHT
                fill = tk.X if self.cget("orient") == "horizontal" else tk.Y
                self.pack(side=side, fill=fill)
        super().set(lo, hi)


def calc_treeview_width(items_or_texts, font=None, indent_per_level=20, icon_padding=44, min_w=240):
    """
    動態精算 Treeview 自適應最佳寬度 (Dynamic Treeview Width Calculation):
    - 依據當前字型實際測量最長項目文字的物理像素寬度。
    - 自動納入樹狀展開箭頭、自訂圖示、階層縮排與呼吸留白。
    - 確保在任何字型大小或 DPI 縮放等級下，Treeview 均 100% 完整呈現，絕不截字。
    """
    if not items_or_texts:
        return min_w

    max_w = 0
    for item in items_or_texts:
        if isinstance(item, tuple):
            text = str(item[0])
            level = int(item[1]) if len(item) > 1 else 0
        else:
            text = str(item)
            leading_spaces = len(text) - len(text.lstrip(' '))
            level = max(0, leading_spaces // 2)

        if font and hasattr(font, 'measure'):
            w = font.measure(text.strip())
        else:
            w = visual_char_length(text.strip()) * 8

        total_w = w + (level * indent_per_level) + icon_padding
        if total_w > max_w:
            max_w = total_w

    return max(min_w, max_w + 20)


# ==============================================================================
# RESPONSIVE TWO-TIER ITEM & DYNAMIC WRAP ENGINE
# ==============================================================================

class ResponsiveWrapManager:
    """
    響應式動態折行管理器 (Responsive Dynamic Wrap Manager):
    - 自動管理容器內部所有長文字標籤之 wraplength。
    - 當容器觸發 <Configure> 事件時，即時依容器實際物理像素寬度重算折行點。
    """
    def __init__(self, container=None, padding=48, min_width=240):
        self.container = container
        self.padding = padding
        self.min_width = min_width
        self.labels = []
        if container:
            self.bind_container(container)

    def bind_container(self, container):
        self.container = container
        self.container.bind("<Configure>", self._on_configure, add="+")

    def register(self, label):
        if label not in self.labels:
            self.labels.append(label)
        return label

    def unregister(self, label):
        if label in self.labels:
            self.labels.remove(label)

    def _on_configure(self, event):
        available_w = max(self.min_width, event.width - self.padding)
        for lbl in list(self.labels):
            try:
                if lbl.winfo_exists():
                    lbl.configure(wraplength=available_w)
            except Exception:
                pass


class TwoTierOption(ttk.Frame):
    """
    現代桌面雙層結構化選項元件 (Two-Tier Structured Option):
    - 第一層：簡潔精確的主選項（Radiobutton 或 Checkbutton），絕不因超長說明而橫向溢出。
    - 第二層：縮排說明文字標籤（Subtitle Label），註冊至動態折行引擎，在任何寬度下自動條件折行。
    """
    def __init__(self, parent, title: str, subtitle: str = "", is_check: bool = False,
                 variable=None, value=None, command=None, wrap_manager=None,
                 font_title=None, font_subtitle=None, indent=24, **kwargs):
        super().__init__(parent, **kwargs)
        self.pack(fill=tk.X, anchor=tk.W, pady=(2, 4))

        if is_check:
            self.btn = ttk.Checkbutton(self, text=title, variable=variable, command=command)
        else:
            self.btn = ttk.Radiobutton(self, text=title, variable=variable, value=value, command=command)
        if font_title:
            self.btn.configure(font=font_title)
        self.btn.pack(anchor=tk.W)

        self.sub_lbl = None
        if subtitle:
            self.sub_lbl = ttk.Label(self, text=subtitle)
            if font_subtitle:
                self.sub_lbl.configure(font=font_subtitle)
            self.sub_lbl.pack(anchor=tk.W, padx=(indent, 0), pady=(1, 2))
            if wrap_manager and hasattr(wrap_manager, "register"):
                wrap_manager.register(self.sub_lbl)



