# -*- coding: utf-8 -*-
"""
==============================================================================
I18N AUTOMATIC STRING EXTRACTOR & SPREADSHEET GENERATOR v1.0
多語言字串自動擷取與試算表生成工具
==============================================================================

[ 用途說明 ]
自動掃描舊 Python 程式碼，精準擷取所有 Tkinter / ttk 介面文字
（按鈕、標籤、選單項目、對話框提示、視窗標題等），
並一鍵自動產生標準的 `_lang/languages.tsv` 試算表！

[ 使用方式 ]
1. 終端機指令模式：
   python _lib/i18n_tool.py path/to/your_app.py

2. 程式碼匯入模式：
   from _lib.i18n_tool import extract_and_generate_tsv
   extract_and_generate_tsv("my_app.py", output_tsv="my_app/_lang/languages.tsv")
"""

import os
import sys
import re
import ast
import argparse
from typing import Dict, List, Tuple, Set

try:
    sys.stdout.reconfigure(encoding='utf-8')
    sys.stderr.reconfigure(encoding='utf-8')
except Exception:
    pass

# 常見標準中英文詞彙庫（自動預填常用翻譯）
COMMON_TRANSLATIONS = {
    # 常用動作
    "確定": ("OK", "确定", "OK"),
    "取消": ("Cancel", "取消", "キャンセル"),
    "儲存": ("Save", "保存", "保存"),
    "存檔": ("Save", "保存", "保存"),
    "另存新檔": ("Save As...", "另存为...", "名前を付けて保存..."),
    "另存新檔...": ("Save As...", "另存为...", "名前を付けて保存..."),
    "載入": ("Load", "加载", "開く"),
    "開啟": ("Open", "打开", "開く"),
    "關閉": ("Close", "关闭", "閉じる"),
    "刪除": ("Delete", "删除", "削除"),
    "清空": ("Clear", "清空", "クリア"),
    "複製": ("Duplicate", "复制", "複製"),
    "上移": ("Move Up", "上移", "上へ"),
    "下移": ("Move Down", "下移", "下へ"),
    "重設": ("Reset", "重置", "リセット"),
    "套用": ("Apply", "应用", "適用"),
    "瀏覽...": ("Browse...", "浏览...", "参照..."),
    "瀏覽": ("Browse", "浏览", "参照"),
    "結束": ("Exit", "退出", "終了"),
    "退出": ("Exit", "退出", "終了"),
    "說明": ("Help", "帮助", "ヘルプ"),
    "設定": ("Settings", "设置", "設定"),
    "偏好設定": ("Preferences...", "偏好设置...", "環境設定..."),
    "偏好設定...": ("Preferences...", "偏好设置...", "環境設定..."),
    "關於": ("About", "关于", "について"),
    "播放": ("Play", "播放", "再生"),
    "暫停": ("Pause", "暂停", "一時停止"),
    "停止": ("Stop", "停止", "停止"),
    "上一首": ("Previous Track", "上一首", "前の曲"),
    "下一首": ("Next Track", "下一首", "次の曲"),
    "提示": ("Notice", "提示", "通知"),
    "警告": ("Warning", "警告", "警告"),
    "錯誤": ("Error", "错误", "エラー"),
    "成功": ("Success", "成功", "成功"),
    "全選": ("Select All", "全选", "すべて選択"),
    "全取消": ("Deselect All", "全取消", "選択解除"),
    
    # 常用選單
    "檔案 (F)": ("File (F)", "文件 (F)", "ファイル (F)"),
    "編輯 (E)": ("Edit (E)", "编辑 (E)", "編集 (E)"),
    "檢視 (V)": ("View (V)", "视图 (V)", "表示 (V)"),
    "工具 (T)": ("Tools (T)", "工具 (T)", "ツール (T)"),
    "播放 (P)": ("Playback (P)", "播放 (P)", "再生 (P)"),
    "說明 (H)": ("Help (H)", "帮助 (H)", "ヘルプ (H)"),
    "語言 (Language)": ("Language", "语言 (Language)", "言語 (Language)"),
}

class TkinterStringExtractor(ast.NodeVisitor):
    def __init__(self):
        self.strings: List[Tuple[str, str, int]] = [] # (text, context_type, lineno)
        self.seen_texts: Set[str] = set()

    def _is_ui_text(self, s: str) -> bool:
        if not s or not isinstance(s, str):
            return False
        s_strip = s.strip()
        if len(s_strip) == 0:
            return False
        # 排除代碼型字串（路徑、色彩代碼、幾何形狀、格式名）
        if re.match(r"^#[0-9a-fA-F]{3,8}$", s_strip): return False
        if re.match(r"^\d+x\d+(\+\d+\+\d+)?$", s_strip): return False
        if re.match(r"^[a-zA-Z0-9_\-\./\\]+\.[a-zA-Z0-9]+$", s_strip): return False
        if s_strip.startswith(("http://", "https://", "file://", "c:\\", "c:/")): return False
        if s_strip.lower() in ("left", "right", "top", "bottom", "both", "x", "y", "none", "horizontal", "vertical", "flat", "solid", "groove", "ridge", "raised", "sunken", "disabled", "normal", "readonly", "w", "e", "n", "s", "nw", "ne", "sw", "se", "center"):
            return False
        # 包含中文或包含常見 UI 詞彙
        has_cjk = bool(re.search(r"[\u4e00-\u9fff]", s))
        has_symbols = bool(re.search(r"[📁📂💾🗑️❌⬆️⬇️🪄✂️✨📐🎛️📻⚙️⌨️ℹ️🔁🌙🚀🔗🖨️🎨📄]", s))
        return has_cjk or has_symbols or (len(s_strip) > 2 and any(c.isalpha() for c in s_strip))

    def visit_Call(self, node: ast.Call):
        # 1. 檢查關鍵字參數 (e.g. text="儲存", label="檔案", title="另存新檔")
        for kw in node.keywords:
            if kw.arg in ("text", "label", "title", "message"):
                if isinstance(kw.value, ast.Constant) and isinstance(kw.value.value, str):
                    val = kw.value.value
                    if self._is_ui_text(val):
                        self._add_string(val, kw.arg, node.lineno)

        # 2. 檢查對話框函式呼叫 (e.g. showinfo("標題", "內容"), askyesno(...))
        func_name = ""
        if isinstance(node.func, ast.Attribute):
            func_name = node.func.attr
        elif isinstance(node.func, ast.Name):
            func_name = node.func.id

        if func_name in ("showinfo", "showwarning", "showerror", "askyesno", "askokcancel", "show_toast", "add_command", "add_cascade"):
            for arg in node.args:
                if isinstance(arg, ast.Constant) and isinstance(arg.value, str):
                    val = arg.value
                    if self._is_ui_text(val):
                        self._add_string(val, func_name, node.lineno)

        self.generic_visit(node)

    def _add_string(self, text: str, context: str, lineno: int):
        clean_t = text.strip()
        if clean_t and clean_t not in self.seen_texts:
            self.seen_texts.add(clean_t)
            self.strings.append((clean_t, context, lineno))

def generate_key_name(text: str, context: str, index: int) -> str:
    """自動為文字產生語意清楚的 Key 鍵名。"""
    # 移除 Emoji 與符號
    clean = re.sub(r"[^\w\s\u4e00-\u9fff]", "", text).strip()
    
    # 根據 context 決定字首前綴
    prefix = "txt"
    if "menu" in context or "add_command" in context or "add_cascade" in context or "label" in context:
        prefix = "menu" if "menu" in context or "add_cascade" in context else "file" if "file" in clean.lower() else "item"
    elif "btn" in context or "Button" in context or "text" in context:
        prefix = "btn"
    elif "title" in context:
        prefix = "dlg" if "對話" in text or "清單" in text or "設定" in text else "title"
    elif "info" in context or "warn" in context or "error" in context or "msg" in context:
        prefix = "msg"
        
    # 拼音或拼字轉換簡短標識
    # 簡單以 index 配合簡短識別
    safe_suffix = re.sub(r"\s+", "_", clean.lower())
    safe_suffix = re.sub(r"[^\x00-\x7F]+", "", safe_suffix) # 移除中文字保留純英數字
    if not safe_suffix:
        safe_suffix = f"item_{index:03d}"
    else:
        safe_suffix = safe_suffix[:20]
        
    return f"{prefix}_{safe_suffix}"

def extract_and_generate_tsv(py_filepath: str, output_tsv: Optional[str] = None) -> str:
    """
    掃描目標 Python 檔案並輸出標準 TSV 試算表。
    """
    if not os.path.exists(py_filepath):
        raise FileNotFoundError(f"找不到檔案：{py_filepath}")

    with open(py_filepath, "r", encoding="utf-8", errors="ignore") as f:
        code = f.read()

    tree = ast.parse(code)
    extractor = TkinterStringExtractor()
    extractor.visit(tree)

    # 決定輸出路徑
    if not output_tsv:
        target_dir = os.path.join(os.path.dirname(os.path.abspath(py_filepath)), "_lang")
        os.makedirs(target_dir, exist_ok=True)
        output_tsv = os.path.join(target_dir, "languages.tsv")
    else:
        os.makedirs(os.path.dirname(os.path.abspath(output_tsv)), exist_ok=True)

    tsv_lines = [
        "# ====================================================================================================",
        f"# 自動擷取自：{os.path.basename(py_filepath)}",
        "# 格式說明：Tab 分隔 (TSV)，可直接使用 Excel、Google Sheets 或純文字編輯器開啟編輯。",
        "# ====================================================================================================",
        "KEY\tzh_TW (繁體中文)\ten_US (English)\tzh_CN (简体中文)\tja_JP (日本語)",
        ""
    ]

    used_keys = set()
    for idx, (text, ctx, lineno) in enumerate(extractor.strings, 1):
        # 產生唯一 Key
        base_key = generate_key_name(text, ctx, idx)
        key = base_key
        counter = 1
        while key in used_keys:
            key = f"{base_key}_{counter}"
            counter += 1
        used_keys.add(key)

        # 查詢是否有內建翻譯
        clean_text_no_emoji = re.sub(r"^[📁📂💾🗑️❌⬆️⬇️🪄✂️✨📐🎛️📻⚙️⌨️ℹ️🔁🌙🚀🔗🖨️🎨📄✓☑️◻️\s]+", "", text).strip()
        trans = COMMON_TRANSLATIONS.get(text) or COMMON_TRANSLATIONS.get(clean_text_no_emoji)
        
        if trans:
            en_val, cn_val, jp_val = trans
        else:
            en_val, cn_val, jp_val = "", "", ""

        # 轉義換行
        safe_tw = text.replace("\n", "\\n").replace("\t", "\\t")
        safe_en = en_val.replace("\n", "\\n").replace("\t", "\\t")
        safe_cn = cn_val.replace("\n", "\\n").replace("\t", "\\t")
        safe_jp = jp_val.replace("\n", "\\n").replace("\t", "\\t")

        tsv_lines.append(f"{key}\t{safe_tw}\t{safe_en}\t{safe_cn}\t{safe_jp}")

    with open(output_tsv, "w", encoding="utf-8") as f:
        f.write("\n".join(tsv_lines) + "\n")

    print(f"✅ 成功自 {os.path.basename(py_filepath)} 擷取 {len(extractor.strings)} 個介面字串！")
    print(f"📊 試算表已輸出至：{output_tsv}")
    return output_tsv

def main():
    parser = argparse.ArgumentParser(description="Tkinter UI 字串自動擷取與多語言試算表生成工具")
    parser.add_argument("file", help="欲掃描擷取字串的 Python 原始碼路徑")
    parser.add_argument("-o", "--output", help="自訂輸出的 TSV 檔案路徑", default=None)
    args = parser.parse_args()

    extract_and_generate_tsv(args.file, args.output)

if __name__ == "__main__":
    if len(sys.argv) > 1:
        main()
    else:
        print("💡 請提供欲掃描的 Python 檔案路徑，例如：")
        print("   python _lib/i18n_tool.py path/to/my_app.py")
