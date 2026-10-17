# -*- coding: utf-8 -*-
"""
==============================================================================
ATG TABLE ENGINE (檔案總管級現代化資料表格引擎)
==============================================================================
提供具備 Windows 檔案總管級別的資料表格能力：
- 拖曳排序 (Draggable Drag-and-Drop)
- F2 原地儲存格編輯 (Inline Cell Editing) 與 Alt+點擊批次編輯
- 多位數補零流水號自動替換 ([##1], (#01)) 與排程標籤快捷輸入
- 表頭右鍵選單 (自訂顯示欄位、最佳化欄寬)
- 雙色交替斑馬紋與極致鍵盤導航
"""

import os
import sys
import copy
import datetime
import tkinter as tk
from tkinter import ttk


def _i18n_text(key, default=""):
    try:
        from _lib import i18n
        res = i18n.t(key)
        if res and res != key:
            return res
    except Exception:
        pass
    return default


class DraggableTreeHelper:
    """
    Helper to enable 'Lego-like' visual drag-and-drop reordering/merging for ttk.Treeview.
    """
    def __init__(self, tree, on_drop_callback=None):
        self.tree = tree
        self.root = tree.winfo_toplevel()
        self.on_drop_callback = on_drop_callback
        self._drag_data = {"items": [], "y": 0, "ghost": None, "last_target_id": None, "auto_scroll_id": None}
        
        try:
            self.tree.tag_configure("drop_target", background="#007acc", foreground="white")
        except Exception:
            pass
            
        self.tree.bind("<ButtonPress-1>", self.on_drag_start, add='+')
        self.tree.bind("<B1-Motion>", self.on_drag_motion, add='+')
        self.tree.bind("<ButtonRelease-1>", self.on_drag_release, add='+')

    def on_drag_start(self, event):
        item = self.tree.identify_row(event.y)
        if item:
            self._drag_data["start_item"] = item
            self._drag_data["start_y"] = event.y
            self._drag_data["start_x"] = event.x
            self._drag_data["active"] = False
            self._drag_data["last_target_id"] = None
            
            is_ctrl = (event.state & 0x0004) != 0
            is_shift = (event.state & 0x0001) != 0
            
            current_selection = self.tree.selection()
            if item not in current_selection and not is_ctrl and not is_shift:
                self.tree.selection_set(item)
                self._drag_data["items"] = [item]
            else:
                self._drag_data["items"] = list(self.tree.selection())

    def on_drag_motion(self, event):
        if not self._drag_data.get("items"):
            return
            
        dy = abs(event.y - self._drag_data.get("start_y", event.y))
        dx = abs(event.x - self._drag_data.get("start_x", event.x))
        if not self._drag_data.get("active") and (dy > 5 or dx > 5):
            self._drag_data["active"] = True
            
        if self._drag_data.get("active"):
            self.tree.configure(cursor="hand2")
            target_id = self.tree.identify_row(event.y)
            
            for item in self.tree.get_children():
                tags = list(self.tree.item(item, "tags") or [])
                if "drop_target" in tags:
                    tags.remove("drop_target")
                    self.tree.item(item, tags=tags)
                    
            if target_id and target_id not in self._drag_data["items"]:
                self._drag_data["last_target_id"] = target_id
                tags = list(self.tree.item(target_id, "tags") or [])
                if "drop_target" not in tags:
                    tags.append("drop_target")
                    self.tree.item(target_id, tags=tags)

            self._drag_data["last_y"] = event.y
            tree_h = self.tree.winfo_height()
            margin = 30
            if event.y < margin:
                self._start_auto_scroll(-1)
            elif event.y > tree_h - margin:
                self._start_auto_scroll(1)
            else:
                self._stop_auto_scroll()

    def on_drag_release(self, event):
        for item in self.tree.get_children():
            tags = list(self.tree.item(item, "tags") or [])
            if "drop_target" in tags:
                tags.remove("drop_target")
                self.tree.item(item, tags=tags)

        if self._drag_data.get("ghost"):
            self._drag_data["ghost"].destroy()
            self._drag_data["ghost"] = None
        self.tree.configure(cursor="")
        self._stop_auto_scroll()
        
        if not self._drag_data.get("active"):
            start_item = self._drag_data.get("start_item")
            if start_item and self._drag_data.get("items"):
                self.tree.selection_set(start_item)
            self._drag_data = {"items": [], "y": 0, "ghost": None, "last_target_id": None, "auto_scroll_id": None}
            return
            
        source_ids = self._drag_data["items"]
        target_id = self._drag_data.get("last_target_id")
        self._drag_data = {"items": [], "y": 0, "ghost": None, "last_target_id": None, "auto_scroll_id": None}
        
        if target_id and target_id not in source_ids:
            if self.on_drop_callback:
                self.on_drop_callback(source_ids, target_id)

    def _start_auto_scroll(self, direction):
        if self._drag_data.get("auto_scroll_dir") == direction:
            return
        self._drag_data["auto_scroll_dir"] = direction
        self._auto_scroll_loop()

    def _stop_auto_scroll(self):
        self._drag_data["auto_scroll_dir"] = 0
        if self._drag_data.get("auto_scroll_id"):
            self.root.after_cancel(self._drag_data["auto_scroll_id"])
            self._drag_data["auto_scroll_id"] = None

    def _auto_scroll_loop(self):
        direction = self._drag_data.get("auto_scroll_dir", 0)
        if direction != 0:
            self.tree.yview_scroll(direction, "units")
            cur_y = self._drag_data.get("last_y", 0)
            target_id = self.tree.identify_row(cur_y)
            
            for item in self.tree.get_children():
                tags = list(self.tree.item(item, "tags") or [])
                if "drop_target" in tags:
                    tags.remove("drop_target")
                    self.tree.item(item, tags=tags)
                    
            if target_id and target_id not in self._drag_data.get("items", []):
                self._drag_data["last_target_id"] = target_id
                tags = list(self.tree.item(target_id, "tags") or [])
                if "drop_target" not in tags:
                    tags.append("drop_target")
                    self.tree.item(target_id, tags=tags)
                    
        self._drag_data["auto_scroll_id"] = self.root.after(50, self._auto_scroll_loop)


class UniversalTreeview(ttk.Treeview):
    """
    現代化多功能資料表格 (Universal Treeview):
    - 自動多欄排序 (Auto-sorting columns)
    - 視覺化拖曳重新排列 (Drag & Drop reordering)
    - 雙色交替斑馬紋 (Alternating row colors)
    - 檔案總管級 F2 原地編輯與流水號標籤補全
    - 表頭右鍵選單自訂顯示欄位與最適化欄寬
    """
    def __init__(self, parent, columns, draggable=True, **kwargs):
        super().__init__(parent, columns=columns, show="headings", **kwargs)
        
        self.columns = columns
        self.draggable = draggable
        self._current_sort_col = "#" if "#" in columns else None
        self._current_sort_desc = False
        
        for col in columns:
            init_txt = f"{col} ▲" if col == "#" else col
            self.heading(col, text=init_txt, command=lambda c=col: self.sort_by(c))
            self.column(col, width=100)
            
        self.tag_configure('odd', background='#252526')
        self.tag_configure('even', background='#1e1e1e')

        if self.draggable:
            self.drag_helper = DraggableTreeHelper(self, self.on_drag_drop_complete)

        self.bind("<Control-a>", self.select_all)
        self.bind("<Control-A>", self.select_all)
        self.bind("<Control-Home>", self.move_to_home)
        self.bind("<Control-End>", self.move_to_end)
        self.bind("<Control-Shift-Home>", self.extend_to_home)
        self.bind("<Control-Shift-End>", self.extend_to_end)
        
        self.bind("<F2>", self._start_inline_edit)
        self.bind("<Button-1>", self._on_tree_click_track_col, add="+")
        self.bind("<ButtonPress>", lambda e: self._hide_hint_tooltip(), add="+")
        self.bind("<Motion>", self._on_tree_motion_hint, add="+")
        self.bind("<Leave>", lambda e: self._hide_hint_tooltip(), add="+")

    def select_all(self, event=None):
        children = self.get_children()
        if children:
            self.selection_set(children)
        return "break"

    def move_to_home(self, event=None):
        children = self.get_children()
        if children:
            self.selection_set(children[0])
            self.see(children[0])
            self.focus(children[0])
        return "break"

    def move_to_end(self, event=None):
        children = self.get_children()
        if children:
            self.selection_set(children[-1])
            self.see(children[-1])
            self.focus(children[-1])
        return "break"

    def extend_to_home(self, event=None):
        children = self.get_children()
        sel = self.selection()
        if children and sel:
            idx = children.index(sel[-1])
            self.selection_set(children[0:idx+1])
            self.see(children[0])
        return "break"

    def extend_to_end(self, event=None):
        children = self.get_children()
        sel = self.selection()
        if children and sel:
            idx = children.index(sel[0])
            self.selection_set(children[idx:])
            self.see(children[-1])
        return "break"

    def refresh_stripes(self):
        for idx, item in enumerate(self.get_children()):
            tag = 'even' if idx % 2 == 0 else 'odd'
            tags = list(self.item(item, "tags") or [])
            tags = [t for t in tags if t not in ('even', 'odd')]
            tags.append(tag)
            self.item(item, tags=tags)

    def on_drag_drop_complete(self, source_ids, target_id):
        pass

    def _on_tree_motion_hint(self, event):
        sel = self.selection()
        if not sel or len(sel) <= 1:
            self._hide_hint_tooltip()
            return
            
        row_id = self.identify_row(event.y)
        if row_id and row_id in sel:
            if not getattr(self, '_hint_timer', None):
                self._hint_timer = self.after(600, lambda: self._show_hint_tooltip(event.x_root, event.y_root))
        else:
            self._hide_hint_tooltip()

    def _show_hint_tooltip(self, x_root, y_root):
        sel = self.selection()
        if not sel or len(sel) <= 1: return
        top = self.winfo_toplevel()
        is_dark = getattr(top, 'is_dark', True)
        s = getattr(top, 'scale_factor', 1.0)
        
        bg = "#1e2227" if is_dark else "#ffffff"
        fg = "#61afef" if is_dark else "#005a9e"
        txt_fg = "#abb2bf" if is_dark else "#333333"
        font_title = ("Microsoft JhengHei UI", max(9, int(9.5 * s)), "bold")
        font_body = ("Microsoft JhengHei UI", max(8, int(8.5 * s)))
        
        if getattr(self, '_hint_win', None) and self._hint_win.winfo_exists():
            try: self._hint_win.destroy()
            except Exception: pass
            
        self._hint_win = tk.Toplevel(top)
        self._hint_win.overrideredirect(True)
        self._hint_win.attributes("-topmost", True)
        try: self._hint_win.attributes("-alpha", 0.94)
        except Exception: pass
        self._hint_win.configure(bg="#3a3f4b", bd=1)
        
        card = tk.Frame(self._hint_win, bg=bg, padx=max(8, int(10 * s)), pady=max(6, int(8 * s)))
        card.pack(fill="both", expand=True)
        
        tk.Label(card, text=f"💡 批次編輯技巧 (已選取 {len(sel)} 首曲目)", bg=bg, fg=fg, font=font_title, anchor="w").pack(fill="x", pady=(0, 2))
        hint_text = (
            "• 按 F2 或 Alt+點擊欄位：批次修改選取曲目\n"
            "• 編輯中按 Tab / Shift+Tab：切換前/後欄位\n"
            "• 支援自動流水號：[##1] 或 (#01)（多位數補零）\n"
            "• 排程標籤支援：P1T1, P1E1, T1, E1, M, MG1O1\n"
            "• 支援方向鍵在文字中自由移動游標與選取"
        )
        tk.Label(card, text=hint_text, bg=bg, fg=txt_fg, font=font_body, justify="left", anchor="w").pack(fill="x")
        self._hint_win.geometry(f"+{x_root + 15}+{y_root + 15}")
        self._hint_win.bind("<Button-1>", lambda e: self._hide_hint_tooltip())
        top.bind("<FocusOut>", lambda e: self._hide_hint_tooltip(), add="+")
        top.bind("<Unmap>", lambda e: self._hide_hint_tooltip(), add="+")

    def _hide_hint_tooltip(self):
        if getattr(self, '_hint_timer', None):
            try: self.after_cancel(self._hint_timer)
            except Exception: pass
            self._hint_timer = None
        if getattr(self, '_hint_win', None) and self._hint_win.winfo_exists():
            try: self._hint_win.destroy()
            except Exception: pass
            self._hint_win = None

    def _on_tree_click_track_col(self, event):
        col = self.identify_column(event.x)
        if col:
            try:
                col_idx = int(col.replace('#', '')) - 1
                vis = self.get_visible_columns()
                if 0 <= col_idx < len(vis):
                    self._last_clicked_col = vis[col_idx]
            except Exception: pass

    def enable_inline_editing(self, default_col="檔案名稱", on_rename_callback=None, editable_columns=None):
        self.inline_edit_col = default_col
        self.on_rename_callback = on_rename_callback
        self.editable_columns = set(editable_columns) if editable_columns is not None else None
        self._hover_col = None
        self._hover_row_id = None
        
        self.bind("<Motion>", self._on_tree_motion, add="+")
        self.bind("<Alt-Button-1>", self._on_alt_click_inline_edit, add="+")
        self.bind("<Alt-1>", self._on_alt_click_inline_edit, add="+")
        self.bind("<Double-1>", self._on_double_click_inline_edit, add="+")
        self.bind("<F2>", self._start_inline_edit)

    def _on_tree_motion(self, event):
        row_id = self.identify_row(event.y)
        col_id = self.identify_column(event.x)
        if row_id and col_id:
            try:
                col_idx = int(col_id.replace('#', '')) - 1
                vis = self.get_visible_columns()
                if 0 <= col_idx < len(vis):
                    self._hover_col = vis[col_idx]
                    self._hover_row_id = row_id
                    return
            except Exception: pass
        self._hover_col = None
        self._hover_row_id = None

    def _on_alt_click_inline_edit(self, event):
        row_id = self.identify_row(event.y)
        col_id = self.identify_column(event.x)
        if not row_id or not col_id: return "break"
        
        try:
            col_idx = int(col_id.replace('#', '')) - 1
            vis = self.get_visible_columns()
            if 0 <= col_idx < len(vis):
                target_col = vis[col_idx]
                if self.editable_columns and target_col not in self.editable_columns:
                    return "break"
                self._start_inline_edit(target_col=target_col, anchor_row=row_id)
                return "break"
        except Exception: pass
        return "break"

    def _on_double_click_inline_edit(self, event):
        region = self.identify_region(event.x, event.y)
        if region != "cell": return
        
        col_id = self.identify_column(event.x)
        row_id = self.identify_row(event.y)
        if not col_id or not row_id: return
        
        try:
            col_idx = int(col_id.replace('#', '')) - 1
            vis = self.get_visible_columns()
            if 0 <= col_idx < len(vis):
                target_col = vis[col_idx]
                if self.editable_columns and target_col not in self.editable_columns:
                    return
                self.after(50, lambda: self._start_inline_edit(target_col=target_col, anchor_row=row_id))
        except Exception: pass

    def _start_inline_edit(self, event=None, target_col=None, anchor_row=None):
        sel = self.selection()
        if not sel: return
        
        if anchor_row and anchor_row in sel:
            anchor_id = anchor_row
        elif getattr(self, '_hover_row_id', None) and self._hover_row_id in sel:
            anchor_id = self._hover_row_id
        else:
            anchor_id = sel[0]
            
        vis_cols = self.get_visible_columns()
        col_name = None
        if target_col and target_col in vis_cols:
            col_name = target_col
        elif getattr(self, '_hover_col', None) and self._hover_col in vis_cols:
            col_name = self._hover_col
        elif getattr(self, '_last_clicked_col', None) and self._last_clicked_col in vis_cols:
            col_name = self._last_clicked_col
        else:
            col_name = getattr(self, 'inline_edit_col', '檔案名稱')
            if col_name not in vis_cols and vis_cols:
                col_name = vis_cols[0]
                
        if self.editable_columns and col_name not in self.editable_columns:
            return
            
        try:
            col_idx = vis_cols.index(col_name)
        except ValueError:
            return
            
        col_id = f"#{col_idx + 1}"
        bbox = self.bbox(anchor_id, col_id)
        if not bbox:
            self.see(anchor_id)
            bbox = self.bbox(anchor_id, col_id)
            if not bbox: return
            
        x, y, w, h = bbox
        cur_vals = self.item(anchor_id, "values")
        full_col_idx = self.columns.index(col_name) if col_name in self.columns else col_idx
        cur_val = cur_vals[full_col_idx] if full_col_idx < len(cur_vals) else ""
        
        entry = ttk.Entry(self, font=self.cget("font"))
        entry.insert(0, str(cur_val))
        entry.select_range(0, tk.END)
        entry.icursor(tk.END)
        entry.place(x=x, y=y, width=max(w, 80), height=h)
        entry.focus_set()
        
        def _finish(save=True, tab_delta=0):
            new_val = entry.get()
            entry.destroy()
            if save and self.on_rename_callback:
                self.on_rename_callback(list(sel), anchor_id, col_name, str(cur_val), new_val)
            if tab_delta != 0:
                self._switch_inline_col(anchor_id, col_name, tab_delta)
                
        entry.bind("<Return>", lambda e: _finish(True))
        entry.bind("<KP_Enter>", lambda e: _finish(True))
        entry.bind("<Escape>", lambda e: _finish(False))
        entry.bind("<FocusOut>", lambda e: _finish(True))
        entry.bind("<Tab>", lambda e: (_finish(True, 1), "break")[1])
        entry.bind("<Shift-Tab>", lambda e: (_finish(True, -1), "break")[1])

    def _switch_inline_col(self, anchor_id, cur_col, delta):
        vis = self.get_visible_columns()
        if not vis: return
        try:
            cur_idx = vis.index(cur_col)
            new_idx = (cur_idx + delta) % len(vis)
            next_col = vis[new_idx]
            if self.editable_columns and next_col not in self.editable_columns:
                self._switch_inline_col(anchor_id, next_col, delta)
                return
            self.after(20, lambda: self._start_inline_edit(target_col=next_col, anchor_row=anchor_id))
        except Exception: pass

    def sort_by(self, col):
        pass

    def get_column_widths(self):
        widths = {}
        for c in self.columns:
            try:
                w = self.column(c, 'width')
                if w: widths[c] = int(w)
            except Exception: pass
        return widths

    def set_column_widths(self, widths_dict):
        if not isinstance(widths_dict, dict): return
        for c, w in widths_dict.items():
            if c in self.columns:
                try: self.column(c, width=int(w))
                except Exception: pass

    def get_visible_columns(self):
        disp = self.cget("displaycolumns")
        if not disp or disp == "#all" or disp == ("#all",):
            return list(self.columns)
        return list(disp)

    def set_visible_columns(self, visible_cols):
        valid = [c for c in visible_cols if c in self.columns]
        if not valid: valid = list(self.columns)
        self.configure(displaycolumns=valid)

    def auto_fit_column(self, col, max_width=800, min_width=40, padding=25):
        try:
            f = tk.font.Font(font=self.cget("font"))
            h_text = self.heading(col, "text") or col
            max_w = f.measure(h_text) + padding
            full_idx = self.columns.index(col) if col in self.columns else -1
            
            if full_idx >= 0:
                for item in self.get_children():
                    vals = self.item(item, "values")
                    if vals and full_idx < len(vals):
                        v_str = str(vals[full_idx])
                        w = f.measure(v_str) + padding
                        if w > max_w: max_w = w
                        
            final_w = max(min_width, min(max_width, max_w))
            self.column(col, width=final_w)
        except Exception: pass

    def auto_fit_all_columns(self, max_width=800, min_width=40, padding=25):
        visible = self.get_visible_columns()
        for col in visible:
            self.auto_fit_column(col, max_width=max_width, min_width=min_width, padding=padding)
        if getattr(self, 'on_column_width_changed', None):
            try: self.on_column_width_changed()
            except Exception: pass

    def enable_header_context_menu(self, on_change_callback=None):
        self.header_change_callback = on_change_callback
        self.bind("<Button-3>", self._show_header_context_menu, add="+")

    def _show_header_context_menu(self, event):
        region = self.identify_region(event.x, event.y)
        if region != "heading": return
        
        top = self.winfo_toplevel()
        is_dark = getattr(top, 'is_dark', True)
        sel_color = "#00e5ff" if is_dark else "#007acc"
        s = getattr(top, 'scale_factor', 1.0)
        menu_font = ("Microsoft JhengHei UI", max(9, int(9.5 * s)))
        
        menu = tk.Menu(
            self,
            tearoff=0,
            bg=menu_bg,
            fg=menu_fg,
            activebackground=active_bg,
            activeforeground=active_fg,
            selectcolor=sel_color,
            relief="solid",
            bd=1,
            font=menu_font
        )
        
        menu.add_command(label="📐 最適化所有欄位寬度", command=self.auto_fit_all_columns)
        menu.add_separator()
        
        visible_set = set(self.get_visible_columns())
        self._col_vars = {}
        for col in self.columns:
            var = tk.BooleanVar(value=(col in visible_set))
            self._col_vars[col] = var
            
            def make_cmd(c_name, v):
                def cmd():
                    curr = list(self.get_visible_columns())
                    if v.get():
                        if c_name not in curr:
                            curr = [c for c in self.columns if c in curr or c == c_name]
                    else:
                        if len(curr) > 1 and c_name in curr:
                            curr.remove(c_name)
                        else:
                            v.set(True)
                    self.set_visible_columns(curr)
                    if getattr(self, 'header_change_callback', None):
                        self.header_change_callback(curr)
                return cmd
                
            heading_txt = self.heading(col, "text") or col
            menu.add_checkbutton(label=heading_txt, variable=var, command=make_cmd(col, var))
            
        try:
            menu.tk_popup(event.x_root, event.y_root)
        finally:
            menu.grab_release()
