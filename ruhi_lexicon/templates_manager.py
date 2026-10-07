"""
Ruhi (如喜) Input Method Framework
ruhi_lexicon.templates_manager - 試算表 (TSV) 文字範本與巨集擴充管理引擎
"""

import os
import re
import datetime
from typing import Dict, List, Optional, Tuple

class TemplateItem:
    def __init__(self, trigger: str, expansion: str, description: str = ""):
        self.trigger = trigger.strip()
        self.expansion = expansion
        self.description = description.strip()

    def expand(self, clipboard_content: str = "") -> Tuple[str, int]:
        """
        執行範本展開與巨集替換
        返回: (展開後的完整文字, 游標定位偏移量 index，若無 %CURSOR% 則為 -1 表示末尾)
        """
        now = datetime.datetime.now()
        text = self.expansion
        
        # 1. 時間與日期巨集替換
        text = text.replace("%YYYY%", now.strftime("%Y"))
        text = text.replace("%YY%", now.strftime("%y"))
        text = text.replace("%MM%", now.strftime("%m"))
        text = text.replace("%M%", str(now.month))
        text = text.replace("%DD%", now.strftime("%d"))
        text = text.replace("%D%", str(now.day))
        text = text.replace("%HH%", now.strftime("%H"))
        text = text.replace("%NN%", now.strftime("%M")) # %NN% 分鐘
        text = text.replace("%SS%", now.strftime("%S"))
        text = text.replace("%DATE%", now.strftime("%Y年%m月%d日"))
        text = text.replace("%TIME%", now.strftime("%H:%M:%S"))

        # 2. 剪貼簿動態內容替換
        text = text.replace("%CLIPBOARD%", clipboard_content)

        # 3. 轉義字元與排版控制鍵轉換
        text = text.replace("\\n", "\n")
        text = text.replace("\\t", "\t")
        text = text.replace("{Tab}", "\t")
        text = text.replace("{Enter}", "\n")
        text = text.replace("{Shift+Enter}", "\n")
        text = text.replace("{Ctrl+Enter}", "\n")

        # 4. 游標定位解析 (%CURSOR%)
        cursor_pos = -1
        if "%CURSOR%" in text:
            cursor_pos = text.find("%CURSOR%")
            text = text.replace("%CURSOR%", "")

        return text, cursor_pos


class TemplatesManager:
    """文字範本管理器 (TSV 格式)"""
    def __init__(self, tsv_path: Optional[str] = None):
        self.templates: Dict[str, TemplateItem] = {}
        self.tsv_path = tsv_path
        if tsv_path and os.path.exists(tsv_path):
            self.load_from_tsv(tsv_path)

    def load_from_tsv(self, filepath: str):
        """從 TSV 試算表載入範本"""
        self.tsv_path = filepath
        self.templates.clear()
        if not os.path.exists(filepath):
            return

        with open(filepath, "r", encoding="utf-8") as f:
            for line_idx, line in enumerate(f):
                line = line.strip()
                if not line or line.startswith("#"):
                    continue
                # 跳過表頭
                if line_idx == 0 and ("TRIGGER" in line or "觸發" in line):
                    continue

                parts = line.split("\t")
                if len(parts) >= 2:
                    trigger = parts[0].strip()
                    expansion = parts[1]
                    desc = parts[2].strip() if len(parts) > 2 else ""
                    self.templates[trigger] = TemplateItem(trigger, expansion, desc)

    def save_to_tsv(self, filepath: Optional[str] = None):
        """將範本保存回 TSV 檔案"""
        target = filepath or self.tsv_path
        if not target:
            return
        os.makedirs(os.path.dirname(os.path.abspath(target)), exist_ok=True)
        with open(target, "w", encoding="utf-8") as f:
            f.write("TRIGGER\tEXPANSION\tDESCRIPTION\n")
            for item in self.templates.values():
                exp_clean = item.expansion.replace("\n", "\\n").replace("\t", "\\t")
                f.write(f"{item.trigger}\t{exp_clean}\t{item.description}\n")

    def match(self, trigger: str) -> Optional[TemplateItem]:
        """精確或前綴匹配範本"""
        return self.templates.get(trigger)

    def search(self, query: str) -> List[TemplateItem]:
        """搜尋包含 query 的文字範本"""
        q = query.lower()
        res = []
        for item in self.templates.values():
            if q in item.trigger.lower() or q in item.description.lower() or q in item.expansion.lower():
                res.append(item)
        return res
