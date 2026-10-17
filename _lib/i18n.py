# -*- coding: utf-8 -*-
"""
==============================================================================
UNIVERSAL INTERNATIONALIZATION (i18n) SPREADSHEET ENGINE v2.0
公用試算表國際化與多語言核心模組
==============================================================================

[ 設計理念與試算表語言檔規範 (Spreadsheet Language Matrix Guidelines) ]

1. 【存放目錄 (Language Folder)】
   - 任何需要支援多語系的應用程式，只需在其所在資料夾下建立 `_lang` 子資料夾：
     例如：`C:\\ATGprjs\\MVlab\\Nyte\\DJ\\_lang\\`
   - 將試算表檔案命名為 `languages.tsv` (推薦 Tab 分隔) 或 `languages.csv`。

2. 【試算表直觀矩陣格式 (Tab-Separated Multi-Language Matrix)】
   - 可以直接用 Microsoft Excel、Google Sheets、LibreOffice Calc 或純文字編輯器開啟。
   - 第一列為標題列 (Header)，第 1 欄為 `KEY`，第 2 欄起為各語言代碼與顯示名稱：
     ```tsv
     KEY	zh_TW (繁體中文)	en_US (English)	zh_CN (简体中文)	ja_JP (日本語)	fr_FR (Français)
     app_title	DJ Nyte 2.0 - 智慧音樂廣播工作站	DJ Nyte 2.0 - Smart Broadcast Studio	DJ Nyte 2.0 - 智能音乐广播工作站	DJ Nyte 2.0 - スマート音楽放送	DJ Nyte 2.0 - Studio Musical
     menu_file	檔案 (F)	File (F)	文件 (F)	ファイル (F)	Fichier (F)
     file_new_tab	新增清單分頁	New Tab	新建列表标签页	新規タブ	Nouvel onglet
     ```
   - 支援 `#` 開頭的註解行與分組標籤（例如 `# --- 檔案選單 ---`），讓翻譯者輕鬆分類。

3. 【極簡擴充性 (Zero Configuration Extensibility)】
   - 使用者欲新增任何新語言（例如法文、韓文、德文、西班牙文等）：
     👉 只要在 `languages.tsv` 最右側新增一欄（例如：`ko_KR (한국어)`）並填入翻譯。
     👉 程式啟動時會自動偵測並將新語言加入主選單【語言 (Language)】與【偏好設定】！
   - 無需繁瑣的引號轉義，無括號巢狀語法錯誤風險。

4. 【快速使用範例 (Quick Start)】
   ```python
   from _lib.i18n import I18n, t, _
   
   # 初始化（自動尋找當前程式目錄下的 _lang 資料夾）
   I18n.init()
   
   # 取得翻譯
   print(t("app_title"))
   print(_("tab_confirm_close_msg", title="MyList"))
   
   # 切換語言
   I18n.set_language("en_US")
   ```
"""

import os
import sys
import csv
import re
import locale
import inspect
from typing import Dict, Any, Optional, List

class I18nManager:
    """
    通用試算表多語言管理器 (Universal Spreadsheet i18n Manager)
    負責掃描、解析與熱重載 _lang/languages.tsv 試算表。
    """
    def __init__(self, lang_dir: Optional[str] = None, default_lang: str = "zh_TW", fallback_lang: str = "zh_TW"):
        self.default_lang = default_lang
        self.fallback_lang = fallback_lang
        self.current_lang = default_lang
        self.lang_dir = lang_dir or self._detect_default_lang_dir()
        
        # 儲存結構: { "zh_TW": { "key": "value", ... }, "en_US": { ... } }
        self.translations: Dict[str, Dict[str, str]] = {}
        # 顯示名稱對照: { "zh_TW": "繁體中文 (Traditional Chinese)", "en_US": "English", ... }
        self.available_languages: Dict[str, str] = {}
        
        self.reload_languages()

    def _detect_default_lang_dir(self) -> str:
        """自動偵測呼叫端腳本所在的 _lang 資料夾路徑。"""
        try:
            for frame_info in inspect.stack():
                f_path = frame_info.filename
                if f_path and os.path.isabs(f_path) and not f_path.endswith("i18n.py"):
                    candidate = os.path.join(os.path.dirname(f_path), "_lang")
                    if os.path.exists(candidate):
                        return candidate
        except Exception:
            pass
        return os.path.join(os.getcwd(), "_lang")

    def set_lang_dir(self, lang_dir: str):
        """重新指定語言檔所在目錄並重新載入。"""
        self.lang_dir = os.path.abspath(lang_dir)
        self.reload_languages()

    def reload_languages(self):
        """掃描並解析 _lang 目錄下的 languages.tsv 或 languages.csv 試算表。"""
        self.translations.clear()
        self.available_languages.clear()

        if not self.lang_dir or not os.path.exists(self.lang_dir):
            return

        # 優先搜尋 languages.tsv，其次 languages.csv
        tsv_path = os.path.join(self.lang_dir, "languages.tsv")
        csv_path = os.path.join(self.lang_dir, "languages.csv")

        target_file = None
        delimiter = "\t"
        if os.path.exists(tsv_path):
            target_file = tsv_path
            delimiter = "\t"
        elif os.path.exists(csv_path):
            target_file = csv_path
            delimiter = ","

        if target_file:
            self._load_matrix_spreadsheet(target_file, delimiter)
        else:
            # 支援個別語系檔案 (例如 zh_TW.tsv, en_US.tsv)
            self._load_individual_spreadsheets()

    def _load_matrix_spreadsheet(self, filepath: str, delimiter: str = "\t"):
        """解析多欄式語言矩陣試算表。"""
        try:
            with open(filepath, "r", encoding="utf-8-sig") as f:
                reader = csv.reader(f, delimiter=delimiter)
                header = None
                lang_cols = []  # [(col_index, lang_code, display_name), ...]

                for row in reader:
                    if not row or len(row) == 0:
                        continue
                    # 忽略以 # 開頭的註解列
                    first_cell = row[0].strip()
                    if first_cell.startswith("#"):
                        continue

                    # 讀取標題列 (Header Row)
                    if header is None:
                        header = [c.strip() for c in row]
                        # 第 0 欄通常是 KEY
                        for idx in range(1, len(header)):
                            col_text = header[idx]
                            if not col_text:
                                continue
                            # 解析 "zh_TW (繁體中文)" 或 "zh_TW"
                            m = re.match(r"^([A-Za-z0-9_-]+)(?:\s*\((.*)\))?", col_text)
                            if m:
                                code = m.group(1)
                                label = col_text
                            else:
                                code = col_text
                                label = col_text
                            
                            lang_cols.append((idx, code, label))
                            self.translations[code] = {}
                            self.available_languages[code] = label
                        continue

                    # 資料列 (Data Rows)
                    key = first_cell
                    if not key:
                        continue

                    for col_idx, code, _ in lang_cols:
                        if col_idx < len(row):
                            val = row[col_idx]
                            # 處理換行轉義
                            val = val.replace("\\n", "\n").replace("\\t", "\t")
                            if val.strip():
                                self.translations[code][key] = val
        except Exception as e:
            print(f"[\u26a0\ufe0f i18n] Error reading spreadsheet {filepath}: {e}", file=sys.stderr)

    def _load_individual_spreadsheets(self):
        """讀取個別語系檔案 (例如 zh_TW.tsv)。"""
        for fname in os.listdir(self.lang_dir):
            if fname.lower().endswith((".tsv", ".csv")):
                code = os.path.splitext(fname)[0]
                delim = "\t" if fname.lower().endswith(".tsv") else ","
                fpath = os.path.join(self.lang_dir, fname)
                self.translations[code] = {}
                self.available_languages[code] = code
                try:
                    with open(fpath, "r", encoding="utf-8-sig") as f:
                        reader = csv.reader(f, delimiter=delim)
                        for row in reader:
                            if not row or len(row) < 2 or row[0].strip().startswith("#"):
                                continue
                            k = row[0].strip()
                            v = row[1].replace("\\n", "\n")
                            self.translations[code][k] = v
                except Exception as e:
                    print(f"[\u26a0\ufe0f i18n] Error reading {fname}: {e}", file=sys.stderr)

    def detect_system_language(self) -> str:
        """偵測作業系統語言並對應至已載入的語言代碼。"""
        try:
            loc = locale.getdefaultlocale()[0]
            if loc:
                loc_l = loc.lower()
                for code in self.translations.keys():
                    if code.lower() == loc_l:
                        return code
                if "zh_tw" in loc_l or "zh_hk" in loc_l or "zh_mo" in loc_l or "hant" in loc_l:
                    return "zh_TW"
                elif "zh" in loc_l or "hans" in loc_l:
                    return "zh_CN"
                elif "ja" in loc_l:
                    return "ja_JP"
                elif "en" in loc_l:
                    return "en_US"
        except Exception:
            pass
        return self.default_lang

    def set_language(self, lang_code: str):
        """切換當前語言（若傳入 'auto' 則自動偵測系統語言）。"""
        if lang_code == "auto":
            self.current_lang = self.detect_system_language()
        elif lang_code in self.translations:
            self.current_lang = lang_code
        else:
            self.current_lang = self.default_lang

    def get_language(self) -> str:
        """取得當前語言代碼。"""
        return self.current_lang

    def get_available_languages(self) -> Dict[str, str]:
        """取得所有已載入語言之 { 代碼: 顯示名稱 } 字典。"""
        return dict(self.available_languages)

    def t(self, key: str, **kwargs) -> str:
        """
        核心翻譯函式：
        1. 查詢 current_lang 中的 key。
        2. 若無，回退查詢 fallback_lang / default_lang。
        3. 若仍無，直接回傳 key 本身。
        4. 支援 str.format(**kwargs) 動態字串插值。
        """
        cur_dict = self.translations.get(self.current_lang, {})
        val = cur_dict.get(key)
        
        if val is None and self.current_lang != self.fallback_lang:
            fallback_dict = self.translations.get(self.fallback_lang, {})
            val = fallback_dict.get(key)
            
        if val is None and self.fallback_lang != self.default_lang:
            def_dict = self.translations.get(self.default_lang, {})
            val = def_dict.get(key)
            
        if val is None:
            val = key
            
        if kwargs and isinstance(val, str):
            try:
                return val.format(**kwargs)
            except Exception:
                return val
        return val

# ==============================================================================
# 全域預設單例與模組級別便捷函式 (Module-level API)
# ==============================================================================
_instance: Optional[I18nManager] = None

def get_instance() -> I18nManager:
    global _instance
    if _instance is None:
        _instance = I18nManager()
    return _instance

def init(lang_dir: Optional[str] = None, default_lang: str = "zh_TW", fallback_lang: str = "zh_TW") -> I18nManager:
    global _instance
    _instance = I18nManager(lang_dir=lang_dir, default_lang=default_lang, fallback_lang=fallback_lang)
    return _instance

def t(key: str, **kwargs) -> str:
    """取得翻譯字串。"""
    return get_instance().t(key, **kwargs)

# 短別名 _
_ = t

def set_language(lang_code: str):
    get_instance().set_language(lang_code)

def get_language() -> str:
    return get_instance().get_language()

def get_available_languages() -> Dict[str, str]:
    return get_instance().get_available_languages()

def detect_system_language() -> str:
    return get_instance().detect_system_language()

def reload_languages():
    get_instance().reload_languages()

# Export convenient class namespace
I18n = sys.modules[__name__]
