#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜 (RuhI) 龍龕形音輸入法 (Longkan Morpho-Phonetic 4-Code IME) 核心碼表編譯器
================================================================================
源自西元 997 年遼代高僧行均《龍龕手鑑》「打破說文死板部首、依視覺所見字形歸部並結合四聲檢字」之平民實用精神，
結合現代計算語言學與資訊理論（實測單字資訊熵達 13.70 bits、雙字詞連打唯一率 98.79%）：

1. 一律補／縮為 4 碼定長框架（不足 4 碼時可按 Space 直接收攏，或補聲調碼滿 4 碼）：
   - 軌道 A【有聲部首(2碼) + 聲符/部件音(2碼)】：
     同時相容「國際羅馬拼音／台羅古音 2 碼」(如 鋪 = 金km + 甫fu -> kmfu；龕 = 合he + 龍lo -> helo/holg)
     與「台灣許氏雙拼 2 碼」(如 鋪 = 金jn + 甫fx -> jnfx；龕 = 合hg + 龍ll -> hgll)。
   - 軌道 B【英文字母視覺象形(1碼) + 聲符/部件音(2碼) + 第4碼補碼(Space / 12345 / dfjs)】：
     針對「肴」(乂x + 有eo -> xeo / xeo2 / xeo3 / xeof)、「凶」(乂x + 凵u -> xu / xu1)、
     「區」(匚c + 品pn -> cpn / cpn3 / cpnf)、「同」(冂n + 同to -> nto / nto2) 等視覺象形字頭。
   - 軌道 C【部件 × 數字 (Component Multiplicity C × N)】：
     針對重複部件字，如「區 = 匚(c) + 口(ko)×3 -> cko3」、「品 = 口(ko)×3 -> ko3 / ko33 / ko3f」、
     「森 = 木(mu)×3 -> mu3 / mu31」、「林 = 木(mu)×2 -> mu2 / mu22」、「晶 = 日(ri)×3 -> ri3 / ri31」、
     「協 = 十(t) + 力(li)×3 -> tli3 / tle3」、「蕊 = 艹(w) + 心(sm)×3 -> wsm3 / wxn3」。
   - 軌道 D【高頻雙字詞與成語 4 碼簡碼】：
     雙字詞取「首字前2碼 + 尾字前2碼」，四字成語取「四字首碼」。
"""

from __future__ import annotations

import os
import re
import sys
import zipfile
from collections import Counter, defaultdict
from pathlib import Path
from typing import Dict, List, Optional, Set, Tuple

if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")
    except Exception:
        pass

REPO_ROOT = Path(__file__).resolve().parent.parent
UNIHAN_ZIP = REPO_ROOT / "_archive" / "_external" / "four_corner_upstream" / "Unihan.zip"
IDS_TXT = REPO_ROOT / "_archive" / "_external" / "ids.txt"
LEXICON_DIR = REPO_ROOT / "ruhi_lexicon" / "data"

KANGXI_RADICALS: Dict[int, str] = {
    1: "一", 2: "丨", 3: "丶", 4: "丿", 5: "乙", 6: "亅",
    7: "二", 8: "亠", 9: "人", 10: "儿", 11: "入", 12: "八",
    13: "冂", 14: "冖", 15: "冰", 16: "几", 17: "凵", 18: "刀",
    19: "力", 20: "勹", 21: "匕", 22: "匚", 23: "匸", 24: "十",
    25: "卜", 26: "卩", 27: "厂", 28: "厶", 29: "又", 30: "口",
    31: "囗", 32: "土", 33: "士", 34: "夂", 35: "夊", 36: "夕",
    37: "大", 38: "女", 39: "子", 40: "宀", 41: "寸", 42: "小",
    43: "尢", 44: "尸", 45: "屮", 46: "山", 47: "川", 48: "工",
    49: "己", 50: "巾", 51: "干", 52: "幺", 53: "广", 54: "廴",
    55: "廾", 56: "弋", 57: "弓", 58: "彐", 59: "彡", 60: "彳",
    61: "心", 62: "戈", 63: "戶", 64: "手", 65: "支", 66: "攴",
    67: "文", 68: "斗", 69: "斤", 70: "方", 71: "无", 72: "日",
    73: "曰", 74: "月", 75: "木", 76: "欠", 77: "止", 78: "歹",
    79: "殳", 80: "毋", 81: "比", 82: "毛", 83: "氏", 84: "气",
    85: "水", 86: "火", 87: "爪", 88: "父", 89: "爻", 90: "爿",
    91: "片", 92: "牙", 93: "牛", 94: "犬", 95: "玄", 96: "玉",
    97: "瓜", 98: "瓦", 99: "甘", 100: "生", 101: "用", 102: "田",
    103: "疋", 104: "疒", 105: "癶", 106: "白", 107: "皮", 108: "皿",
    109: "目", 110: "矛", 111: "矢", 112: "石", 113: "示", 114: "禸",
    115: "禾", 116: "穴", 117: "立", 118: "竹", 119: "米", 120: "糸",
    121: "缶", 122: "网", 123: "羊", 124: "羽", 125: "老", 126: "而",
    127: "耒", 128: "耳", 129: "聿", 130: "肉", 131: "臣", 132: "自",
    133: "至", 134: "臼", 135: "舌", 136: "舛", 137: "舟", 138: "艮",
    139: "色", 140: "艸", 141: "虍", 142: "虫", 143: "血", 144: "行",
    145: "衣", 146: "襾", 147: "見", 148: "角", 149: "言", 150: "谷",
    151: "豆", 152: "豕", 153: "豸", 154: "貝", 155: "赤", 156: "走",
    157: "足", 158: "身", 159: "車", 160: "辛", 161: "辰", 162: "辵",
    163: "邑", 164: "酉", 165: "釆", 166: "里", 167: "金", 168: "長",
    169: "門", 170: "阜", 171: "隶", 172: "隹", 173: "雨", 174: "靑",
    175: "非", 176: "面", 177: "革", 178: "韋", 179: "韭", 180: "音",
    181: "頁", 182: "風", 183: "飛", 184: "食", 185: "首", 186: "香",
    187: "馬", 188: "骨", 189: "高", 190: "髟", 191: "鬥", 192: "鬯",
    193: "鬲", 194: "鬼", 195: "魚", 196: "鳥", 197: "鹵", 198: "鹿",
    199: "麥", 200: "麻", 201: "黃", 202: "黍", 203: "黑", 204: "黹",
    205: "黽", 206: "鼎", 207: "鼓", 208: "鼠", 209: "鼻", 210: "齊",
    211: "齒", 212: "龍", 213: "龜", 214: "龠",
}

HSU_MAP: Dict[str, str] = {
    'ㄅ': 'b', 'ㄆ': 'p', 'ㄇ': 'm', 'ㄈ': 'f',
    'ㄉ': 'd', 'ㄊ': 't', 'ㄋ': 'n', 'ㄌ': 'l',
    'ㄍ': 'g', 'ㄎ': 'k', 'ㄏ': 'h',
    'ㄐ': 'j', 'ㄑ': 'v', 'ㄒ': 'c',
    'ㄓ': 'j', 'ㄔ': 'v', 'ㄕ': 'c', 'ㄖ': 'r',
    'ㄗ': 'z', 'ㄘ': 'a', 'ㄙ': 's',
    'ㄧ': 'e', 'ㄨ': 'x', 'ㄩ': 'u',
    'ㄚ': 'y', 'ㄛ': 'h', 'ㄜ': 'g', 'ㄝ': 'e',
    'ㄞ': 'i', 'ㄟ': 'a', 'ㄠ': 'w', 'ㄡ': 'o',
    'ㄢ': 'm', 'ㄣ': 'n', 'ㄤ': 'k', 'ㄥ': 'l', 'ㄦ': 'l',
}

ROMAN_INIT: Dict[str, str] = {
    'ㄅ': 'b', 'ㄆ': 'p', 'ㄇ': 'm', 'ㄈ': 'f',
    'ㄉ': 'd', 'ㄊ': 't', 'ㄋ': 'n', 'ㄌ': 'l',
    'ㄍ': 'g', 'ㄎ': 'k', 'ㄏ': 'h',
    'ㄐ': 'j', 'ㄑ': 'q', 'ㄒ': 'x',
    'ㄓ': 'z', 'ㄔ': 'c', 'ㄕ': 's', 'ㄖ': 'r',
    'ㄗ': 'z', 'ㄘ': 'c', 'ㄙ': 's',
}

ROMAN_FINAL: Dict[str, str] = {
    'ㄚ': 'a', 'ㄛ': 'o', 'ㄜ': 'e', 'ㄝ': 'e',
    'ㄞ': 'i', 'ㄟ': 'e', 'ㄠ': 'o', 'ㄡ': 'o',
    'ㄢ': 'n', 'ㄣ': 'n', 'ㄤ': 'n', 'ㄥ': 'n', 'ㄦ': 'r',
    'ㄧ': 'i', 'ㄨ': 'u', 'ㄩ': 'v',
}

HERITAGE_RAD_ALIAS: Dict[str, List[str]] = {
    "金": ["jn", "km"],
    "心": ["sn", "xn", "cn", "sm"],
    "木": ["mu", "mx", "mk"],
    "水": ["se", "su", "ca"],
    "火": ["ho", "hu"],
    "土": ["tu", "tx"],
    "口": ["ko"],
    "手": ["so", "co"],
    "日": ["ri", "rr"],
    "月": ["ye", "yu", "ue"],
    "人": ["rn"],
    "言": ["yn", "in", "em"],
    "女": ["nu", "nv"],
    "竹": ["zu", "ju", "jx", "tk"],
    "糸": ["mi", "si", "me"],
    "艸": ["co", "ca", "aw"],
    "辵": ["zo"],
    "肉": ["ro"],
    "貝": ["be", "ba"],
    "車": ["ce", "vg"],
    "馬": ["ma", "my"],
    "魚": ["yu", "uu"],
    "鳥": ["no", "nw"],
    "龍": ["ln", "ll"],
    "音": ["in", "en"],
    "食": ["si", "ss"],
    "冂": ["mi", "jn", "jl", "mo", "m1", "mm", "nn"],
    "冖": ["mo", "mu", "m1", "mm", "nn"],
    "匚": ["fn", "fo", "f1", "ff", "cc", "xi", "fk", "ce"],
    "匸": ["fo", "f1", "ff", "cc", "xi", "ce"],
    "幺": ["yo", "ao", "ya", "a1", "aa", "me", "ma", "ew", "ww"],
    "么": ["yo", "ao", "me", "ma", "a1", "aa", "ya", "ew", "ww", "mg"],
    "勹": ["bo", "b1", "bb", "nn"],
    "𠫓": ["te", "t1", "tt", "tu"],
    "丂": ["ke", "ko", "k1", "kk"],
    "厂": ["he", "hn", "h1", "hh", "cn", "ff"],
    "丩": ["ji", "j1", "jj"],
    "卩": ["zi", "z1", "zz", "je", "pp"],
    "厶": ["si", "s1", "ss"],
    "凵": ["yu", "v1", "vv", "uu", "kn"],
    "乚": ["en", "n1", "nn", "yi", "ll", "zz"],
    "尢": ["an", "wn", "yo", "a1", "aa"],
    "儿": ["er", "rn", "r1", "rr"],
    "重": ["zn", "jn", "cn", "zo", "jo", "co", "jl", "vl"],
    "長": ["cn", "zn", "jn", "vk", "jk"],
    "行": ["xn", "sn", "hn", "cl", "hl"],
    "樂": ["le", "ye", "lg"],
    "乾": ["gn", "qn", "cn", "gm", "vm"],
    "便": ["bn", "pn", "bm", "pm"],
    "乘": ["cn", "sn", "vl", "cl"],
    "曾": ["cn", "zn", "al", "zl"],
    "朝": ["co", "zo", "ca", "za", "vw", "jw"],
    "括": ["ko", "ga", "kh", "gy"],
    "賜": ["ci", "cc", "si", "ss"],
    "攴": ["pu", "wn", "px", "xn"],
}

VISUAL_PICTOGRAPH_RADICALS: Dict[str, str] = {
    "乂": "x", "爻": "x", "凵": "u", "匚": "c", "匸": "c",
    "冂": "n", "冖": "m", "亠": "t", "十": "t", "工": "i",
    "丨": "i", "口": "o", "囗": "o", "山": "w", "艸": "w",
    "人": "a", "入": "a", "八": "v", "丷": "v", "厂": "f",
    "广": "f", "尸": "p", "戶": "p", "匕": "l", "儿": "l",
    "乙": "z", "弓": "s",
}

COLLOQUIAL_RAD_BPM: Dict[str, str] = {
    "艸": "ㄘㄠˇ", "辵": "ㄗㄡˇ", "阜": "ㄈㄨˋ", "邑": "ㄧˋ",
    "攴": "ㄆㄨ", "宀": "ㄇㄧㄢˊ", "彳": "ㄔˋ", "广": "ㄍㄨㄤˇ",
    "疒": "ㄅㄧㄥˋ", "虍": "ㄏㄨˇ", "髟": "ㄅㄧㄠ", "糸": "ㄙ",
    "虫": "ㄔㄨㄥˊ", "肉": "ㄖㄡˋ", "行": "ㄒㄧㄥˊ", "長": "ㄔㄤˊ",
    "幺": "ㄧㄠ", "么": "ㄧㄠ", "无": "ㄨˊ", "匚": "ㄈㄤ", "冂": "ㄐㄩㄥ",
    "子": "ㄗˇ", "重": "ㄓㄨㄥˋ", "便": "ㄅㄧㄢˋ", "乘": "ㄔㄥˊ", "曾": "ㄘㄥˊ",
    "和": "ㄏㄜˊ", "怎": "ㄗㄣˇ", "蘋": "ㄆㄧㄥˊ", "怔": "ㄓㄥ", "括": "ㄎㄨㄛˋ",
    "賜": "ㄘˋ", "镸": "ㄔㄤˊ", "勹": "ㄅㄛ", "𠂊": "ㄅㄛ", "𠫓": "ㄊㄜ",
    "丂": "ㄎㄜ", "厂": "ㄏㄜ", "丩": "ㄐㄧ", "卩": "ㄗ", "㔾": "ㄗ",
    "厶": "ㄙ", "凵": "ㄩ", "乚": "ㄣ", "𠃌": "ㄣ", "尢": "ㄤ", "儿": "ㄦ",
}

# 指標性視覺合體／特殊結構字拆解（含活招牌「龕 = 合+龍」、「鋪 = 金+甫」、「肴 = 乂+有」、「區 = 匚+品 / 匚+口×3」）
CURATED_SPECIAL_CHARS: Dict[str, List[Tuple[str, str, int]]] = {
    # 格式: ch -> [(code, breakdown_desc, bonus_weight), ...]
    "龍": [
        ("lnln", "獨體疊音:龍(ln)+龍(ln)", 2680),
        ("liln", "最簡部件:立(li)+龍(ln)", 2660),
        ("yeln", "最簡部件:月(ye)+龍(ln)", 2640),
        ("lxl2", "許氏友善:ㄌ(l)ㄨ(x)ㄥ(l)+2聲", 2620),
        ("lolo", "古音相容:龍(lo)+龍(lo)", 2450),
    ],
    "鋪": [
        ("jenfx", "許氏全拼:金(jen)+甫(fx)", 2680),
        ("jenfx1", "許氏全拼:金(jen)+甫(fx)+1聲", 2680),
        ("jenfx4", "許氏全拼:金(jen)+甫(fx)+4聲", 2680),
        ("jenfxj", "許氏全拼:金(jen)+甫(fx)+4聲j", 2680),
        ("jinfu", "拼音全拼:金(jin)+甫(fu)", 2660),
        ("jinfu1", "拼音全拼:金(jin)+甫(fu)+1聲", 2660),
        ("jinfu4", "拼音全拼:金(jin)+甫(fu)+4聲", 2660),
        ("kmfu", "金(km)+甫(fu)", 2600),
        ("jnfu", "金(jn)+甫(fu)", 2500),
        ("jnfx", "金(jn)+甫(fx)", 2500),
        ("kmpu", "金(km)+鋪(pu)", 2400),
        ("ahem", "視覺拆字:釒(ae)+甫(em)", 2450),
    ],
    "龕": [
        ("hglxl", "許氏全拼:合(hg)+龍(lxl)", 2700),
        ("hglxl1", "許氏全拼:合(hg)+龍(lxl)+1聲", 2700),
        ("helong", "拼音全拼:合(he)+龍(long)", 2690),
        ("helong1", "拼音全拼:合(he)+龍(long)+1聲", 2690),
        ("holn", "最簡部件:合(ho)+龍(ln)", 2680),
        ("hgln", "許氏友善:合(hg)+龍(ln)", 2660),
        ("helo", "合(he)+龍(lo)", 2550),
        ("holg", "合(ho)+龍(lg)", 2540),
        ("hgll", "許氏雙拼:合(hg)+龍(ll)", 2530),
        ("aolo", "合(ao)+龍(lo)", 2500),
        ("aeod", "視覺拆字:合(aeo)+龍(d)", 2480),
    ],
    "肴": [
        ("xeo", "乂(x)+有(eo)+␣", 2600),
        ("xeo2", "乂(x)+有(eo)+二聲2", 2600),
        ("xeod", "乂(x)+有(eo)+二聲d", 2600),
        ("xeo3", "乂(x)+有(eo)+三聲3", 2600),
        ("xeof", "乂(x)+有(eo)+三聲f", 2600),
        ("xyo", "乂(x)+有(yo)+␣", 2550),
        ("xyo2", "乂(x)+有(yo)+二聲2", 2550),
        ("xyo3", "乂(x)+有(yo)+三聲3", 2550),
    ],
    "凶": [
        ("xu", "乂(x)+凵(u)+␣", 2600),
        ("xu1", "乂(x)+凵(u)+一聲1", 2600),
        ("ux", "凵(u)+乂(x)+␣", 2550),
        ("ux1", "凵(u)+乂(x)+一聲1", 2550),
    ],
    "區": [
        ("cpn", "匚(c)+品(pn)+␣", 2600),
        ("cpn3", "匚(c)+品(pn)+三聲3", 2600),
        ("cpnf", "匚(c)+品(pn)+三聲f", 2600),
        ("cpn1", "匚(c)+品(pn)+一聲1", 2550),
        ("cko3", "匚(c)+口(ko)×3", 2690),
        ("cqu", "匚(c)+區(qu)+␣", 2500),
        ("cooo", "視覺拆字:匚(c)+口×3(ooo)", 2620),
    ],
    "品": [
        ("ko3", "口(ko)×3+␣", 2600),
        ("ko33", "口(ko)×3+三聲3", 2600),
        ("ko3f", "口(ko)×3+三聲f", 2600),
        ("kokoko", "許氏全拼:口(ko)×3", 2660),
        ("koukoukou", "拼音全拼:口(kou)×3", 2640),
        ("ooo", "視覺拆字:口(o)×3+␣", 2650),
        ("ooo3", "視覺拆字:口(o)×3+3聲", 2650),
        ("ooof", "視覺拆字:口(o)×3+3聲f", 2650),
    ],
    "森": [
        ("mu3", "木(mu)×3+␣", 2600),
        ("mu31", "木(mu)×3+一聲1", 2600),
        ("mx3", "木(mx)×3+␣", 2550),
        ("mx31", "木(mx)×3+一聲1", 2550),
        ("mxmxmx", "許氏全拼:木(mx)×3", 2650),
        ("mumumu", "拼音全拼:木(mu)×3", 2640),
        ("ttt", "視覺拆字:木(t)×3+␣", 2620),
    ],
    "林": [
        ("mu2", "木(mu)×2+␣", 2600),
        ("mu22", "木(mu)×2+二聲2", 2600),
        ("mu2d", "木(mu)×2+二聲d", 2600),
        ("mx2", "木(mx)×2+␣", 2550),
        ("mx22", "木(mx)×2+二聲2", 2550),
        ("mxmx", "許氏全拼:木(mx)×2", 2640),
        ("mumu", "拼音全拼:木(mu)×2", 2630),
        ("txtx", "視覺拆字:木(tx)+木(tx)", 2620),
        ("tt", "視覺簡碼:木(t)+木(t)", 2580),
    ],
    "晶": [
        ("ri3", "日(ri)×3+␣", 2600),
        ("ri31", "日(ri)×3+一聲1", 2600),
        ("rr3", "日(rr)×3+␣", 2550),
        ("rrr", "許氏全拼:日(r)×3", 2660),
        ("ririri", "拼音全拼:日(ri)×3", 2640),
    ],
    "淼": [
        ("su3", "水(su)×3+␣", 2600),
        ("su33", "水(su)×3+三聲3", 2600),
        ("ca3", "水(ca)×3+␣", 2550),
        ("cacaca", "許氏全拼:水(ca)×3", 2660),
        ("shuishuishui", "拼音全拼:水(shui)×3", 2640),
    ],
    "焱": [
        ("ho3", "火(ho)×3+␣", 2600),
        ("ho34", "火(ho)×3+四聲4", 2600),
        ("hohoho", "許氏全拼:火(ho)×3", 2660),
        ("huohuohuo", "拼音全拼:火(huo)×3", 2640),
    ],
    "炎": [
        ("ho2", "火(ho)×2+␣", 2600),
        ("ho22", "火(ho)×2+二聲2", 2600),
        ("hoho", "許氏全拼:火(ho)×2", 2640),
        ("huohuo", "拼音全拼:火(huo)×2", 2630),
    ],
    "鑫": [
        ("km3", "金(km)×3+␣", 2600),
        ("km31", "金(km)×3+一聲1", 2600),
        ("jn3", "金(jn)×3+␣", 2550),
        ("jenjenjen", "許氏全拼:金(jen)×3", 2660),
        ("jnjnjn", "許氏雙拼:金(jn)×3", 2650),
        ("jinjinjin", "拼音全拼:金(jin)×3", 2640),
    ],
    "磊": [
        ("si3", "石(si)×3+␣", 2600),
        ("si33", "石(si)×3+三聲3", 2600),
        ("cc3", "石(cc)×3+␣", 2550),
        ("ccc", "許氏全拼:石(c)×3", 2660),
        ("shishishi", "拼音全拼:石(shi)×3", 2640),
    ],
    "眾": [
        ("rn3", "人(rn)×3+␣", 2600),
        ("rn34", "人(rn)×3+四聲4", 2600),
        ("rnrnrn", "許氏全拼:人(rn)×3", 2660),
        ("renrenren", "拼音全拼:人(ren)×3", 2640),
    ],
    "从": [
        ("rn2", "人(rn)×2+␣", 2500),
        ("rnrn", "許氏全拼:人(rn)×2", 2560),
        ("renren", "拼音全拼:人(ren)×2", 2540),
    ],
    "圭": [
        ("tu2", "土(tu)×2+␣", 2600),
        ("tu21", "土(tu)×2+一聲1", 2600),
        ("txtx", "許氏全拼:土(tx)×2", 2580),
        ("tutu", "拼音全拼:土(tu)×2", 2620),
    ],
    "垚": [
        ("tu3", "土(tu)×3+␣", 2600),
        ("txtxtx", "許氏全拼:土(tx)×3", 2650),
        ("tututu", "拼音全拼:土(tu)×3", 2640),
    ],
    "朋": [
        ("yu2", "月(yu)×2+␣", 2600),
        ("yu22", "月(yu)×2+二聲2", 2600),
        ("ueue", "許氏全拼:月(ue)×2", 2640),
        ("yueyue", "拼音全拼:月(yue)×2", 2630),
    ],
    "呂": [
        ("ko2", "口(ko)×2+␣", 2550),
        ("ko23", "口(ko)×2+三聲3", 2600),
        ("koko", "許氏全拼:口(ko)×2", 2580),
        ("oo", "口(o)×2+␣", 2500),
    ],
    "回": [
        ("ko2", "口(ko)×2+␣", 2620),
        ("ko22", "口(ko)×2+二聲2", 2620),
        ("koko", "許氏全拼:口(ko)×2", 2620),
        ("oo", "口(o)×2+␣", 2600),
    ],
    "器": [
        ("ko4", "口(ko)×4+␣", 2600),
        ("ko44", "口(ko)×4+四聲4", 2600),
        ("oko4", "口(o)+口(ko)×4", 2600),
        ("kokokoko", "許氏全拼:口(ko)×4", 2650),
    ],
    "協": [
        ("tli3", "十(t)+力(li)×3", 2600),
        ("tle3", "十(t)+力(le)×3", 2600),
        ("sili", "十(si)+力(li)", 2500),
    ],
    "脅": [
        ("oli3", "月(o)+力(li)×3", 2600),
        ("ole3", "月(o)+力(le)×3", 2600),
    ],
    "蕊": [
        ("wsm3", "艹(w)+心(sm)×3", 2600),
        ("wxn3", "艹(w)+心(xn)×3", 2600),
        ("wcn3", "艹(w)+心(cn)×3", 2600),
    ],
    "爽": [
        ("kx4", "大(k)+乂(x)×4+␣", 2600),
        ("da4", "大(da)+乂×4+␣", 2550),
    ],
    "轟": [
        ("ce3", "車(ce)×3+␣", 2600),
        ("ce31", "車(ce)×3+一聲1", 2600),
        ("vgvgvg", "許氏全拼:車(vg)×3", 2660),
        ("checheche", "拼音全拼:車(che)×3", 2640),
    ],
    "雙": [
        ("zv2", "隹(zv)×2+␣", 2600),
        ("jaja", "許氏全拼:隹(ja)×2", 2640),
    ],
    "矗": [
        ("zi3", "直(zi)×3+␣", 2600),
        ("jjj", "許氏全拼:直(j)×3", 2660),
        ("zhizhizhi", "拼音全拼:直(zhi)×3", 2640),
    ],
    "希": [
        ("xbu", "乂(x)+布(bu)+␣", 2600),
        ("xbu1", "乂(x)+布(bu)+一聲1", 2600),
        ("xbx", "乂(x)+布(bx)+␣", 2550),
    ],
    "同": [
        ("nto", "冂(n)+同(to)+␣", 2600),
        ("nto2", "冂(n)+同(to)+二聲2", 2600),
        ("ntl", "冂(n)+同(tl)+␣", 2550),
    ],
    "中": [
        ("oi", "口(o)+丨(i)+␣", 2600),
        ("oi1", "口(o)+丨(i)+一聲1", 2600),
    ],
    "古": [
        ("cko", "許氏全拼:十(c)+口(ko)", 2650),
        ("cko3", "許氏全拼:十(c)+口(ko)+3聲", 2660),
        ("ckof", "許氏全拼:十(c)+口(ko)+3聲f", 2660),
        ("shikou", "拼音全拼:十(shi)+口(kou)", 2650),
        ("tio", "視覺拆字:十(ti)+口(o)", 2640),
        ("to", "十(t)+口(o)+␣", 2600),
        ("to3", "十(t)+口(o)+三聲3", 2600),
        ("tof", "十(t)+口(o)+三聲f", 2600),
        ("siko", "十(si)+口(ko)", 2550),
    ],
    "叶": [
        ("koc", "許氏全拼:口(ko)+十(c)", 2650),
        ("koushi", "拼音全拼:口(kou)+十(shi)", 2640),
        ("oti", "視覺拆字:口(o)+十(ti)", 2650),
        ("ot", "視覺簡碼:口(o)+十(t)", 2600),
    ],
    "杏": [
        ("mxko", "許氏全拼:木(mx)+口(ko)", 2650),
        ("mukou", "拼音全拼:木(mu)+口(kou)", 2640),
        ("txo", "視覺拆字:木(tx)+口(o)", 2650),
    ],
    "呆": [
        ("komx", "許氏全拼:口(ko)+木(mx)", 2650),
        ("koumu", "拼音全拼:口(kou)+木(mu)", 2640),
        ("otx", "視覺拆字:口(o)+木(tx)", 2650),
        ("oe", "視覺簡碼:口(o)+木(e)", 2580),
    ],
    "好": [
        ("nuze", "許氏全拼:女(nu)+子(ze)", 2650),
        ("nuz", "許氏全拼:女(nu)+子(z)", 2650),
        ("nvzi", "拼音全拼:女(nv)+子(zi)", 2640),
        ("xoje", "視覺拆字:女(xo)+子(je)", 2650),
        ("xj", "視覺簡碼:女(x)+子(j)", 2600),
    ],
    "喆": [
        ("jeje", "許氏全拼:吉(je)+吉(je)", 2680),
        ("jeje2", "許氏全拼:吉(je)+吉(je)+2聲", 2680),
        ("jejed", "許氏全拼:吉(je)+吉(je)+2聲d", 2680),
        ("jiji", "拼音全拼:吉(ji)+吉(ji)", 2680),
        ("jiji2", "拼音全拼:吉(ji)+吉(ji)+2聲", 2680),
        ("tote", "視覺拆字:士口(to)+士口(te)", 2600),
    ],
    "謝": [
        ("emcnam", "許氏全拼:言(em)+身(cn)+寸(am)", 2680),
        ("emcnam4", "許氏全拼:言(em)+身(cn)+寸(am)+4聲", 2680),
        ("emcnamj", "許氏全拼:言(em)+身(cn)+寸(am)+4聲j", 2680),
        ("emam", "許氏首尾:言(em)+寸(am)", 2660),
        ("yanshencun", "拼音全拼:言(yan)+身(shen)+寸(cun)", 2670),
        ("yancun", "拼音首尾:言(yan)+寸(cun)", 2650),
    ],
    "喜": [
        ("cdoko", "許氏全拼:士(c)+豆(do)+口(ko)", 2680),
        ("cdoko3", "許氏全拼:士(c)+豆(do)+口(ko)+3聲", 2680),
        ("cdokof", "許氏全拼:士(c)+豆(do)+口(ko)+3聲f", 2680),
        ("cko", "許氏首尾:士(c)+口(ko)", 2630),
        ("shidoukou", "拼音全拼:士(shi)+豆(dou)+口(kou)", 2670),
        ("shikou", "拼音首尾:士(shi)+口(kou)", 2620),
    ],
    "看": [
        ("comx", "許氏全拼:手(co)+目(mx)", 2680),
        ("comx4", "許氏全拼:手(co)+目(mx)+4聲", 2680),
        ("comxj", "許氏全拼:手(co)+目(mx)+4聲j", 2680),
        ("shoumu", "拼音全拼:手(shou)+目(mu)", 2670),
        ("shoumu4", "拼音全拼:手(shou)+目(mu)+4聲", 2670),
    ],
    "語": [
        ("emx", "許氏全拼:言(em)+吾(x)", 2680),
        ("emx3", "許氏全拼:言(em)+吾(x)+3聲", 2680),
        ("emxf", "許氏全拼:言(em)+吾(x)+3聲f", 2680),
        ("emxx", "許氏雙拼:言(em)+吾(xx)", 2650),
        ("yanwu", "拼音全拼:言(yan)+吾(wu)", 2670),
        ("yanwu3", "拼音全拼:言(yan)+吾(wu)+3聲", 2670),
    ],
    "合": [
        ("ao", "人(a)+口(o)+␣", 2600),
        ("ao2", "人(a)+口(o)+二聲2", 2600),
        ("aod", "人(a)+口(o)+二聲d", 2600),
        ("aeo", "視覺拆字:人(a)+一(e)+口(o)", 2640),
        ("rnko", "人(rn)+口(ko)", 2550),
    ],
    "休": [
        ("rnmu", "人(rn)+木(mu)", 2600),
        ("rnm1", "人(rn)+木(m)+1聲", 2580),
        ("rnmx", "許氏全拼:人(rn)+木(mx)", 2650),
        ("renmu", "拼音全拼:人(ren)+木(mu)", 2650),
        ("atx", "視覺拆字:亻(a)+木(tx)", 2640),
        ("at", "視覺簡碼:亻(a)+木(t)", 2580),
    ],
    "信": [
        ("rnyn", "人(rn)+言(yn)", 2600),
        ("rny4", "人(rn)+言(y)+4聲", 2580),
        ("rnem", "許氏全拼:人(rn)+言(em)", 2650),
        ("renyan", "拼音全拼:人(ren)+言(yan)", 2650),
        ("aveo", "視覺拆字:亻(a)+言(veo)", 2600),
    ],
    "明": [
        ("rue", "許氏全拼:日(r)+月(ue)", 2680),
        ("rue2", "許氏全拼:日(r)+月(ue)+2聲", 2680),
        ("rued", "許氏全拼:日(r)+月(ue)+2聲d", 2680),
        ("reue", "許氏相容:日(re)+月(ue)", 2670),
        ("reue2", "許氏相容:日(re)+月(ue)+2聲", 2670),
        ("riyue", "拼音全拼:日(ri)+月(yue)", 2680),
        ("riyue2", "拼音全拼:日(ri)+月(yue)+2聲", 2680),
        ("bode", "視覺拆字:日(bo)+月(de)", 2660),
        ("bd", "視覺簡碼:日(b)+月(d)", 2620),
        ("riye", "日(ri)+月(ye)", 2620),
        ("riy4", "日(ri)+月(y)+4", 2610),
        ("riyu", "日(ri)+月(yu)", 2600),
        ("rruu", "日(rr)+月(uu)", 2550),
    ],
    "章": [
        ("liza", "最簡部件:立(li)+早(za)", 2640),
        ("liz3", "最簡部件:立(li)+早(z)+3聲", 2620),
        ("liz1", "最簡部件:立(li)+早(z)+1聲", 2600),
    ],
    "意": [
        ("insm", "最簡部件:音(in)+心(sm)", 2640),
        ("ins4", "最簡部件:音(in)+心(s)+4聲", 2620),
        ("lis4", "最簡部件:立(li)+心(s)+4聲", 2600),
    ],
    "音": [
        ("lir4", "最簡部件:立(li)+日(r)+4", 2680),
        ("liri", "最簡部件:立(li)+日(ri)", 2670),
        ("inin", "獨體疊音:音(in)+音(in)", 2660),
        ("ynyn", "漢拼相容:音(yn)+音(yn)", 2450),
    ],
    "法": [
        ("seci", "最簡部件:水(se)+去(ci)", 2680),
        ("sefa", "最簡部件:水(se)+本音法(fa)", 2660),
        ("sufa", "水(su)+本音法(fa)", 2480),
        ("suqu", "水(su)+右旁去(qu)", 2460),
        ("cafa", "許氏水(ca)+法(fa)", 2440),
        ("cafy", "許氏水(ca)+許氏法(fy)", 2420),
    ],
    "形": [
        ("jn3", "最簡部件:井(jn)+彡(3畫)", 2680),
        ("jnsn", "最簡部件:井(jn)+彡(sn)", 2660),
        ("jnse", "許氏友善:井(jn)+形(se)", 2640),
        ("jncn", "許氏友善:井(jn)+彡/形(cn)", 2620),
        ("snjg", "彡(sn)+古聲符井(jg)", 2450),
        ("snxg", "彡(sn)+本音形(xg)", 2430),
        ("kasn", "左旁開(ka)+右部彡(sn)", 2420),
        ("cmjg", "許氏彡(cm)+井(jg)", 2410),
        ("cmjl", "許氏彡(cm)+許氏井(jl)", 2400),
    ],
}


def bpm_strip_tone(bpm: str) -> str:
    return bpm.replace('ˊ', '').replace('ˇ', '').replace('ˋ', '').replace('˙', '').strip()


def bpm_to_hsu_2key(bpm: str) -> str:
    clean = bpm_strip_tone(bpm)
    keys = [HSU_MAP[c] for c in clean if c in HSU_MAP]
    if not keys:
        return "xx"
    if len(keys) == 1:
        return keys[0] + keys[0]
    return keys[0] + keys[-1]


def bpm_to_hsu_full(bpm: str) -> str:
    clean = bpm_strip_tone(bpm)
    return "".join(HSU_MAP[c] for c in clean if c in HSU_MAP)


PINYIN_INIT_FULL: Dict[str, str] = {
    'ㄅ': 'b', 'ㄆ': 'p', 'ㄇ': 'm', 'ㄈ': 'f',
    'ㄉ': 'd', 'ㄊ': 't', 'ㄋ': 'n', 'ㄌ': 'l',
    'ㄍ': 'g', 'ㄎ': 'k', 'ㄏ': 'h',
    'ㄐ': 'j', 'ㄑ': 'q', 'ㄒ': 'x',
    'ㄓ': 'zh', 'ㄔ': 'ch', 'ㄕ': 'sh', 'ㄖ': 'r',
    'ㄗ': 'z', 'ㄘ': 'c', 'ㄙ': 's',
}

PINYIN_FINAL_SIMPLE: Dict[str, str] = {
    'ㄚ': 'a', 'ㄛ': 'o', 'ㄜ': 'e', 'ㄝ': 'e',
    'ㄞ': 'ai', 'ㄟ': 'ei', 'ㄠ': 'ao', 'ㄡ': 'ou',
    'ㄢ': 'an', 'ㄣ': 'en', 'ㄤ': 'ang', 'ㄥ': 'eng', 'ㄦ': 'er',
    'ㄧ': 'i', 'ㄨ': 'u', 'ㄩ': 'u',
}


def bpm_to_pinyin_full(bpm: str) -> str:
    """將注音轉換為標準漢語拼音全拼（無調號），供龍龕 2.0 部件全拼免縮碼軌使用"""
    clean = bpm_strip_tone(bpm)
    if not clean:
        return ""
    init = ""
    rest = clean
    if clean[0] in PINYIN_INIT_FULL:
        init = PINYIN_INIT_FULL[clean[0]]
        rest = clean[1:]
    if not rest:
        return init + "i"
    if len(rest) == 1:
        r0 = rest[0]
        if not init:
            if r0 == 'ㄧ':
                return "yi"
            if r0 == 'ㄨ':
                return "wu"
            if r0 == 'ㄩ':
                return "yu"
        if r0 == 'ㄩ' and init in ('n', 'l'):
            return init + "v"
        return init + PINYIN_FINAL_SIMPLE.get(r0, 'a')
    med, fin = rest[0], rest[-1]
    if med == 'ㄧ':
        map_i = {
            'ㄚ': 'ia', 'ㄛ': 'io', 'ㄝ': 'ie', 'ㄞ': 'iai', 'ㄠ': 'iao',
            'ㄡ': 'iu', 'ㄢ': 'ian', 'ㄣ': 'in', 'ㄤ': 'iang', 'ㄥ': 'ing',
        }
        map_yi = {
            'ㄚ': 'ya', 'ㄛ': 'yo', 'ㄝ': 'ye', 'ㄞ': 'yai', 'ㄠ': 'yao',
            'ㄡ': 'you', 'ㄢ': 'yan', 'ㄣ': 'yin', 'ㄤ': 'yang', 'ㄥ': 'ying',
        }
        return (init + map_i.get(fin, 'i')) if init else map_yi.get(fin, 'yi')
    if med == 'ㄨ':
        if fin == 'ㄥ':
            return (init + "ong") if init else "weng"
        map_u = {
            'ㄚ': 'ua', 'ㄛ': 'uo', 'ㄞ': 'uai', 'ㄟ': 'ui',
            'ㄢ': 'uan', 'ㄣ': 'un', 'ㄤ': 'uang',
        }
        map_wu = {
            'ㄚ': 'wa', 'ㄛ': 'wo', 'ㄞ': 'wai', 'ㄟ': 'wei',
            'ㄢ': 'wan', 'ㄣ': 'wen', 'ㄤ': 'wang',
        }
        return (init + map_u.get(fin, 'u')) if init else map_wu.get(fin, 'wu')
    if med == 'ㄩ':
        if fin == 'ㄥ':
            return (init + "iong") if init else "yong"
        map_v = {'ㄝ': 'ue', 'ㄢ': 'uan', 'ㄣ': 'un'}
        map_yu = {'ㄝ': 'yue', 'ㄢ': 'yuan', 'ㄣ': 'yun'}
        return (init + map_v.get(fin, 'u')) if init else map_yu.get(fin, 'yu')
    return init + PINYIN_FINAL_SIMPLE.get(fin, 'a')


# 徐冰《英文方塊字》+ 幾何直覺之「漢字基本部件/筆畫 -> 英文字母」視覺映射表 (龍龕 2.0 軌道 V：外國人友善零讀音軌)
VISUAL_SHAPE_MAP: Dict[str, str] = {
    "口": "o", "囗": "o", "〇": "o",
    "日": "b", "曰": "b", "目": "b", "四": "b", "罒": "b", "田": "b", "由": "b", "甲": "b", "申": "b", "貝": "b", "見": "b",
    "月": "d", "⺼": "d", "肉": "d", "舟": "d", "骨": "d", "角": "d", "夕": "d", "歹": "d",
    "十": "t", "廾": "t", "艹": "t", "艸": "t", "䒑": "t", "土": "t", "士": "t", "干": "t", "寸": "t", "木": "t", "禾": "t", "米": "t",
    "亠": "t", "立": "t", "辛": "t", "言": "t", "訁": "t", "讠": "t", "文": "t", "方": "t",
    "厂": "f", "广": "f", "疒": "f", "戶": "f", "户": "f", "尸": "f", "尺": "f", "毛": "f", "气": "f", "斤": "f", "爪": "f", "爫": "f", "⺤": "f",
    "一": "e", "二": "e", "三": "e", "ヨ": "e", "彐": "e", "⺕": "e", "聿": "e", "⺻": "e", "艮": "e", "臣": "e", "耳": "e", "車": "e", "馬": "e",
    "工": "h", "卄": "h", "王": "h", "玉": "h", "五": "h", "井": "h", "亞": "h", "門": "h", "鬥": "h", "冊": "h", "甘": "h",
    "丨": "i", "刂": "i", "川": "i", "巛": "i", "彳": "i", "巾": "i", "中": "i", "卜": "i", "上": "i", "下": "i", "止": "i",
    "亅": "j", "丁": "j", "于": "j", "子": "j", "孑": "j", "了": "j", "矛": "j", "丩": "j",
    "人": "a", "亻": "a", "入": "a", "亼": "a", "亽": "a", "大": "a", "太": "a", "犬": "a", "犭": "a", "矢": "a", "夫": "a", "天": "a", "父": "a", "金": "a", "釒": "a", "钅": "a", "食": "a", "飠": "a",
    "八": "v", "丷": "v", "ソ": "v", "冫": "v", "冰": "v", "小": "v", "⺌": "v", "忄": "v", "⺗": "v", "心": "v",
    "灬": "w", "火": "w", "山": "w", "屮": "w",
    "水": "s", "氵": "s", "弓": "s", "虫": "s",
    "丫": "y", "羊": "y", "⺶": "y", "⺷": "y", "𦍌": "y", "竹": "y", "⺮": "y", "隹": "y", "鳥": "y",
    "乂": "x", "㐅": "x", "爻": "x", "又": "x", "夂": "x", "夊": "x", "攵": "x", "攴": "x", "支": "x", "殳": "x", "戈": "x", "弋": "x", "女": "x", "母": "x",
    "手": "k", "扌": "k", "龵": "k", "牛": "k", "牜": "k", "生": "k", "片": "k", "爿": "k", "衣": "k", "衤": "k", "礻": "k", "示": "k",
    "冂": "m", "冖": "m", "宀": "m", "穴": "m", "雨": "m", "⻗": "m", "皿": "m", "网": "m",
    "勹": "n", "𠂊": "n", "⺈": "n", "几": "n", "風": "n", "乃": "n", "力": "n", "刀": "n", "刃": "n",
    "凵": "u", "匚": "c", "匸": "c",
    "乚": "l", "𠃌": "l", "儿": "l", "匕": "l",
    "乙": "z", "厶": "z", "幺": "z", "乡": "z", "糸": "z", "糹": "z", "纟": "z", "辶": "z", "⻌": "z", "辵": "z", "廴": "z",
    "巳": "p", "己": "p", "卩": "p", "⺋": "p", "㔾": "p", "阝": "p", "阜": "p", "邑": "p",
    "丿": "a", "𠂉": "a", "丶": "v", "乀": "v", "乁": "l", "乛": "z", "𠃍": "z", "𡿨": "z", "彡": "e",
}

ATOM_VISUAL_SEQ: Dict[str, str] = {
    "一": "e", "二": "ee", "三": "eee", "十": "ti", "土": "te", "士": "te", "王": "he", "玉": "hev",
    "口": "o", "囗": "o", "日": "bo", "曰": "bo", "目": "be", "田": "ot", "由": "ot", "甲": "ot", "申": "oi",
    "中": "oi", "人": "a", "入": "a", "八": "v", "大": "at", "太": "atv", "天": "ea", "夫": "eat",
    "木": "tx", "本": "txe", "末": "etx", "未": "etx", "禾": "atx", "米": "vtx", "來": "txaa",
    "水": "six", "氵": "s", "火": "va", "灬": "w", "山": "w", "出": "wu", "女": "xo", "子": "je", "了": "j",
    "手": "eki", "扌": "k", "心": "uv", "忄": "v", "月": "de", "⺼": "d", "斤": "ft", "爪": "f", "爫": "f",
    "厂": "f", "广": "vf", "疒": "vf", "尸": "fo", "戶": "vfo", "户": "vfo", "門": "hh",
    "言": "veo", "訁": "vo", "金": "ahe", "釒": "ae", "食": "aez", "飠": "az",
    "糸": "zzv", "糹": "zv", "幺": "zz", "艹": "tt", "艸": "tt", "辶": "vz", "⻌": "vz", "辵": "vz",
    "阝": "p", "卩": "p", "刀": "na", "刂": "i", "力": "na", "又": "x", "乂": "x", "寸": "tj",
    "工": "h", "干": "te", "巾": "mi", "冖": "m", "宀": "vm", "穴": "vmv", "冂": "m", "凵": "u", "匚": "c",
    "勹": "n", "几": "n", "儿": "l", "乚": "l", "乙": "z", "弓": "s", "厶": "z", "匕": "l",
    "卜": "iv", "上": "ie", "下": "ei", "止": "iie", "正": "eiie", "步": "iiea",
    "小": "jv", "⺌": "v", "少": "jva", "尖": "vja", "白": "abo", "百": "eabo", "自": "abe",
    "貝": "bev", "見": "bel", "頁": "eabe", "耳": "eh", "臣": "ce", "車": "eoe", "馬": "ehw",
    "牛": "ate", "牜": "k", "生": "ate", "羊": "vhe", "⺶": "yh", "犬": "atv", "犭": "a", "豕": "eax",
    "鳥": "abw", "隹": "aie", "魚": "nbw", "虫": "ois", "龍": "tdez", "龜": "nbz",
    "立": "tve", "音": "tveb", "章": "tveb", "皿": "mo", "四": "ob", "罒": "o", "网": "mx",
    "西": "eo", "覀": "eo", "雨": "emv", "⻗": "mv", "雲": "emvz", "石": "efo", "示": "eev", "礻": "kv",
    "衣": "vea", "衤": "ka", "市": "vmi", "布": "etmi", "希": "xetm",
    "戈": "txv", "弋": "tv", "戊": "ftx", "戌": "ftx", "成": "fna", "我": "ktx",
    "方": "tvn", "文": "vx", "父": "vx", "交": "vxa", "尺": "foa", "尼": "fol", "尾": "fof",
    "巴": "ol", "色": "nol", "邑": "ool", "艮": "ezx", "良": "vezx", "羽": "nn", "習": "nnbo",
    "竹": "aa", "⺮": "yy", "豆": "eov", "瓜": "fzv", "果": "btx",
    "里": "ote", "重": "aote", "黑": "otw", "用": "mte", "甫": "emtv", "角": "nmte",
    "皮": "fxi", "毛": "afe", "氏": "afe", "气": "aee", "片": "ike", "爿": "ike", "牙": "eja", "齒": "iieo",
    "足": "oia", "⻊": "oi", "身": "abea", "首": "vabe", "面": "eob", "骨": "mmd", "肉": "maa", "血": "amo",
    "行": "iiej", "走": "tiia", "赤": "tjav", "谷": "vvao", "辰": "feea", "酉": "eob",
    "采": "ftx", "長": "eeza", "非": "ieee", "革": "toet", "韋": "eoe", "韭": "ieee", "風": "nva", "飛": "zva",
    "香": "atxb", "高": "vomo", "髟": "eezv", "鬯": "uxv", "鬲": "eomn", "鬼": "abol", "麥": "txax",
    "麻": "vftx", "黃": "ttob", "黍": "atxs", "鼎": "beih", "鼓": "teox", "鼠": "nbzz", "鼻": "abto", "齊": "vxii",
}


TONGYONG_INIT: Dict[str, str] = {
    'ㄅ': 'b', 'ㄆ': 'p', 'ㄇ': 'm', 'ㄈ': 'f',
    'ㄉ': 'd', 'ㄊ': 't', 'ㄋ': 'n', 'ㄌ': 'l',
    'ㄍ': 'g', 'ㄎ': 'k', 'ㄏ': 'h',
    'ㄐ': 'j', 'ㄑ': 'c', 'ㄒ': 's',
    'ㄓ': 'j', 'ㄔ': 'c', 'ㄕ': 's', 'ㄖ': 'r',
    'ㄗ': 'z', 'ㄘ': 'c', 'ㄙ': 's',
}


def bpm_to_tongyong_2key(bpm: str) -> str:
    """通用拼音（Tongyong Pinyin）與歐美自然拼讀直覺雙碼：ㄒ->s、ㄑ->c、ㄓ->j(jh)、ㄩ->u(yu)、空韻->-i"""
    clean = bpm_strip_tone(bpm)
    if not clean:
        return "xx"
    if clean in ("ㄧㄣ", "ㄧㄥ"):
        return "in"
    if clean == "ㄩㄝ":
        return "ye"
    init = ""
    rest = clean
    if clean[0] in TONGYONG_INIT:
        init = TONGYONG_INIT[clean[0]]
        rest = clean[1:]
    if not rest:
        return init + "i"
    if rest[-1] in ('ㄢ', 'ㄣ', 'ㄤ', 'ㄥ'):
        if init:
            return init + "n"
        if clean.startswith('ㄨ'):
            return "wn"
        if clean.startswith(('ㄧ', 'ㄩ')):
            return "yn"
        return ('a' if clean[0] in ('ㄚ', 'ㄞ', 'ㄠ', 'ㄢ', 'ㄤ') else 'e') + "n"
    if not init:
        if clean.startswith('ㄧ'):
            return "yi" if len(clean) == 1 else ("y" + ROMAN_FINAL.get(clean[-1], 'i'))
        if clean.startswith('ㄨ'):
            return "wu" if len(clean) == 1 else ("w" + ROMAN_FINAL.get(clean[-1], 'u'))
        if clean.startswith('ㄩ'):
            return "yu" if len(clean) == 1 else ("y" + ROMAN_FINAL.get(clean[-1], 'u'))
        f = ROMAN_FINAL.get(clean[-1], 'a')
        if clean[0] in ('ㄚ', 'ㄞ', 'ㄠ', 'ㄢ', 'ㄤ'):
            return 'a' + f
        if clean[0] in ('ㄛ', 'ㄡ'):
            return 'o' + f
        return 'e' + f
    if rest == "ㄩ":
        return init + "u"
    if rest == "ㄨㄥ":
        return init + "o"
    return init + ROMAN_FINAL.get(rest[-1], 'a')


def bpm_to_palatal_variants_2key(bpm: str) -> List[str]:
    """舌面音 ㄐ/ㄑ/ㄒ、捲舌音 ㄓ 及撮口音 ㄩ 雙軌相容（同時支援漢語拼音與通用拼音）：
    - ㄑ -> c (通拼 ci-/cyu-) / q (漢拼 qi-/qu-)
    - ㄒ -> s (通拼 si-/syu-) / x (漢拼 xi-/xu-)
    - ㄓ -> z (漢拼 zh-) / j (通拼 jh-，如 至=zi/ji、重=zn/jn、竹=zu/ju)
    - ㄩ -> u (通拼 yu/cyu/syu/nyu/lyu) / i / v
    """
    clean = bpm_strip_tone(bpm)
    if not clean:
        return []
    if clean in ("ㄋㄩ", "ㄌㄩ"):
        ic = ROMAN_INIT[clean[0]]
        return [ic + "u", ic + "v"]
    if clean[0] not in ('ㄐ', 'ㄑ', 'ㄒ', 'ㄓ'):
        return []
    init_choices = {'ㄐ': ['j'], 'ㄑ': ['c', 'q'], 'ㄒ': ['s', 'x'], 'ㄓ': ['z', 'j']}[clean[0]]
    rest = clean[1:]
    final_choices: List[str] = []
    if not rest:
        final_choices = ['i', 'h']
    elif rest == 'ㄧ':
        final_choices = ['i', 'e']
    elif rest == 'ㄩ':
        final_choices = ['u', 'i', 'v']
    elif rest[-1] in ('ㄢ', 'ㄣ', 'ㄤ', 'ㄥ'):
        final_choices = ['n', 'o'] if rest == "ㄨㄥ" else ['n']
    else:
        f = ROMAN_FINAL.get(rest[-1], 'i')
        final_choices = [f, 'a'] if rest[-1] in ('ㄞ', 'ㄠ') and f != 'a' else [f]
    out: List[str] = []
    for ic in init_choices:
        for fc in final_choices:
            cand = ic + fc
            if cand not in out:
                out.append(cand)
    return out


def bpm_to_nasal_n_2key(bpm: str) -> str:
    """前後鼻音合流 (-n / -ng -> n) 且零聲母 ㄧㄣ/ㄧㄥ -> in 之直覺雙碼"""
    clean = bpm_strip_tone(bpm)
    if not clean:
        return "xx"
    if clean in ("ㄧㄣ", "ㄧㄥ"):
        return "in"
    if clean == "ㄩㄝ":
        return "ye"
    init = ""
    rest = clean
    if clean[0] in ROMAN_INIT:
        init = ROMAN_INIT[clean[0]]
        rest = clean[1:]
    if not rest:
        return init + init
    if rest[-1] in ('ㄢ', 'ㄣ', 'ㄤ', 'ㄥ'):
        if init:
            return init + "n"
        if clean.startswith('ㄨ'):
            return "wn"
        if clean.startswith(('ㄧ', 'ㄩ')):
            return "yn"
        return ('a' if clean[0] in ('ㄚ', 'ㄞ', 'ㄠ', 'ㄢ', 'ㄤ') else 'e') + "n"
    return bpm_to_roman_2key(bpm)


def bpm_to_roman_2key(bpm: str) -> str:
    clean = bpm_strip_tone(bpm)
    if not clean:
        return "xx"
    init = ""
    rest = clean
    if clean[0] in ROMAN_INIT:
        init = ROMAN_INIT[clean[0]]
        rest = clean[1:]
    if not rest:
        return init + init
    if not init:
        if clean.startswith('ㄧ'):
            if len(clean) == 1:
                return "yi"
            return "y" + ROMAN_FINAL.get(clean[-1], 'i')
        elif clean.startswith('ㄨ'):
            if len(clean) == 1:
                return "wu"
            return "w" + ROMAN_FINAL.get(clean[-1], 'u')
        elif clean.startswith('ㄩ'):
            if len(clean) == 1:
                return "yu"
            return "y" + ROMAN_FINAL.get(clean[-1], 'v')
        else:
            f = ROMAN_FINAL.get(clean[-1], 'a')
            if clean[0] in ('ㄚ', 'ㄞ', 'ㄠ', 'ㄢ', 'ㄤ'):
                return 'a' + f
            if clean[0] in ('ㄛ', 'ㄡ'):
                return 'o' + f
            return 'e' + f
    if rest == "ㄨㄥ":
        return init + "o"
    return init + ROMAN_FINAL.get(rest[-1], 'a')


def get_tone_pair(bpm: str) -> Tuple[str, str]:
    if 'ˊ' in bpm:
        return ('2', 'd')
    elif 'ˇ' in bpm:
        return ('3', 'f')
    elif 'ˋ' in bpm:
        return ('4', 'j')
    elif '˙' in bpm:
        return ('5', 's')
    return ('1', '')


def main() -> int:
    char_bpm: Dict[str, str] = {}
    char_weight: Counter = Counter()
    word_weight: Counter = Counter()

    for tsv_name in ["lexicon_mcbopomofo.tsv", "lexicon_moe.tsv"]:
        p = LEXICON_DIR / tsv_name
        if not p.exists():
            continue
        is_mcb = (tsv_name == "lexicon_mcbopomofo.tsv")
        with open(p, "r", encoding="utf-8") as f:
            for line in f:
                if line.startswith("#") or not line.strip():
                    continue
                parts = line.strip().split("\t")
                if len(parts) >= 3 and parts[2].lstrip("-").isdigit():
                    w, bpm, wt = parts[0], parts[1], int(parts[2])
                    if len(w) == 1 and '\u4e00' <= w <= '\u9fff':
                        eff_wt = wt + (500 if is_mcb else 0)
                        if eff_wt > char_weight[w]:
                            char_weight[w] = eff_wt
                            char_bpm[w] = bpm
                    elif 2 <= len(w) <= 6 and all('\u4e00' <= c <= '\u9fff' for c in w):
                        if wt > word_weight[w]:
                            word_weight[w] = wt

    rs_map: Dict[str, Tuple[int, int]] = {}
    phonetic_map: Dict[str, List[str]] = {}
    strokes_map: Dict[str, int] = {}
    has_trad_var: Set[str] = set()
    has_simp_var: Set[str] = set()

    with zipfile.ZipFile(UNIHAN_ZIP, "r") as zf:
        for line in zf.open("Unihan_IRGSources.txt"):
            s = line.decode("utf-8", "ignore").strip().split("\t")
            if len(s) >= 3 and s[0].startswith("U+"):
                ch = chr(int(s[0][2:], 16))
                if s[1] == "kRSUnicode":
                    tok = s[2].split()[0]
                    rad_str, res_str = tok.split(".")
                    rs_map[ch] = (int(rad_str.replace("'", "")), int(res_str))
                elif s[1] == "kTotalStrokes":
                    strokes_map[ch] = int(s[2].split()[0])
        for line in zf.open("Unihan_DictionaryLikeData.txt"):
            s = line.decode("utf-8", "ignore").strip().split("\t")
            if len(s) >= 3 and s[0].startswith("U+") and s[1] == "kPhonetic":
                ch = chr(int(s[0][2:], 16))
                phonetic_map[ch] = [t.replace("*", "") for t in s[2].split()]
        if "Unihan_Variants.txt" in zf.namelist():
            for line in zf.open("Unihan_Variants.txt"):
                s = line.decode("utf-8", "ignore").strip().split("\t")
                if len(s) >= 3 and s[0].startswith("U+"):
                    ch = chr(int(s[0][2:], 16))
                    other_targets = [
                        t.split("<")[0] for t in s[2].split() if t.split("<")[0] != s[0]
                    ]
                    if s[1] == "kTraditionalVariant" and other_targets:
                        has_trad_var.add(ch)
                    elif s[1] == "kSimplifiedVariant" and other_targets:
                        has_simp_var.add(ch)

    def is_pure_simplified(c: str) -> bool:
        return (c in has_trad_var) and (c not in has_simp_var)

    series_members: Dict[str, List[str]] = defaultdict(list)
    for ch, pids in phonetic_map.items():
        if ch in char_weight and not is_pure_simplified(ch):
            for pid in pids:
                series_members[pid].append(ch)

    series_root_char: Dict[str, str] = {}
    for pid, members in series_members.items():
        rad_counter = Counter(KANGXI_RADICALS.get(rs_map.get(m, (0, 0))[0], "") for m in members)
        dominant_rad = rad_counter.most_common(1)[0][0] if rad_counter else ""
        # 優先從正體常用字中挑選筆畫最簡之聲符字根；若聲符本身即為獨立部首字（如 馬、龍、音、包、方），不盲目排除
        trad_common = [m for m in members if not is_pure_simplified(m) and char_weight.get(m, 0) >= 550]
        non_rad = [m for m in trad_common if m != dominant_rad]
        # 唯有當 non_rad 最簡字筆畫 <= dominant_rad 筆畫時才排除 dominant_rad（避免「馬」系誤選罕字「傌」）
        if non_rad and (
            dominant_rad not in trad_common
            or min(strokes_map.get(m, 99) for m in non_rad) <= strokes_map.get(dominant_rad, 99)
        ):
            candidates = non_rad
        elif trad_common:
            candidates = trad_common
        else:
            candidates = [m for m in members if not is_pure_simplified(m)] or members
        series_root_char[pid] = sorted(candidates, key=lambda c: (strokes_map.get(c, 99), -char_weight.get(c, 0)))[0]

    def get_rad_bpm(rad_ch: str) -> str:
        return COLLOQUIAL_RAD_BPM.get(rad_ch, char_bpm.get(rad_ch, ""))

    def get_2key_variants(ch_or_rad: str) -> List[str]:
        bpm = get_rad_bpm(ch_or_rad)
        clean = bpm_strip_tone(bpm)
        res: List[str] = []
        for a in HERITAGE_RAD_ALIAS.get(ch_or_rad, []):
            if a not in res:
                res.append(a)
        nasal_n = bpm_to_nasal_n_2key(bpm)
        if nasal_n not in res:
            res.append(nasal_n)
        ty = bpm_to_tongyong_2key(bpm)
        if ty not in res:
            res.append(ty)
        for pv in bpm_to_palatal_variants_2key(bpm):
            if pv not in res:
                res.append(pv)
        rom = bpm_to_roman_2key(bpm)
        if rom not in res:
            res.append(rom)
        # 舌尖元音（空韻 ㄓ/ㄔ/ㄕ/ㄖ/ㄗ/ㄘ/ㄙ，拼音 zhi/chi/shi/ri/zi/ci/si）同時支援 -i 韻尾（如 吃=koci/kocc、食=sibo/ssbo）
        if len(clean) == 1 and clean[0] in ('ㄓ', 'ㄔ', 'ㄕ', 'ㄖ', 'ㄗ', 'ㄘ', 'ㄙ'):
            apical_i = ROMAN_INIT[clean[0]] + 'i'
            if apical_i not in res:
                res.append(apical_i)
        # 複韻母 ㄞ(ai) / ㄠ(ao) 除頭尾取碼 (-i / -o，如 再=mizi、來=rnli、飽=sibo) 外，亦相容取主元音 -a (如 再=miza、來=rnla、飽=siba)
        if len(clean) >= 2 and clean[0] in ROMAN_INIT and clean[-1] in ('ㄞ', 'ㄠ'):
            open_a = ROMAN_INIT[clean[0]] + 'a'
            if open_a not in res:
                res.append(open_a)
        hsu = bpm_to_hsu_2key(bpm)
        if hsu not in res:
            res.append(hsu)
        return res

    # 儲存 (code, text) -> (weight, kind, breakdown)
    entries_map: Dict[Tuple[str, str], Tuple[int, str, str]] = {}
    char_primary_2key: Dict[str, str] = {}
    char_tail_2key: Dict[str, str] = {}

    def upsert_entry(code: str, text: str, weight: int, kind: str, breakdown: str):
        if not code or not (2 <= len(code) <= 12):
            return
        key = (code.lower(), text)
        prev = entries_map.get(key)
        if prev is None or weight > prev[0]:
            entries_map[key] = (weight, kind, breakdown)

    # 1. 先寫入精心校準之指標字與部件×數字字 (CURATED_SPECIAL_CHARS)
    for ch, spec_list in CURATED_SPECIAL_CHARS.items():
        for code, bdesc, wt in spec_list:
            upsert_entry(code, ch, wt, "curated", bdesc)
            if ch not in char_primary_2key and len(code) >= 2:
                char_primary_2key[ch] = code[:2]
            if ch not in char_tail_2key and len(code) == 4:
                char_tail_2key[ch] = code[2:4]

    # 1.5 載入 Unicode IDS (Ideographic Description Sequences) 筆順結構表，
    #     支援「依筆順拆解為 2 個可讀部件 (C1+C2)，若為 3~5 個可讀部件則取首尾可讀部件 (C1+Ck)」之直覺正軌
    RAD_TO_READABLE: Dict[str, str] = {
        "龵": "手", "扌": "手", "亻": "人", "彳": "彳", "氵": "水", "冫": "冰",
        "忄": "心", "⺗": "心", "刂": "刀", "钅": "金", "釒": "金", "訁": "言", "讠": "言",
        "飠": "食", "饣": "食", "𩙿": "食", "糹": "糸", "纟": "糸", "艹": "艸", "䒑": "艸",
        "辶": "辵", "⻌": "辵", "阝": "阜", "⺼": "月", "灬": "火", "牜": "牛",
        "犭": "犬", "礻": "示", "衤": "衣", "耂": "老", "⺮": "竹", "⻊": "足", "⻗": "雨",
        "攵": "攴", "夂": "夊", "夊": "夊", "尢": "尢", "尣": "尢", "户": "戶",
        "丷": "八", "冖": "冖", "宀": "宀", "广": "广", "疒": "疒",
        "虍": "虎", "覀": "西", "⺻": "聿", "囗": "口", "亼": "人", "亽": "人",
        "𠮛": "口", "开": "井", "𦍌": "羊", "⺶": "羊", "⺷": "羊",
        "爫": "爪", "⺤": "爪", "罒": "四", "𦥑": "臼", "勹": "勹", "𠂊": "勹", "⺈": "勹",
        "卩": "卩", "⺋": "卩", "㔾": "卩", "厂": "厂", "厶": "厶", "廴": "延",
        "廾": "共", "彐": "雪", "⺕": "雪", "彡": "彡", "𫠠": "戈", "镸": "長",
        "𠫓": "𠫓", "丂": "丂", "丩": "丩", "乚": "乚", "𠃌": "乚", "㐅": "乂",
        "乡": "幺", "𢆶": "幺",
    }
    BOPOMOFO_COMP_MAP: Dict[str, Tuple[str, str, List[str]]] = {
        "勹": ("ㄅ", "b", ["b1", "bo", "bb"]),
        "攴": ("ㄆ", "p", ["p1", "po", "pu", "pp"]),
        "夊": ("ㄆ", "p", ["p1", "po", "pp"]),
        "冂": ("ㄇ", "m", ["m1", "mo", "mm", "mi"]),
        "冖": ("ㄇ", "m", ["m1", "mo", "mm", "mu"]),
        "宀": ("ㄇ", "m", ["m1", "mo", "mm", "mn"]),
        "匚": ("ㄈ", "f", ["f1", "fo", "fn", "ff"]),
        "匸": ("ㄈ", "f", ["f1", "fo", "ff", "xi"]),
        "𠫓": ("ㄊ", "t", ["t1", "te", "tt", "tu"]),
        "乃": ("ㄋ", "n", ["n1", "ni", "ne", "nn"]),
        "巛": ("ㄍ", "g", ["g1", "ge", "gg", "cn"]),
        "巜": ("ㄍ", "g", ["g1", "ge", "gg"]),
        "丂": ("ㄎ", "k", ["k1", "ke", "ko", "kk"]),
        "厂": ("ㄏ", "h", ["h1", "he", "hn", "hh"]),
        "丩": ("ㄐ", "j", ["j1", "ji", "jj"]),
        "彳": ("ㄔ", "c", ["c1", "ci", "cc"]),
        "尸": ("ㄕ", "s", ["s1", "si", "ss"]),
        "卩": ("ㄗ", "z", ["z1", "zi", "zz", "je"]),
        "厶": ("ㄙ", "s", ["s1", "si", "ss"]),
        "乂": ("ㄨ", "u", ["wu", "u1", "uu"]),
        "凵": ("ㄩ", "v", ["yu", "v1", "vv", "kn"]),
        "幺": ("ㄠ", "a", ["ao", "yo", "a1", "aa", "ew", "ww"]),
        "么": ("ㄠ", "a", ["ao", "yo", "me", "a1", "aa", "ew", "ww"]),
        "又": ("ㄡ", "o", ["yo", "ou", "o1", "oo"]),
        "乚": ("ㄣ", "n", ["n1", "en", "nn", "yi"]),
        "尢": ("ㄤ", "a", ["an", "wn", "yo", "a1", "aa"]),
        "儿": ("ㄦ", "r", ["er", "rn", "r1", "rr"]),
    }
    LATIN_COMP_MAP: Dict[str, str] = {
        "勹": "n", "冂": "n", "冖": "n", "匚": "c", "匸": "c",
        "凵": "u", "乂": "x", "爻": "x", "厂": "f", "卩": "p", "乚": "l",
    }
    MULTI_COMP_OVERRIDE: Dict[str, List[str]] = {
        "合": ["人", "一", "口"],
        "壴": ["士", "豆"],
        "尌": ["士", "豆", "寸"],
        "賣": ["士", "四", "貝"],
        "買": ["四", "貝"],
        "聽": ["耳", "王", "十", "四", "一", "心"],
        "角": ["勹", "用"],
        "免": ["勹", "口", "儿"],
        "兔": ["勹", "口", "儿"],
        "元": ["二", "儿"],
        "光": ["小", "儿"],
        "先": ["牛", "儿"],
        "印": ["爪", "卩"],
        "即": ["艮", "卩"],
        "卸": ["午", "止", "卩"],
    }
    NO_DECOMPOSE_ATOMS: Set[str] = set("七九丸凡刀力方成司局毛电甩匕儿夕乃久乞也习勺勻勿")
    ATOMIC_READABLE: Set[str] = set(
        "一二三四五六七八九十百千萬上下左右中大小多少人入儿女孑子口日曰月木水火土金心手目耳舌牙齒足身首面頁骨肉血皮毛羽爪言音食衣巾糸艸竹禾米豆瓜果貝車馬牛羊犬豕鹿魚鳥虫龍龜刀刃匕力弓矢戈矛斤欠攴殳卜又寸工己干广廾弋彡彳戶文方止歹母比氏气父片瓦甘生用田由甲申电疋白皿矢石示穴立缶网羊老而耒聿臣自至臼舟艮色虎血行西見谷走辛辰邑酉采里長門阜隶隹雨青非革韋韭風飛香高髟鬥鬯鬲鬼麥麻黃黍黑鼎鼓鼠鼻齊"
        "包乞甫去旦巴半平正本末未丙央失冊永曲朱此求衣谷赤辛里"
    )
    STROKE_ONLY: Set[str] = set("丨丶丿乙亅乛𡿨乀乁𠃍𠂉𠂇㇏㇐㇑㇒㇓㇔㇕㇖㇗㇘㇙㇚㇛㇜㇝㇞㇟㇠")
    IDS_OPS: Set[str] = set("⿰⿱⿲⿳⿴⿵⿶⿷⿸⿹⿺⿻")

    ids_raw: Dict[str, str] = {}
    if IDS_TXT.exists():
        with open(IDS_TXT, "r", encoding="utf-8") as f:
            for line in f:
                if line.startswith("#") or not line.strip():
                    continue
                p = line.strip().split("\t")
                if len(p) >= 3:
                    chosen = p[2]
                    for cand in p[2:]:
                        if "T" in cand:
                            chosen = cand
                            break
                    chosen = re.sub(r"\[.*?\]", "", chosen)
                    ids_raw[p[1]] = chosen

    def get_ids_children(ch_s: str) -> List[str]:
        s = ids_raw.get(ch_s, "")
        return [c for c in s if c not in IDS_OPS and not c.isascii() and not ("①" <= c <= "⑳")]

    def is_familiar_readable(c_s: str) -> bool:
        if c_s in RAD_TO_READABLE or c_s in ATOMIC_READABLE or c_s in COLLOQUIAL_RAD_BPM:
            return True
        return c_s in char_bpm and char_weight.get(c_s, 0) >= 850

    def decompose_readable_seq(ch_s: str, depth: int = 0) -> List[str]:
        if depth == 0 and ch_s in NO_DECOMPOSE_ATOMS:
            return [ch_s]
        if depth > 0 and ch_s in MULTI_COMP_OVERRIDE:
            return list(MULTI_COMP_OVERRIDE[ch_s])
        if ch_s in MULTI_COMP_OVERRIDE and depth == 0:
            return list(MULTI_COMP_OVERRIDE[ch_s])
        children = get_ids_children(ch_s)
        if not children or len(children) == 1:
            norm = RAD_TO_READABLE.get(ch_s, ch_s)
            return [norm] if norm not in STROKE_ONLY and (norm in char_bpm or norm in COLLOQUIAL_RAD_BPM) else []
        out: List[str] = []
        for c in children:
            if c in STROKE_ONLY:
                continue
            if c in MULTI_COMP_OVERRIDE:
                out.extend(MULTI_COMP_OVERRIDE[c])
            elif c in RAD_TO_READABLE:
                out.append(RAD_TO_READABLE[c])
            elif c in ATOMIC_READABLE:
                out.append(c)
            else:
                sub = decompose_readable_seq(c, depth + 1)
                if len(sub) >= 2:
                    out.extend(sub)
                elif is_familiar_readable(c):
                    out.append(c)
                elif sub:
                    out.extend(sub)
        return out

    def decompose_top2_readable(ch_s: str) -> Optional[Tuple[str, str]]:
        if ch_s in NO_DECOMPOSE_ATOMS:
            return None
        if ch_s in MULTI_COMP_OVERRIDE and len(MULTI_COMP_OVERRIDE[ch_s]) == 2:
            return (MULTI_COMP_OVERRIDE[ch_s][0], MULTI_COMP_OVERRIDE[ch_s][1])
        children = get_ids_children(ch_s)
        if len(children) == 2:
            c1 = RAD_TO_READABLE.get(children[0], children[0])
            c2 = RAD_TO_READABLE.get(children[1], children[1])
            if c1 != ch_s and c2 != ch_s and is_familiar_readable(c1) and is_familiar_readable(c2):
                return (c1, c2)
        return None

    def decompose_visual_4(ch_s: str, depth: int = 0) -> str:
        """徐冰式純視覺英文字母拆字 (龍龕 2.0 軌道 V：外國人零讀音友善軌)"""
        if ch_s in ATOM_VISUAL_SEQ:
            return ATOM_VISUAL_SEQ[ch_s][:4]
        if ch_s in MULTI_COMP_OVERRIDE:
            seq = "".join(ATOM_VISUAL_SEQ.get(c, VISUAL_SHAPE_MAP.get(c, "")) for c in MULTI_COMP_OVERRIDE[ch_s])
            if len(seq) >= 2:
                return seq[:4]
        children = get_ids_children(ch_s)
        if not children:
            return VISUAL_SHAPE_MAP.get(ch_s, "")
        if len(children) == 1:
            return VISUAL_SHAPE_MAP.get(children[0], "")
        parts: List[str] = []
        for c in children:
            if c in ATOM_VISUAL_SEQ:
                parts.append(ATOM_VISUAL_SEQ[c])
            elif c in VISUAL_SHAPE_MAP:
                parts.append(VISUAL_SHAPE_MAP[c])
            elif depth < 3:
                sub = decompose_visual_4(c, depth + 1)
                if sub:
                    parts.append(sub)
        if not parts:
            return ""
        if len(parts) == 2:
            p1, p2 = parts[0], parts[1]
            if len(p1) + len(p2) <= 4:
                return p1 + p2
            if len(p1) == 1:
                return p1 + p2[:2] + p2[-1]
            if len(p2) == 1:
                return p1[:2] + p1[-1] + p2
            return p1[:2] + p2[0] + p2[-1]
        elif len(parts) == 3:
            p1, p2, p3 = parts[0], parts[1], parts[2]
            if len(p1) + len(p2) + len(p3) <= 4:
                return p1 + p2 + p3
            return p1[0] + p2[0] + p3[0] + p3[-1]
        else:
            return "".join(p[0] for p in parts[:3]) + parts[-1][-1]

    # 2. 針對全詞庫單字編譯多軌龍龕碼
    sorted_chars = [ch for ch, _ in char_weight.most_common(13000)]
    for ch in sorted_chars:
        base_wt = max(100, min(2200, char_weight[ch]))
        rad_id, res_strokes = rs_map.get(ch, (0, 0))
        rad_ch = KANGXI_RADICALS.get(rad_id, ch)
        pids = phonetic_map.get(ch, [])
        phon_ch = series_root_char.get(pids[0], ch) if pids else ch
        # 若為部首獨體本字（如 龍、音、入、馬），或聲符筆畫反而大於等於整字筆畫，退回本字讀音
        if ch == rad_ch or res_strokes == 0 or (phon_ch != ch and strokes_map.get(phon_ch, 99) >= strokes_map.get(ch, 99)):
            phon_ch = ch

        r_variants = get_2key_variants(rad_ch)
        p_variants = get_2key_variants(phon_ch)
        c_variants = get_2key_variants(ch)
        r_hsu = bpm_to_hsu_2key(get_rad_bpm(rad_ch))
        p_hsu = bpm_to_hsu_2key(get_rad_bpm(phon_ch))
        c_hsu = bpm_to_hsu_2key(get_rad_bpm(ch))

        if ch not in char_primary_2key and r_variants:
            char_primary_2key[ch] = r_variants[0]
        is_obscure_phon = (
            phon_ch != ch
            and char_weight.get(phon_ch, 0) < 950
            and (not p_variants or not c_variants or p_variants[0] != c_variants[0])
        )
        if ch not in char_tail_2key:
            if is_obscure_phon and c_variants:
                char_tail_2key[ch] = c_variants[0][:2]
            elif p_variants:
                char_tail_2key[ch] = p_variants[0][:2]

        # 軌道 S（筆順可讀部件正軌）：
        # - 若依筆順可直接拆為 2 個常見可讀部件 (C1, C2)（如 看=手+目、問=門+口、和=禾+口、到=至+刀），直接取 C1(2碼)+C2(2碼) 作為正軌主碼！
        # - 若無法拆為 2 個可讀部件，而是拆成 3~5 個可讀部件序列 [C1, C2, ..., Ck]（如 喜=士➔豆➔口、識=言➔音➔戈、聽=耳➔...➔心、樹=木➔士➔豆➔寸、謝=言➔身➔寸），
        #   則取「第一個可讀部件 C1(2碼) ＋ 最後一個可讀部件 Ck(2碼)」組合！
        top2_pair = decompose_top2_readable(ch)
        read_seq = decompose_readable_seq(ch)
        first_last_pair = (read_seq[0], read_seq[-1]) if len(read_seq) >= 2 and (read_seq[0] != ch or read_seq[-1] != ch) else None

        stroke_pairs_to_emit: List[Tuple[str, str, str, int]] = []
        if top2_pair is not None:
            stroke_pairs_to_emit.append((top2_pair[0], top2_pair[1], "stroke_bin", 250))
            if first_last_pair is not None and first_last_pair != top2_pair and len(read_seq) >= 3:
                chain_str = "➔".join(read_seq[:5])
                stroke_pairs_to_emit.append((first_last_pair[0], first_last_pair[1], f"首尾部件({chain_str})", 238))
        elif first_last_pair is not None:
            if len(read_seq) >= 3:
                chain_str = "➔".join(read_seq[:5])
                stroke_pairs_to_emit.append((first_last_pair[0], first_last_pair[1], f"首尾部件({chain_str})", 252))
            else:
                stroke_pairs_to_emit.append((first_last_pair[0], first_last_pair[1], "stroke_bin", 250))

        for comp1, comp2, s_tag, bonus_top in stroke_pairs_to_emit:
            v1_list = get_2key_variants(comp1)
            v2_list = get_2key_variants(comp2)
            v1_hsu = bpm_to_hsu_2key(get_rad_bpm(comp1))
            v2_hsu = bpm_to_hsu_2key(get_rad_bpm(comp2))
            v1_ty = bpm_to_tongyong_2key(get_rad_bpm(comp1))
            v2_ty = bpm_to_tongyong_2key(get_rad_bpm(comp2))
            for i1, k1 in enumerate(v1_list):
                for i2, k2 in enumerate(v2_list):
                    code4 = (k1 + k2)[:4]
                    if i1 == 0 and i2 == 0:
                        wt_s = base_wt + bonus_top
                        if s_tag == "stroke_bin":
                            bdesc_s = f"{comp1}({k1})+{comp2}({k2})"
                        else:
                            bdesc_s = f"{s_tag}:{comp1}({k1})+尾部{comp2}({k2})"
                    elif k1 == v1_hsu and k2 == v2_hsu:
                        wt_s = base_wt + bonus_top - 30
                        bdesc_s = f"許氏雙拼:{comp1}({k1})+許氏{comp2}({k2})"
                    elif k1 == v1_ty and k2 == v2_ty:
                        wt_s = base_wt + 145
                        bdesc_s = f"通用拼音:{comp1}({k1})+{comp2}({k2})"
                    elif (k1 == v1_hsu and i1 > 0 and i2 == 0) or (k2 == v2_hsu and i2 > 0 and i1 == 0):
                        wt_s = base_wt + 75
                        bdesc_s = f"雙拼混搭:{comp1}({k1})+許氏{comp2}({k2})"
                    else:
                        wt_s = base_wt + 65
                        bdesc_s = f"筆順相容:{comp1}({k1})+{comp2}({k2})"
                    upsert_entry(code4, ch, wt_s, "stroke_comp", bdesc_s)
            # 軌道 S-BPM & S-LAT：注音同形部件（如 負=ㄅ+貝 -> bbe / b1be / bobe / bbbe；麼=麻+ㄠ -> maao / maa1）
            # 與英文字母同形部件（如 負=n+貝 -> nbe / nnbe）
            if comp1 in BOPOMOFO_COMP_MAP and v2_list:
                bpm1_sym, k1_single, k1_twos = BOPOMOFO_COMP_MAP[comp1]
                k2_0 = v2_list[0][:2]
                upsert_entry(
                    f"{k1_single}{k2_0}",
                    ch,
                    base_wt + bonus_top + 5,
                    "bpm_comp",
                    f"注音部件:{comp1}({k1_single})+{comp2}({k2_0})+␣",
                )
                for idx_b, k1_2 in enumerate(k1_twos):
                    wt_b = base_wt + bonus_top + (4 if k1_2.endswith("1") else (-2 if idx_b == 0 else -8))
                    upsert_entry(
                        f"{k1_2}{k2_0}",
                        ch,
                        wt_b,
                        "bpm_comp",
                        f"注音部件:{comp1}({k1_2})+{comp2}({k2_0})",
                    )
            if comp2 in BOPOMOFO_COMP_MAP and v1_list:
                bpm2_sym, k2_single, k2_twos = BOPOMOFO_COMP_MAP[comp2]
                k1_0 = v1_list[0][:2]
                upsert_entry(
                    f"{k1_0}{k2_single}",
                    ch,
                    base_wt + bonus_top + 5,
                    "bpm_comp",
                    f"注音部件:{comp1}({k1_0})+{comp2}({k2_single})+␣",
                )
                for idx_b, k2_2 in enumerate(k2_twos):
                    wt_b = base_wt + bonus_top + (4 if k2_2.endswith("1") else (-2 if idx_b == 0 else -8))
                    upsert_entry(
                        f"{k1_0}{k2_2}",
                        ch,
                        wt_b,
                        "bpm_comp",
                        f"注音部件:{comp1}({k1_0})+{comp2}({k2_2})",
                    )
            if comp1 in LATIN_COMP_MAP and v2_list:
                lat1 = LATIN_COMP_MAP[comp1]
                k2_0 = v2_list[0][:2]
                upsert_entry(
                    f"{lat1}{k2_0}",
                    ch,
                    base_wt + bonus_top + 3,
                    "latin_comp",
                    f"字母部件:{comp1}({lat1})+{comp2}({k2_0})+␣",
                )
                upsert_entry(
                    f"{lat1}{lat1}{k2_0}",
                    ch,
                    base_wt + bonus_top - 6,
                    "latin_comp",
                    f"字母部件:{comp1}({lat1}{lat1})+{comp2}({k2_0})",
                )
            if comp2 in LATIN_COMP_MAP and v1_list:
                lat2 = LATIN_COMP_MAP[comp2]
                k1_0 = v1_list[0][:2]
                upsert_entry(
                    f"{k1_0}{lat2}",
                    ch,
                    base_wt + bonus_top + 3,
                    "latin_comp",
                    f"字母部件:{comp1}({k1_0})+{comp2}({lat2})+␣",
                )
                upsert_entry(
                    f"{k1_0}{lat2}{lat2}",
                    ch,
                    base_wt + bonus_top - 6,
                    "latin_comp",
                    f"字母部件:{comp1}({k1_0})+{comp2}({lat2}{lat2})",
                )
            # 同步提供筆順首尾部件之 2+1+調 (如 看 = 手(so)+目(m)+4聲 -> som4)
            c_num_t, _ = get_tone_pair(char_bpm.get(ch, ""))
            if v1_list and v2_list and c_num_t:
                k1_0 = v1_list[0][:2]
                k2_init = v2_list[0][0]
                upsert_entry(
                    f"{k1_0}{k2_init}{c_num_t}",
                    ch,
                    base_wt + 135,
                    "rad2_phon1_tone",
                    f"2+1+調:{comp1}({k1_0})+{comp2}({k2_init})+{c_num_t}聲",
                )

        # 軌道 A1：部首音(2碼) + 聲符音(2碼) (如 鋪 = 金jn + 甫fu -> jnfu / jnfx)
        for idx_r, rc in enumerate(r_variants):
            for idx_p, pc in enumerate(p_variants):
                code4 = (rc + pc)[:4]
                if idx_r == 0 and idx_p == 0:
                    wt_adj = base_wt + (190 if is_obscure_phon else 180)
                elif (rc == r_hsu and pc == p_hsu):
                    wt_adj = base_wt + (120 if is_obscure_phon else 130)
                elif idx_r == 0:
                    wt_adj = base_wt + 70
                elif idx_p == 0:
                    wt_adj = base_wt + 55
                else:
                    wt_adj = base_wt + 35
                if rad_ch == phon_ch:
                    if idx_r == 0 and idx_p == 0:
                        bdesc = f"獨體疊音:{rad_ch}({rc})+{phon_ch}({pc})"
                    elif rc == pc and rc == r_hsu:
                        bdesc = f"許氏疊音:{rad_ch}({rc})+{phon_ch}({pc})"
                    else:
                        bdesc = f"雙拼混搭:{rad_ch}({rc})+{phon_ch}({pc})"
                else:
                    if idx_r == 0 and idx_p == 0:
                        if is_obscure_phon:
                            bdesc = f"右旁部件:{rad_ch}({rc})+右旁{phon_ch}({pc})"
                        else:
                            bdesc = f"{rad_ch}({rc})+{phon_ch}({pc})"
                    elif (rc == r_hsu and pc == p_hsu) or (rc == r_hsu and idx_r > 0 and idx_p == 0) or (pc == p_hsu and idx_p > 0 and idx_r == 0):
                        bdesc = f"許氏部件:{rad_ch}({rc})+右旁{phon_ch}({pc})"
                    else:
                        bdesc = f"部件相容:{rad_ch}({rc})+右旁{phon_ch}({pc})"
                upsert_entry(code4, ch, wt_adj, "rad_phon", bdesc)

        # 軌道 A2：部首音(2碼) + 整字本音(2碼) (解決古今音變與罕見聲符，如 銀 = 金(jn)+銀(in)->jnin / 許氏金(jn)+許氏銀(en)->jnen)
        if phon_ch != ch:
            for idx_r, rc in enumerate(r_variants):
                for idx_c, cc in enumerate(c_variants):
                    code4 = (rc + cc)[:4]
                    if idx_r == 0 and idx_c == 0:
                        wt_adj = base_wt + (225 if is_obscure_phon else 135)
                        bdesc = f"{rad_ch}({rc})+{ch}({cc})" if is_obscure_phon else f"本音直覺:{rad_ch}({rc})+本音{ch}({cc})"
                    elif rc == r_hsu and cc == c_hsu:
                        wt_adj = base_wt + (205 if is_obscure_phon else 115)
                        bdesc = f"許氏雙拼:{rad_ch}({rc})+許氏{ch}({cc})"
                    elif (rc == r_hsu and idx_r > 0 and idx_c == 0) or (cc == c_hsu and idx_c > 0 and idx_r == 0):
                        wt_adj = base_wt + (195 if is_obscure_phon else 85)
                        bdesc = f"許氏雙拼:{rad_ch}({rc})+許氏{ch}({cc})"
                    else:
                        wt_adj = base_wt + 55
                        bdesc = f"本音相容:{rad_ch}({rc})+本音{ch}({cc})"
                    upsert_entry(code4, ch, wt_adj, "rad_char", bdesc)

            # 軌道 A3（2+1+調 並存軌）：前部件(2碼) + 後部件首音(1碼) + 聲調數字(1..5) (如 音=lir4、明=riy4、銀=jni2/jne2)
            c_num_t, _ = get_tone_pair(char_bpm.get(ch, ""))
            p_num_t, _ = get_tone_pair(char_bpm.get(phon_ch, ""))
            if r_variants and c_variants and c_num_t:
                rc0 = r_variants[0][:2]
                cc_init = c_variants[0][0]
                upsert_entry(
                    f"{rc0}{cc_init}{c_num_t}",
                    ch,
                    base_wt + 120,
                    "rad2_phon1_tone",
                    f"2+1+調:{rad_ch}({rc0})+{ch}({cc_init})+{c_num_t}聲",
                )
                if c_hsu and c_hsu[0] != cc_init:
                    upsert_entry(
                        f"{r_hsu}{c_hsu[0]}{c_num_t}",
                        ch,
                        base_wt + 105,
                        "rad2_phon1_tone",
                        f"許氏友善:{rad_ch}({r_hsu})+{ch}({c_hsu[0]})+{c_num_t}聲",
                    )
            if r_variants and p_variants:
                rc0 = r_variants[0][:2]
                pc_init = p_variants[0][0]
                for t_num in (c_num_t, p_num_t):
                    if t_num:
                        code_21t = f"{rc0}{pc_init}{t_num}"
                        upsert_entry(
                            code_21t,
                            ch,
                            base_wt + 90,
                            "rad2_phon1_tone",
                            f"2+1+調:{rad_ch}({rc0})+{phon_ch}({pc_init})+{t_num}聲",
                        )

        # 軌道 B：視覺英文字母象形(1碼) + 聲符/本字音(2碼) [+ 選用第4碼聲調 12345 / dfjs]
        if rad_ch in VISUAL_PICTOGRAPH_RADICALS:
            vk = VISUAL_PICTOGRAPH_RADICALS[rad_ch]
            c_num_t, c_key_t = get_tone_pair(char_bpm.get(ch, ""))
            p_num_t, p_key_t = get_tone_pair(char_bpm.get(phon_ch, ""))
            for pc in (p_variants[:2] + c_variants[:1]):
                code3 = f"{vk}{pc[:2]}"
                bdesc3 = f"象形{rad_ch}({vk})+{phon_ch}({pc[:2]})"
                upsert_entry(code3, ch, base_wt + 60, "pict_3", bdesc3)
                for t_pad in (c_num_t, c_key_t, p_num_t, p_key_t):
                    if t_pad:
                        code4 = f"{code3}{t_pad}"
                        upsert_entry(code4, ch, base_wt + 90, "pict_4", f"{bdesc3}+調({t_pad})")

        # 軌道 E：許氏注音友善全拼＋聲調數字 4 碼直通（如 龍 = ㄌl+ㄨx+ㄥl+2聲 -> lxl2）
        hsu_full = bpm_to_hsu_full(char_bpm.get(ch, ""))
        if len(hsu_full) == 3:
            c_num_t, _ = get_tone_pair(char_bpm.get(ch, ""))
            if c_num_t:
                hsu_t4 = f"{hsu_full}{c_num_t}"
                upsert_entry(
                    hsu_t4,
                    ch,
                    base_wt + 50,
                    "hsu_tone4",
                    f"許氏友善:{hsu_full}+{c_num_t}聲",
                )

        # 軌道 H & P（龍龕 2.0 變長部件全注音／全拼音免縮碼軌，首部件免打聲調，以聲調鍵或空白鍵定界）
        c_num_t, c_key_t = get_tone_pair(char_bpm.get(ch, ""))
        full_comp_pairs: List[Tuple[str, str, int]] = []
        for comp1, comp2, _, bonus_top in stroke_pairs_to_emit:
            full_comp_pairs.append((comp1, comp2, bonus_top + 25))
        if (rad_ch, phon_ch) not in [(a, b) for a, b, _ in full_comp_pairs]:
            full_comp_pairs.append((rad_ch, phon_ch, 195))
        if phon_ch != ch and (rad_ch, ch) not in [(a, b) for a, b, _ in full_comp_pairs]:
            full_comp_pairs.append((rad_ch, ch, 175))

        for comp1, comp2, f_bonus in full_comp_pairs:
            b1 = get_rad_bpm(comp1)
            b2 = get_rad_bpm(comp2)
            h1 = bpm_to_hsu_full(b1)
            h2 = bpm_to_hsu_full(b2)
            if h1 and h2 and 2 <= len(h1 + h2) <= 8:
                h_code = h1 + h2
                h_desc = f"許氏全拼:{comp1}({h1})+{comp2}({h2})"
                upsert_entry(h_code, ch, base_wt + f_bonus, "hsu_full_comp", h_desc)
                if c_num_t:
                    upsert_entry(
                        f"{h_code}{c_num_t}",
                        ch,
                        base_wt + f_bonus + 15,
                        "hsu_full_comp",
                        f"{h_desc}+{c_num_t}聲",
                    )
                if c_key_t:
                    upsert_entry(
                        f"{h_code}{c_key_t}",
                        ch,
                        base_wt + f_bonus + 15,
                        "hsu_full_comp",
                        f"{h_desc}+{c_key_t}調",
                    )
            p1 = bpm_to_pinyin_full(b1)
            p2 = bpm_to_pinyin_full(b2)
            if p1 and p2 and 3 <= len(p1 + p2) <= 12:
                p_code = p1 + p2
                p_desc = f"拼音全拼:{comp1}({p1})+{comp2}({p2})"
                upsert_entry(p_code, ch, base_wt + f_bonus - 5, "pinyin_full_comp", p_desc)
                if c_num_t:
                    upsert_entry(
                        f"{p_code}{c_num_t}",
                        ch,
                        base_wt + f_bonus + 10,
                        "pinyin_full_comp",
                        f"{p_desc}+{c_num_t}聲",
                    )

        # 三部件全拼支援（如 謝 = 言(em)+身(cn)+寸(am) -> emcnam；喜 = 士(c)+豆(do)+口(ko) -> cdoko）
        if len(read_seq) == 3:
            c1, c2, c3 = read_seq[0], read_seq[1], read_seq[2]
            h1, h2, h3 = bpm_to_hsu_full(get_rad_bpm(c1)), bpm_to_hsu_full(get_rad_bpm(c2)), bpm_to_hsu_full(get_rad_bpm(c3))
            if h1 and h2 and h3 and 3 <= len(h1 + h2 + h3) <= 10:
                h3_code = h1 + h2 + h3
                h3_desc = f"許氏三部件:{c1}({h1})+{c2}({h2})+{c3}({h3})"
                upsert_entry(h3_code, ch, base_wt + 275, "hsu_full_comp", h3_desc)
                if c_num_t:
                    upsert_entry(f"{h3_code}{c_num_t}", ch, base_wt + 290, "hsu_full_comp", f"{h3_desc}+{c_num_t}聲")
                if c_key_t:
                    upsert_entry(f"{h3_code}{c_key_t}", ch, base_wt + 290, "hsu_full_comp", f"{h3_desc}+{c_key_t}調")
            p1, p2, p3 = bpm_to_pinyin_full(get_rad_bpm(c1)), bpm_to_pinyin_full(get_rad_bpm(c2)), bpm_to_pinyin_full(get_rad_bpm(c3))
            if p1 and p2 and p3 and 4 <= len(p1 + p2 + p3) <= 12:
                p3_code = p1 + p2 + p3
                p3_desc = f"拼音三部件:{c1}({p1})+{c2}({p2})+{c3}({p3})"
                upsert_entry(p3_code, ch, base_wt + 270, "pinyin_full_comp", p3_desc)
                if c_num_t:
                    upsert_entry(f"{p3_code}{c_num_t}", ch, base_wt + 285, "pinyin_full_comp", f"{p3_desc}+{c_num_t}聲")

        # 軌道 V：徐冰《英文方塊字》式純視覺英文字母碼（外國人零讀音友善軌）
        v_code = decompose_visual_4(ch)
        if v_code and 2 <= len(v_code) <= 4:
            v_desc = f"視覺拆字:{ch}({v_code})"
            upsert_entry(v_code, ch, base_wt + 95, "visual_4", v_desc)
            if c_num_t:
                upsert_entry(f"{v_code}{c_num_t}", ch, base_wt + 115, "visual_4", f"{v_desc}+{c_num_t}聲")
            if c_key_t:
                upsert_entry(f"{v_code}{c_key_t}", ch, base_wt + 115, "visual_4", f"{v_desc}+{c_key_t}調")

    # 3. 軌道 D：編譯 Tab 詞彙簡碼模式（主力聚焦三字詞以上之多字詞：3字5碼、4字6碼、5~6字詞，並向下相容2字4碼）
    #    統一公式：【第一字前兩碼 + 中間字第一碼們 + 最後一字後兩碼】（與智慧四角號碼邏輯一致，且尾字取後兩碼聲符可大幅提升熵值、降低重碼！）
    #    因使用者日常單字/雙字連打已極順手，唯有三字詞以上之常用多字詞與成語才會特地啟用 Tab 詞彙簡碼，故大幅擴充 3~6 字詞收錄並賦予優先權重加成。
    for core_phrase, core_wt in [
        ("龍龕形音", 1900),
        ("龍龕形音法", 1900),
        ("如喜輸入法", 1850),
        ("智慧四角號碼", 1850),
        ("中央研究院", 1750),
        ("全民健康保險", 1750),
        ("衛生福利部", 1700),
    ]:
        if word_weight[core_phrase] < core_wt:
            word_weight[core_phrase] = core_wt

    words_2 = [w for w, _ in word_weight.most_common() if len(w) == 2][:8000]
    words_3 = [w for w, _ in word_weight.most_common() if len(w) == 3][:15000]
    words_4 = [w for w, _ in word_weight.most_common() if len(w) == 4][:15000]
    words_5_6 = [w for w, _ in word_weight.most_common() if len(w) in (5, 6)][:4000]

    for w in words_3 + words_4 + words_5_6 + words_2:
        if any(c not in char_bpm for c in w):
            continue
        wt = max(200, min(2000, word_weight[w]))
        len_bonus = 40 if len(w) == 2 else (200 if len(w) == 3 else (280 if len(w) == 4 else 320))
        first_head2 = char_primary_2key.get(w[0], bpm_to_nasal_n_2key(char_bpm[w[0]]))[:2]
        mids_1 = "".join(
            char_primary_2key.get(c, bpm_to_nasal_n_2key(char_bpm[c]))[0]
            for c in w[1:-1]
        )
        last_tail2 = char_tail_2key.get(w[-1], bpm_to_nasal_n_2key(char_bpm[w[-1]]))[:2]
        code_n2 = f"{first_head2}{mids_1}{last_tail2}"
        kind_tag = f"phrase{len(w)}"
        if len(w) == 2:
            bdesc_n2 = f"首前2+尾後2:{w[0]}({first_head2})+{w[1]}({last_tail2})"
        else:
            mid_desc = "+".join(
                f"{c}({char_primary_2key.get(c, bpm_to_nasal_n_2key(char_bpm[c]))[0]})"
                for c in w[1:-1]
            )
            bdesc_n2 = f"首前2+中1+尾後2:{w[0]}({first_head2})+{mid_desc}+{w[-1]}({last_tail2})"
        upsert_entry(code_n2, w, wt + len_bonus, kind_tag, bdesc_n2)

        # 若為 5~6 字長詞，額外提供 6 碼封頂簡碼（第一字前兩碼 + 次字第1碼 + 三字第1碼 + 最後一字後兩碼）免除記長碼負擔
        if len(w) >= 5:
            code_cap6 = f"{first_head2}{mids_1[:2]}{last_tail2}"
            upsert_entry(
                code_cap6,
                w,
                wt + len_bonus - 10,
                kind_tag,
                f"6碼封頂:{w[0]}({first_head2})+{w[1]}({mids_1[0]})+{w[2]}({mids_1[1]})+…+{w[-1]}({last_tail2})",
            )

        # 本音直覺與許氏雙拼詞碼相容（針對多字詞同構展開：首字2碼 + 中字首碼 + 尾字後2碼）
        p_first2 = bpm_to_nasal_n_2key(char_bpm[w[0]])[:2]
        p_mids1 = "".join(bpm_to_nasal_n_2key(char_bpm[c])[0] for c in w[1:-1])
        p_last2 = bpm_to_nasal_n_2key(char_bpm[w[-1]])[:2]
        code_phon = f"{p_first2}{p_mids1}{p_last2}"
        if code_phon != code_n2:
            upsert_entry(code_phon, w, wt + len_bonus - 30, kind_tag, f"本音詞碼:{w}({code_phon})")

        h_first2 = bpm_to_hsu_2key(char_bpm[w[0]])[:2]
        h_mids1 = "".join(bpm_to_hsu_2key(char_bpm[c])[0] for c in w[1:-1])
        h_last2 = bpm_to_hsu_2key(char_bpm[w[-1]])[:2]
        code_hsu = f"{h_first2}{h_mids1}{h_last2}"
        if code_hsu not in (code_n2, code_phon):
            upsert_entry(code_hsu, w, wt + len_bonus - 50, kind_tag, f"許氏詞碼:{w}({code_hsu})")

    # 排序並寫入開發者私有主檔與 Rust 編譯期混淆二進位檔 (longkan_embedded.bin)
    sorted_rows = sorted(
        entries_map.items(),
        key=lambda item: (item[0][0], -item[1][0], item[0][1])
    )

    header = (
        "# 如喜 (RuhI) 龍龕形音輸入法 (Longkan Morpho-Phonetic 4-Code IME) 核心碼表\n"
        "# 欄位格式：CODE<TAB>TEXT<TAB>WEIGHT<TAB>KIND<TAB>BREAKDOWN\n"
    )

    public_tsv = LEXICON_DIR / "longkan.tsv"
    public_tsv.parent.mkdir(parents=True, exist_ok=True)
    with open(public_tsv, "w", encoding="utf-8", newline="\n") as f:
        f.write(header)
        for (code, text), (wt, kind, bdesc) in sorted_rows:
            f.write(f"{code}\t{text}\t{wt}\t{kind}\t{bdesc}\n")

    print(f"[編譯完成] 龍龕形音輸入法碼表共 {len(sorted_rows):,} 筆條目")
    print(f"  -> 標準明文碼表: {public_tsv}")
    for sample_code in ["kmfu", "jnfx", "helo", "hgll", "xeo", "xeo2", "xeo3", "xeof", "xu", "xu1", "cpn", "cpn3", "cpnf", "cko3", "ko3", "mu3", "tli3", "wsm3"]:
        cands = [
            f"{txt}({bdesc})"
            for (cd, txt), (wt, knd, bdesc) in sorted_rows
            if cd == sample_code
        ][:4]
        print(f"  [{sample_code:<4}] -> {', '.join(cands)}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
