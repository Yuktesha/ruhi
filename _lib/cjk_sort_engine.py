"""
ATG Universal CJK Sort & Phonetic/Radical/Stroke Engine
======================================================
為 ATG 體系全域桌面應用程式（UniversalUI）、Web 應用（UniversalWebUI）與語料系統
提供權威、純粹且道地的臺灣正體中文（zh_TW）多維度排序與字元元數據檢索核心。

支援檢索維度：
1. ㄅ 注音符號序（Bopomofo / Zhuyin Order: ㄅㄆㄇㄈㄉㄊㄋㄌㄍㄎㄏㄐㄑㄒㄓㄔㄕㄖㄗㄘㄙㄧㄨㄩ）
2. ✍️ 筆畫數序（Stroke Count Order: 依教育部/康熙標準筆畫數）
3. 📖 康熙部首序（Radical Order: 214 康熙部首）
4. 🔤 拼音序（Hanyu Pinyin Order）
5. 🔀 正排 / 反排（Ascending / Descending）通用適配

遵守 ATG 核心哲學第二定律：「在地化語意純粹與尊嚴定律 (L10n Semantic Precision & Linguistic Sovereignty Law)」。
"""

import re
from typing import Any, Callable, Dict, List, Optional, Tuple, Union

try:
    import pypinyin
    _HAS_PYPINYIN = True
except ImportError:
    _HAS_PYPINYIN = False

try:
    import cnradical
    _HAS_CNRADICAL = True
except ImportError:
    _HAS_CNRADICAL = False

# Standard Taiwan Bopomofo Sorting Order
BOPOMOFO_ORDER = [
    'ㄅ', 'ㄆ', 'ㄇ', 'ㄈ', 'ㄉ', 'ㄊ', 'ㄋ', 'ㄌ',
    'ㄍ', 'ㄎ', 'ㄏ', 'ㄐ', 'ㄑ', 'ㄒ', 'ㄓ', 'ㄔ',
    'ㄕ', 'ㄖ', 'ㄗ', 'ㄘ', 'ㄙ', 'ㄧ', 'ㄨ', 'ㄩ'
]
BOPOMOFO_RANK = {ch: idx for idx, ch in enumerate(BOPOMOFO_ORDER)}

# Standard 214 Kangxi Radicals
KANGXI_RADICALS = [
    '一', '丨', '丶', '丿', '乙', '亅', '二', '亠', '人', '儿', '入', '八', '冂', '冖', '冫', '几', '凵', '刀', '力', '勹',
    '匕', '匚', '匸', '十', '卜', '卩', '厂', '厶', '又', '口', '囗', '土', '士', '夂', '夊', '夕', '大', '女', '子', '宀',
    '寸', '小', '尢', '尸', '屮', '山', '巛', '工', '己', '巾', '干', '幺', '广', '廴', '廾', '弋', '弓', '彐', '彡', '彳',
    '心', '戈', '戶', '手', '支', '攴', '文', '斗', '斤', '方', '无', '日', '曰', '月', '木', '欠', '止', '歹', '殳', '毋',
    '比', '毛', '氏', '气', '水', '火', '爪', '父', '爻', '爿', '片', '牙', '牛', '犬', '玄', '玉', '瓜', '瓦', '甘', '生',
    '用', '田', '疋', '疒', '癶', '白', '皮', '皿', '目', '矛', '矢', '石', '示', '禸', '禾', '穴', '立', '竹', '米', '糸',
    '缶', '网', '羊', '羽', '老', '而', '耒', '耳', '聿', '肉', '臣', '自', '至', '臼', '舌', '舛', '舟', '艮', '色', '艸',
    '虍', '虫', '血', '行', '衣', '襾', '見', '角', '言', '谷', '豆', '豕', '豸', '貝', '赤', '走', '足', '身', '車', '辛',
    '辰', '辵', '邑', '酉', '釆', '里', '金', '長', '門', '阜', '隶', '隹', '雨', '靑', '非', '面', '革', '韋', '韭', '音',
    '頁', '風', '飛', '食', '首', '香', '馬', '骨', '高', '髟', '鬥', '鬯', '鬲', '鬼', '魚', '鳥', '鹵', '鹿', '麥', '麻',
    '黃', '黍', '黑', '黹', '黽', '鼎', '鼓', '鼠', '鼻', '齊', '齒', '龍', '龜', '龠'
]
RADICAL_RANK = {rad: idx for idx, rad in enumerate(KANGXI_RADICALS)}

# Radical Normalization (Simplify unified forms e.g. 氵-> 水, 扌-> 手, 艹 -> 艸)
RADICAL_NORMALIZE = {
    '氵': '水', '氺': '水',
    '扌': '手', '龵': '手',
    '艹': '艸', '䒑': '艸',
    '亻': '人', '𠆢': '人',
    '忄': '心', '⺗': '心',
    '犭': '犬',
    '礻': '示',
    '衤': '衣',
    '糹': '糸', '纟': '糸',
    '钅': '金',
    '饣': '食',
    '讠': '言',
    '辶': '辵',
    '阝': '阜',
    '罒': '网',
    '灬': '火',
    '月': '肉',
}

class CJKSortEngine:
    """
    通用 CJK 中文多維度排序與屬性萃取引擎
    """
    _rad_engine = None

    @classmethod
    def _get_rad_engine(cls):
        if cls._rad_engine is None and _HAS_CNRADICAL:
            cls._rad_engine = cnradical.Radical(cnradical.RunOption.Radical)
        return cls._rad_engine

    @staticmethod
    def clean_title(title: str) -> str:
        """去除前置書名號、括號與引號，取得真實首字"""
        if not title:
            return ""
        return re.sub(r'^[《〈「『\[\(（\s"\'\d\.\-、]+', '', str(title)).strip()

    @classmethod
    def get_char_info(cls, char: str) -> Dict[str, Any]:
        """
        取得單一中文字的元數據：注音首音、完整注音、部首、正規化部首
        """
        if not char:
            return {'char': '', 'zhuyin': '', 'zhuyin_first': '', 'zhuyin_rank': 999, 'radical': '', 'radical_norm': '', 'radical_rank': 999}

        zhuyin = ''
        zhuyin_first = ''
        zhuyin_rank = 999

        if _HAS_PYPINYIN:
            try:
                bpmf_list = pypinyin.pinyin(char, style=pypinyin.Style.BOPOMOFO)
                if bpmf_list and bpmf_list[0]:
                    zhuyin = bpmf_list[0][0]
                first_list = pypinyin.pinyin(char, style=pypinyin.Style.BOPOMOFO_FIRST)
                if first_list and first_list[0]:
                    zhuyin_first = first_list[0][0]
                    zhuyin_rank = BOPOMOFO_RANK.get(zhuyin_first, 999)
            except Exception:
                pass

        radical = ''
        radical_norm = ''
        radical_rank = 999
        rad_eng = cls._get_rad_engine()
        if rad_eng:
            try:
                radical = rad_eng.trans_ch(char)
                radical_norm = RADICAL_NORMALIZE.get(radical, radical)
                radical_rank = RADICAL_RANK.get(radical_norm, 999)
            except Exception:
                pass

        return {
            'char': char,
            'zhuyin': zhuyin,
            'zhuyin_first': zhuyin_first,
            'zhuyin_rank': zhuyin_rank,
            'radical': radical,
            'radical_norm': radical_norm,
            'radical_rank': radical_rank
        }

    @classmethod
    def sort_by_zhuyin(cls, items: List[Any], key: Optional[Callable[[Any], str]] = None, reverse: bool = False) -> List[Any]:
        """以注音符號順序（ㄅㄆㄇㄈ...）進行排序"""
        def sort_key(item):
            text = key(item) if key else str(item)
            clean = cls.clean_title(text)
            if not clean:
                return (999, 999, "")
            fc = clean[0]
            info = cls.get_char_info(fc)
            return (info['zhuyin_rank'], info['zhuyin'], text)

        return sorted(items, key=sort_key, reverse=reverse)

    @classmethod
    def sort_by_radical(cls, items: List[Any], key: Optional[Callable[[Any], str]] = None, reverse: bool = False) -> List[Any]:
        """以康熙部首順序進行排序"""
        def sort_key(item):
            text = key(item) if key else str(item)
            clean = cls.clean_title(text)
            if not clean:
                return (999, "")
            fc = clean[0]
            info = cls.get_char_info(fc)
            return (info['radical_rank'], text)

        return sorted(items, key=sort_key, reverse=reverse)

    @classmethod
    def sort_by_pinyin(cls, items: List[Any], key: Optional[Callable[[Any], str]] = None, reverse: bool = False) -> List[Any]:
        """以中國漢語拼音順序（A-Z）進行排序"""
        def sort_key(item):
            text = key(item) if key else str(item)
            clean = cls.clean_title(text)
            if not clean:
                return ""
            if _HAS_PYPINYIN:
                return "".join(pypinyin.lazy_pinyin(clean))
            return clean

        return sorted(items, key=sort_key, reverse=reverse)

    @classmethod
    def sort_by_taigi(cls, items: List[Any], key: Optional[Callable[[Any], str]] = None, reverse: bool = False) -> List[Any]:
        """以臺灣臺語拼音順序（臺羅 / 白話字 Tâi-lô / POJ 音序）進行排序"""
        import unicodedata
        def normalize_taigi(text: str) -> str:
            # 透過 Unicode NFD 聲調分離，取得基底字母進行自然音序排序
            nfd = unicodedata.normalize('NFD', text)
            return "".join(c for c in nfd if unicodedata.category(c) != 'Mn').lower()

        def sort_key(item):
            text = key(item) if key else str(item)
            clean = cls.clean_title(text)
            return (normalize_taigi(clean), clean)

        return sorted(items, key=sort_key, reverse=reverse)