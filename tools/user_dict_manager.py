#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜沙盒輸入座 (RuhI Sandbox Input Seat)
user_dict_manager.py — 使用者自建詞庫與刪除詞黑名單管理模組
=============================================================================
支援：
1. user_dict.tsv 讀寫、去重、權重置頂與 Toggle 刪除
2. deleted_dict.tsv 遮蔽詞黑名單管理（被刪除的內建詞與自建詞）
3. Tkinter/ttk 視覺化詞庫管理視窗 (UniversalUI 風格，支援深淺色自適應)
"""

import os
import time
from datetime import datetime
from typing import Dict, List, Optional, Set, Tuple
import tkinter as tk
from tkinter import ttk, font, messagebox


class UserDictEntry:
    def __init__(self, word: str, bopomofo: str, weight: int = 2000, category: str = "user", note: str = ""):
        self.word = word.strip()
        self.bopomofo = bopomofo.strip()
        self.weight = weight
        self.category = category.strip() or "user"
        self.note = note.strip()

    def to_tsv(self) -> str:
        return f"{self.word}\t{self.bopomofo}\t{self.weight}\t{self.category}\t{self.note}"


class DeletedEntry:
    def __init__(self, word: str, bopomofo: str = "", deleted_at: str = ""):
        self.word = word.strip()
        self.bopomofo = bopomofo.strip()
        self.deleted_at = deleted_at.strip() or datetime.now().strftime("%Y-%m-%d %H:%M:%S")

    def to_tsv(self) -> str:
        return f"{self.word}\t{self.bopomofo}\t{self.deleted_at}"


class UserDictStore:
    """
    自建詞庫與遮蔽清單資料管理
    """

    def __init__(self, user_dict_path: str, deleted_dict_path: str):
        self.user_dict_path = os.path.abspath(user_dict_path)
        self.deleted_dict_path = os.path.abspath(deleted_dict_path)

        # 記憶體快取
        # word -> UserDictEntry
        self.custom_words: Dict[str, UserDictEntry] = {}
        # word -> DeletedEntry
        self.deleted_words: Dict[str, DeletedEntry] = {}

        self.load_all()

    def load_all(self):
        """重新載入自建詞庫與黑名單"""
        self._load_user_dict()
        self._load_deleted_dict()

    def _load_user_dict(self):
        self.custom_words.clear()
        if not os.path.exists(self.user_dict_path):
            return

        try:
            with open(self.user_dict_path, "r", encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if not line or line.startswith("#") or line.startswith("WORD"):
                        continue
                    parts = line.split("\t")
                    if len(parts) >= 2:
                        word = parts[0].strip()
                        bpm = parts[1].strip()
                        weight = int(parts[2].strip()) if len(parts) > 2 and parts[2].strip().isdigit() else 2000
                        cat = parts[3].strip() if len(parts) > 3 else "user"
                        note = parts[4].strip() if len(parts) > 4 else ""
                        self.custom_words[word] = UserDictEntry(word, bpm, weight, cat, note)
        except Exception as e:
            print(f"[user_dict_manager] 載入 user_dict 失敗: {e}")

    def _load_deleted_dict(self):
        self.deleted_words.clear()
        if not os.path.exists(self.deleted_dict_path):
            return

        try:
            with open(self.deleted_dict_path, "r", encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if not line or line.startswith("#") or line.startswith("WORD"):
                        continue
                    parts = line.split("\t")
                    if len(parts) >= 1:
                        word = parts[0].strip()
                        bpm = parts[1].strip() if len(parts) > 1 else ""
                        dt = parts[2].strip() if len(parts) > 2 else ""
                        self.deleted_words[word] = DeletedEntry(word, bpm, dt)
        except Exception as e:
            print(f"[user_dict_manager] 載入 deleted_dict 失敗: {e}")

    def save_user_dict(self):
        """將自建詞寫入 TSV"""
        try:
            os.makedirs(os.path.dirname(self.user_dict_path), exist_ok=True)
            with open(self.user_dict_path, "w", encoding="utf-8", newline="\n") as f:
                f.write("# RuhI 使用者自建詞庫 (User Custom Lexicon)\n")
                f.write("WORD\tBOPOMOFO\tWEIGHT\tCATEGORY\tNOTE\n")
                for entry in self.custom_words.values():
                    f.write(entry.to_tsv() + "\n")
        except Exception as e:
            print(f"[user_dict_manager] 儲存 user_dict 失敗: {e}")

    def save_deleted_dict(self):
        """將黑名單寫入 TSV"""
        try:
            os.makedirs(os.path.dirname(self.deleted_dict_path), exist_ok=True)
            with open(self.deleted_dict_path, "w", encoding="utf-8", newline="\n") as f:
                f.write("# RuhI 已刪除／遮蔽詞清單 (Blacklist & Deleted Entries)\n")
                f.write("WORD\tBOPOMOFO\tDELETED_AT\n")
                for entry in self.deleted_words.values():
                    f.write(entry.to_tsv() + "\n")
        except Exception as e:
            print(f"[user_dict_manager] 儲存 deleted_dict 失敗: {e}")

    def toggle_word(self, word: str, bopomofo: str, weight: int = 2000) -> Tuple[str, str]:
        """
        Toggle 自建詞：
        - 若已存在於自建詞庫 -> 移除（或移至刪除清單），回傳 ("removed", word)
        - 若未存在 -> 新增進自建詞庫，並自刪除清單解封，回傳 ("added", word)
        """
        word = word.strip()
        bopomofo = bopomofo.strip()
        if not word:
            return ("error", "字詞不得為空")

        if word in self.custom_words:
            # 已存在 -> Toggle 移除
            old_entry = self.custom_words.pop(word)
            self.save_user_dict()
            # 記錄到刪除歷史
            self.deleted_words[word] = DeletedEntry(word, old_entry.bopomofo)
            self.save_deleted_dict()
            return ("removed", word)
        else:
            # 不存在 -> 新增
            # 若曾在黑名單中，解除封鎖
            if word in self.deleted_words:
                self.deleted_words.pop(word)
                self.save_deleted_dict()

            entry = UserDictEntry(
                word=word,
                bopomofo=bopomofo,
                weight=weight,
                category="user",
                note=datetime.now().strftime("%Y-%m-%d %H:%M")
            )
            self.custom_words[word] = entry
            self.save_user_dict()
            return ("added", word)

    def delete_word(self, word: str) -> bool:
        """自自建詞庫直接刪除字詞"""
        if word in self.custom_words:
            self.custom_words.pop(word)
            self.save_user_dict()
            return True
        return False

    def is_deleted(self, word: str) -> bool:
        """檢查字詞是否在遮蔽黑名單中"""
        return word in self.deleted_words

    def recover_deleted(self, word: str) -> bool:
        """從刪除清單恢復字詞"""
        if word in self.deleted_words:
            self.deleted_words.pop(word)
            self.save_deleted_dict()
            return True
        return False

    def permanent_delete(self, word: str) -> bool:
        """徹底自刪除清單中抹除紀錄"""
        if word in self.deleted_words:
            self.deleted_words.pop(word)
            self.save_deleted_dict()
            return True
        return False

    def add_word_with_semantic_analysis(
        self,
        word: str,
        context: str = "",
        bopomofo: str = "",
        force: bool = False,
        engine_bridge: Any = None
    ) -> Dict[str, Any]:
        """
        以 AI 語意分析師進行深度解毒與安全入庫：
        1. 錯字防火牆審查（攔截筆誤，防禦劫掠）
        2. 先驗權重歸一化（落實《嚴禁單字權重打地鼠守則》）
        3. 萃取二階語意引力場與防劫掠互斥罩
        4. 寫入自建詞庫與轉移矩陣，並記錄語意規則
        """
        from ruhi_brain.semantic_analyst import RuhiSemanticAnalyst
        analyst = RuhiSemanticAnalyst(
            user_dict_path=self.user_dict_path,
        )
        verdict = analyst.analyze_custom_word(
            word=word,
            context=context,
            user_bpm=bopomofo or None
        )
        if verdict.action_recommended == "WARN_POTENTIAL_TYPO" and not force:
            return {
                "success": False,
                "action": "blocked_typo",
                "verdict": verdict,
                "message": f"疑似同音筆誤：建議修正為『{verdict.typo_audit.suggested_correct}』"
            }

        apply_res = analyst.apply_verdict(verdict, commit_to_disk=True, engine_bridge=engine_bridge)
        # 重新載入快取
        self._load_user_dict()
        return {
            "success": True,
            "action": "added_safely",
            "verdict": verdict,
            "apply_result": apply_res
        }


# ============================================================================
# 視覺化管理中心視窗 (UserDictDialog)
# ============================================================================

class UserDictDialog(tk.Toplevel):
    """
    如喜詞庫管理中心視窗
    """

    def __init__(self, parent, store: UserDictStore, on_update_callback=None, palette=None, federation_manager=None):
        super().__init__(parent)
        self.store = store
        self.on_update = on_update_callback
        self.federation_manager = federation_manager
        self.palette = palette or {
            "is_dark": False,
            "bg_main": "#F7F5F0",
            "bg_card": "#FFFFFF",
            "ink_primary": "#1C1D21",
            "ink_secondary": "#4A4D57",
            "ink_muted": "#8E929D",
            "accent_blue": "#2563EB",
            "accent_commit": "#059669",
            "border": "#DDD7CD",
        }

        self.title("如喜詞庫與語意聯邦中心 (RuhI Lexicon & Semantic Federation)")
        self.geometry("820x550")
        self.minsize(680, 450)

        p = self.palette
        self.configure(bg=p["bg_main"])

        # 設定字體
        self.f_title = font.Font(family="Microsoft JhengHei UI", size=13, weight="bold")
        self.f_base = font.Font(family="Microsoft JhengHei UI", size=11)
        self.f_small = font.Font(family="Microsoft JhengHei UI", size=10)

        self._build_ui()
        self._load_data()

        # 快捷鍵
        self.bind("<Escape>", lambda e: self.destroy())
        self.protocol("WM_DELETE_WINDOW", self._on_close)

        # 視窗居中
        self.transient(parent)
        self.grab_set()

    def _build_ui(self):
        p = self.palette

        # 頂部標題與搜尋欄
        top_frame = tk.Frame(self, bg=p["bg_main"])
        top_frame.pack(fill="x", padx=16, pady=(14, 8))

        lbl_title = tk.Label(
            top_frame,
            text="✦ 詞庫與語意聯邦中心",
            font=self.f_title,
            bg=p["bg_main"],
            fg=p["ink_primary"]
        )
        lbl_title.pack(side="left")

        # 搜尋輸入框
        search_frame = tk.Frame(top_frame, bg=p["bg_main"])
        search_frame.pack(side="right")

        tk.Label(search_frame, text="🔍 搜尋：", font=self.f_small, bg=p["bg_main"], fg=p["ink_secondary"]).pack(side="left")
        self.var_search = tk.StringVar()
        self.var_search.trace_add("write", lambda *args: self._filter_data())
        ent_search = tk.Entry(
            search_frame,
            textvariable=self.var_search,
            font=self.f_small,
            bg=p["bg_card"],
            fg=p["ink_primary"],
            relief="solid",
            bd=1,
            width=20
        )
        ent_search.pack(side="left", padx=4)

        # Notebook (頁籤)
        style = ttk.Style()
        style.theme_use("default")
        style.configure("TNotebook", background=p["bg_main"], borderwidth=0)
        style.configure("TNotebook.Tab", background=p["bg_card"], foreground=p["ink_primary"], font=("Microsoft JhengHei UI", 10), padding=[12, 4])
        style.map("TNotebook.Tab", background=[("selected", p["accent_blue"])], foreground=[("selected", "#FFFFFF")])

        self.notebook = ttk.Notebook(self)
        self.notebook.pack(fill="both", expand=True, padx=16, pady=6)

        # Tab 1: 自建詞
        self.tab_user = tk.Frame(self.notebook, bg=p["bg_card"])
        self.notebook.add(self.tab_user, text="  使用者自建詞庫 (User Dict)  ")

        # Tab 2: 刪除/遮蔽黑名單
        self.tab_deleted = tk.Frame(self.notebook, bg=p["bg_card"])
        self.notebook.add(self.tab_deleted, text="  已刪除／遮蔽詞 (Blacklist)  ")

        # Tab 3: 語意聯邦蜂巢與隱私審計
        self.tab_federation = tk.Frame(self.notebook, bg=p["bg_card"])
        self.notebook.add(self.tab_federation, text="  語意聯邦蜂巢 (Zero-PII Hive)  ")

        self._build_tab_user(p)
        self._build_tab_deleted(p)
        self._build_tab_federation(p)

        # 底部按鈕列
        bottom_frame = tk.Frame(self, bg=p["bg_main"])
        bottom_frame.pack(fill="x", padx=16, pady=12)

        self.lbl_status = tk.Label(
            bottom_frame,
            text=f"自建詞共 {len(self.store.custom_words)} 筆 | 已遮蔽共 {len(self.store.deleted_words)} 筆",
            font=self.f_small,
            bg=p["bg_main"],
            fg=p["ink_muted"]
        )
        self.lbl_status.pack(side="left")

        btn_close = tk.Button(
            bottom_frame,
            text="關閉 (ESC)",
            font=self.f_small,
            bg=p["bg_card"],
            fg=p["ink_primary"],
            relief="groove",
            command=self._on_close,
            padx=12,
            pady=2
        )
        btn_close.pack(side="right")

    def _build_tab_user(self, p):
        frame = self.tab_user

        # Treeview
        tree_frame = tk.Frame(frame, bg=p["bg_card"])
        tree_frame.pack(fill="both", expand=True, padx=10, pady=10)

        cols = ("word", "bopomofo", "weight", "note")
        self.tree_user = ttk.Treeview(tree_frame, columns=cols, show="headings", selectmode="browse")
        self.tree_user.heading("word", text="詞彙")
        self.tree_user.heading("bopomofo", text="注音")
        self.tree_user.heading("weight", text="權重")
        self.tree_user.heading("note", text="備註 / 日期")

        self.tree_user.column("word", width=140, anchor="w")
        self.tree_user.column("bopomofo", width=180, anchor="w")
        self.tree_user.column("weight", width=80, anchor="center")
        self.tree_user.column("note", width=180, anchor="w")

        scroll = ttk.Scrollbar(tree_frame, orient="vertical", command=self.tree_user.yview)
        self.tree_user.configure(yscrollcommand=scroll.set)

        self.tree_user.pack(side="left", fill="both", expand=True)
        scroll.pack(side="right", fill="y")

        # 操作列
        op_frame = tk.Frame(frame, bg=p["bg_card"])
        op_frame.pack(fill="x", padx=10, pady=(0, 10))

        btn_del = tk.Button(
            op_frame,
            text="❌ 刪除選取詞",
            font=self.f_small,
            bg="#FEE2E2" if not p["is_dark"] else "#7F1D1D",
            fg="#DC2626" if not p["is_dark"] else "#FCA5A5",
            relief="flat",
            command=self._on_delete_user_word,
            padx=10, pady=3
        )
        btn_del.pack(side="left", padx=4)

        btn_boost = tk.Button(
            op_frame,
            text="⭐ 設為最高優先 (權重2000)",
            font=self.f_small,
            bg=p["bg_main"],
            fg=p["accent_blue"],
            relief="groove",
            command=self._on_boost_user_word,
            padx=10, pady=3
        )
        btn_boost.pack(side="left", padx=4)

    def _build_tab_deleted(self, p):
        frame = self.tab_deleted

        tree_frame = tk.Frame(frame, bg=p["bg_card"])
        tree_frame.pack(fill="both", expand=True, padx=10, pady=10)

        cols = ("word", "bopomofo", "deleted_at")
        self.tree_del = ttk.Treeview(tree_frame, columns=cols, show="headings", selectmode="browse")
        self.tree_del.heading("word", text="遮蔽詞彙")
        self.tree_del.heading("bopomofo", text="注音")
        self.tree_del.heading("deleted_at", text="遮蔽時間")

        self.tree_del.column("word", width=160, anchor="w")
        self.tree_del.column("bopomofo", width=200, anchor="w")
        self.tree_del.column("deleted_at", width=200, anchor="w")

        scroll = ttk.Scrollbar(tree_frame, orient="vertical", command=self.tree_del.yview)
        self.tree_del.configure(yscrollcommand=scroll.set)

        self.tree_del.pack(side="left", fill="both", expand=True)
        scroll.pack(side="right", fill="y")

        # 操作列
        op_frame = tk.Frame(frame, bg=p["bg_card"])
        op_frame.pack(fill="x", padx=10, pady=(0, 10))

        btn_recover = tk.Button(
            op_frame,
            text="🔄 恢復此詞 (解除遮蔽)",
            font=self.f_small,
            bg="#ECFDF5" if not p["is_dark"] else "#064E3B",
            fg="#059669" if not p["is_dark"] else "#6EE7B7",
            relief="flat",
            command=self._on_recover_deleted_word,
            padx=10, pady=3
        )
        btn_recover.pack(side="left", padx=4)

        btn_purge = tk.Button(
            op_frame,
            text="🗑️ 清除紀錄",
            font=self.f_small,
            bg=p["bg_main"],
            fg=p["ink_muted"],
            relief="groove",
            command=self._on_purge_deleted_word,
            padx=10, pady=3
        )
        btn_purge.pack(side="left", padx=4)

    def _build_tab_federation(self, p: dict):
        """建構 Tab 3：語意聯邦蜂巢與 Zero-PII 隱私審計面板"""
        frame = self.tab_federation

        # 頂部宣告說明卡片
        intro_frame = tk.Frame(frame, bg="#F0FDF4" if not p["is_dark"] else "#064E3B", bd=1, relief="solid")
        intro_frame.pack(fill="x", padx=10, pady=10)

        lbl_desc = tk.Label(
            intro_frame,
            text="🛡️ 隱私透明承諾 (Zero-PII Privacy)：\n"
                 "• 絕不上傳個人文本、對話紀錄、帳號密碼或未公開專有名詞。\n"
                 "• 僅提煉去識別化之中文語法結構轉移（如：程度副詞修飾形容詞）。\n"
                 "• 匿名回饋社群蜂巢，讓數萬台本機 AI 語意分析師協同進化。",
            font=self.f_small,
            bg="#F0FDF4" if not p["is_dark"] else "#064E3B",
            fg="#065F46" if not p["is_dark"] else "#A7F3D0",
            justify="left",
            anchor="w"
        )
        lbl_desc.pack(fill="x", padx=10, pady=8)

        # 暫存語意特徵列表
        tree_frame = tk.Frame(frame, bg=p["bg_card"])
        tree_frame.pack(fill="both", expand=True, padx=10, pady=(0, 10))

        cols = ("type", "prev", "curr", "weight", "timestamp")
        self.tree_fed = ttk.Treeview(tree_frame, columns=cols, show="headings", selectmode="browse")
        self.tree_fed.heading("type", text="特徵類型")
        self.tree_fed.heading("prev", text="前驅語法詞")
        self.tree_fed.heading("curr", text="接續語法詞")
        self.tree_fed.heading("weight", text="增量權重")
        self.tree_fed.heading("timestamp", text="提煉時間戳")

        self.tree_fed.column("type", width=140, anchor="w")
        self.tree_fed.column("prev", width=120, anchor="w")
        self.tree_fed.column("curr", width=120, anchor="w")
        self.tree_fed.column("weight", width=100, anchor="center")
        self.tree_fed.column("timestamp", width=140, anchor="center")

        scroll = ttk.Scrollbar(tree_frame, orient="vertical", command=self.tree_fed.yview)
        self.tree_fed.configure(yscrollcommand=scroll.set)

        self.tree_fed.pack(side="left", fill="both", expand=True)
        scroll.pack(side="right", fill="y")

        # 操作列
        op_frame = tk.Frame(frame, bg=p["bg_card"])
        op_frame.pack(fill="x", padx=10, pady=(0, 10))

        # Tailcat 去中心化通訊同意權 Checkbox
        self.var_tailcat = tk.BooleanVar(value=bool(getattr(self.federation_manager, 'tailcat_enabled', False)))
        chk_tailcat = tk.Checkbutton(
            op_frame,
            text="📡 授權 Tailcat 通訊同意權 (加入去中心化語意增益社群)",
            variable=self.var_tailcat,
            font=self.f_small,
            bg=p["bg_card"],
            fg=p["ink_primary"],
            activebackground=p["bg_card"],
            selectcolor=p["bg_main"],
            command=self._on_toggle_tailcat
        )
        chk_tailcat.pack(side="left", padx=4)

        btn_sync_tailcat = tk.Button(
            op_frame,
            text="🚀 立即廣播特徵",
            font=self.f_small,
            bg="#ECFDF5" if not p["is_dark"] else "#064E3B",
            fg="#059669" if not p["is_dark"] else "#6EE7B7",
            relief="flat",
            command=self._on_sync_tailcat,
            padx=8, pady=2
        )
        btn_sync_tailcat.pack(side="left", padx=6)

        btn_clear_fed = tk.Button(
            op_frame,
            text="🧹 清空暫存",
            font=self.f_small,
            bg=p["bg_main"],
            fg=p["ink_muted"],
            relief="groove",
            command=self._on_clear_federation,
            padx=8, pady=2
        )
        btn_clear_fed.pack(side="left", padx=4)

        self.lbl_fed_count = tk.Label(
            op_frame,
            text="目前暫存 0 筆純語法特徵",
            font=self.f_small,
            bg=p["bg_card"],
            fg=p["ink_secondary"]
        )
        self.lbl_fed_count.pack(side="right", padx=10)

    def _on_toggle_tailcat(self):
        enabled = self.var_tailcat.get()
        if self.federation_manager:
            self.federation_manager.set_tailcat_consent(enabled)
            status_str = "已開啟 Tailcat 通訊同意權，加入去中心化語意增益社群" if enabled else "已關閉 Tailcat 通訊，完全退出社群廣播並轉為本機離線私有"
            self.lbl_status.config(text=f"✦ Tailcat 狀態：{status_str}")

    def _on_sync_tailcat(self):
        if not self.federation_manager:
            return
        if not self.federation_manager.tailcat_enabled:
            messagebox.showinfo("Tailcat 通訊提示", "您目前未開啟 Tailcat 通訊同意權。\n\n請先勾選「授權 Tailcat 通訊同意權」以自願加入去中心化增益社群！")
            return
        count, msg = self.federation_manager.sync_to_tailcat_peer()
        self._load_data()
        messagebox.showinfo("Tailcat 同步結果", msg)

    def _on_clear_federation(self):
        if self.federation_manager:
            self.federation_manager.clear_staged_records()
            self._load_data()

    def _load_data(self):
        """填充資料到 Treeview"""
        q = self.var_search.get().strip().lower()

        # 1. 填入自建詞
        for item in self.tree_user.get_children():
            self.tree_user.delete(item)

        for word, entry in sorted(self.store.custom_words.items(), key=lambda x: -x[1].weight):
            if q and (q not in word.lower() and q not in entry.bopomofo.lower()):
                continue
            self.tree_user.insert("", "end", iid=word, values=(entry.word, entry.bopomofo, entry.weight, entry.note))

        # 2. 填入遮蔽詞
        for item in self.tree_del.get_children():
            self.tree_del.delete(item)

        for word, entry in sorted(self.store.deleted_words.items(), key=lambda x: x[1].deleted_at, reverse=True):
            if q and (q not in word.lower() and q not in entry.bopomofo.lower()):
                continue
            self.tree_del.insert("", "end", iid=word, values=(entry.word, entry.bopomofo, entry.deleted_at))

        # 3. 填入語意聯邦暫存特徵 (Zero-PII)
        if hasattr(self, 'tree_fed'):
            for item in self.tree_fed.get_children():
                self.tree_fed.delete(item)
            fed_records = self.federation_manager.get_staged_records() if self.federation_manager else []
            for r in reversed(fed_records):
                prev_w = r.get("prev", "")
                curr_w = r.get("curr", "")
                if q and (q not in prev_w.lower() and q not in curr_w.lower()):
                    continue
                ts = time.strftime("%Y-%m-%d %H:%M:%S", time.localtime(r.get("timestamp", time.time())))
                self.tree_fed.insert("", "end", values=(
                    r.get("type", "bigram_transition"),
                    prev_w,
                    curr_w,
                    f"+{r.get('weight', 1.0)}",
                    ts
                ))
            if hasattr(self, 'lbl_fed_count'):
                self.lbl_fed_count.config(text=f"目前暫存 {len(fed_records)} 筆純語法特徵（100% 本地透明審計）")

        self.lbl_status.config(
            text=f"自建詞共 {len(self.store.custom_words)} 筆 | 已遮蔽共 {len(self.store.deleted_words)} 筆"
        )

    def _filter_data(self):
        self._load_data()

    def _on_delete_user_word(self):
        sel = self.tree_user.selection()
        if not sel:
            return
        word = sel[0]
        if messagebox.askyesno("確認刪除", f"確定要自自建詞庫刪除『{word}』並加入遮蔽清單嗎？"):
            self.store.toggle_word(word, "")
            self._load_data()
            if self.on_update:
                self.on_update("deleted", word)

    def _on_boost_user_word(self):
        sel = self.tree_user.selection()
        if not sel:
            return
        word = sel[0]
        if word in self.store.custom_words:
            self.store.custom_words[word].weight = 2000
            self.store.save_user_dict()
            self._load_data()
            if self.on_update:
                self.on_update("boosted", word)

    def _on_recover_deleted_word(self):
        sel = self.tree_del.selection()
        if not sel:
            return
        word = sel[0]
        if self.store.recover_deleted(word):
            self._load_data()
            if self.on_update:
                self.on_update("recovered", word)

    def _on_purge_deleted_word(self):
        sel = self.tree_del.selection()
        if not sel:
            return
        word = sel[0]
        if self.store.permanent_delete(word):
            self._load_data()
            if self.on_update:
                self.on_update("purged", word)

    def _on_close(self):
        self.destroy()
