#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import sys
from pathlib import Path
_ROOT = Path(__file__).resolve().parent.parent
_TOOLS = _ROOT / "tools"
for p in [str(_ROOT), str(_TOOLS)]:
    if p not in sys.path:
        sys.path.insert(0, p)
"""
ruhi_sandbox/test_phases_2_and_3.py
驗證 Phase 2（穿戴論元槽位引擎）與 Phase 3（偏正短語遞歸左分支解析與數量詞鏈條）
特別測試許聞廉博士經典五重遞歸小花貓測試集！
使用 ruhi.dll 底層引擎直接仿真鍵盤輸入測試。
"""
import os
import sys
import ctypes
import json
from pathlib import Path

sys.stdout.reconfigure(encoding='utf-8')

def get_hsu_keys_for_syllables(syl_list):
    init_map = {
        'ㄅ': 'b', 'ㄆ': 'p', 'ㄇ': 'm', 'ㄈ': 'f',
        'ㄉ': 'd', 'ㄊ': 't', 'ㄋ': 'n', 'ㄌ': 'l',
        'ㄍ': 'g', 'ㄎ': 'k', 'ㄏ': 'h',
        'ㄐ': 'j', 'ㄑ': 'v', 'ㄒ': 'c',
        'ㄓ': 'j', 'ㄔ': 'v', 'ㄕ': 'c', 'ㄖ': 'r',
        'ㄗ': 'z', 'ㄘ': 'a', 'ㄙ': 's'
    }
    med_map = {'ㄧ': 'e', 'ㄨ': 'x', 'ㄩ': 'u'}
    fin_map = {
        'ㄚ': 'r', 'ㄛ': 'h', 'ㄜ': 'g', 'ㄝ': 'e',
        'ㄞ': 'i', 'ㄟ': 'a', 'ㄠ': 'w', 'ㄡ': 'o',
        'ㄢ': 'm', 'ㄣ': 'n', 'ㄤ': 'k', 'ㄥ': 'l', 'ㄦ': 'l'
    }

    keys = ''
    for s in syl_list:
        s = s.strip()
        if not s: continue
        tone = ' '
        if 'ˊ' in s: tone = 'd'; s = s.replace('ˊ', '')
        elif 'ˇ' in s: tone = 'f'; s = s.replace('ˇ', '')
        elif 'ˋ' in s: tone = 'j'; s = s.replace('ˋ', '')
        elif '˙' in s: tone = 's'; s = s.replace('˙', '')

        res = ''
        i = 0
        if len(s) > 0 and s[0] in init_map:
            res += init_map[s[0]]
            i += 1
        if i < len(s) and s[i] in med_map:
            res += med_map[s[i]]
            i += 1
        if i < len(s) and s[i] in fin_map:
            res += fin_map[s[i]]
            i += 1

        keys += res + tone
    return keys

TEST_CASES = [
    # ── 許聞廉博士經典五重遞歸小花貓測試集 ──
    ("貓", "ㄇㄠ"),
    ("一隻貓", "ㄧ ㄓ ㄇㄠ"),
    ("一隻小花貓", "ㄧ ㄓ ㄒㄧㄠˇ ㄏㄨㄚ ㄇㄠ"),
    ("一隻頭上戴著花圈的小花貓", "ㄧ ㄓ ㄊㄡˊ ㄕㄤˋ ㄉㄞˋ ㄓㄜ˙ ㄏㄨㄚ ㄑㄩㄢ ㄉㄜ˙ ㄒㄧㄠˇ ㄏㄨㄚ ㄇㄠ"),
    ("一隻頭上戴著花環身上有玳瑁花紋的小花貓", "ㄧ ㄓ ㄊㄡˊ ㄕㄤˋ ㄉㄞˋ ㄓㄜ˙ ㄏㄨㄚ ㄏㄨㄢˊ ㄕㄣ ㄕㄤˋ ㄧㄡˇ ㄉㄞˋ ㄇㄟˋ ㄏㄨㄚ ㄨㄣˊ ㄉㄜ˙ ㄒㄧㄠˇ ㄏㄨㄚ ㄇㄠ"),
    ("一隻頭上戴著花環身上有玳瑁花紋的小花貓(白讀)", "ㄧ ㄓ ㄊㄡˊ ㄕㄤˋ ㄉㄞˋ ㄓㄜ˙ ㄏㄨㄚ ㄏㄨㄢˊ ㄕㄣ ㄕㄤˋ ㄧㄡˇ ㄉㄞˋ ㄇㄠˋ ㄏㄨㄚ ㄨㄣˊ ㄉㄜ˙ ㄒㄧㄠˇ ㄏㄨㄚ ㄇㄠ"),

    # ── Phase 2: 穿戴論元槽位測試集 ──
    ("頭上戴著安全帽", "ㄊㄡˊ ㄕㄤˋ ㄉㄞˋ ㄓㄜ˙ ㄢ ㄑㄩㄢˊ ㄇㄠˋ"),
    ("頭上戴著口罩", "ㄊㄡˊ ㄕㄤˋ ㄉㄞˋ ㄓㄜ˙ ㄎㄡˇ ㄓㄠˋ"),
    ("臉上戴著眼鏡", "ㄌㄧㄢˇ ㄕㄤˋ ㄉㄞˋ ㄓㄜ˙ ㄧㄢˇ ㄐㄧㄥˋ"),
    ("身上穿著外套", "ㄕㄣ ㄕㄤˋ ㄔㄨㄢ ㄓㄜ˙ ㄨㄞˋ ㄊㄠˋ"),
    ("身上穿著雨衣", "ㄕㄣ ㄕㄤˋ ㄔㄨㄢ ㄓㄜ˙ ㄩˇ ㄧ"),
    ("肩上背著書包", "ㄐㄧㄢ ㄕㄤˋ ㄅㄟ ㄓㄜ˙ ㄕㄨ ㄅㄠ"),
    ("肩膀背著吉他", "ㄐㄧㄢ ㄅㄤˇ ㄅㄟ ㄓㄜ˙ ㄐㄧˊ ㄊㄚ"),
    ("胸前佩戴徽章", "ㄒㄩㄥ ㄑㄧㄢˊ ㄆㄟˋ ㄉㄞˋ ㄏㄨㄟ ㄓㄤ"),
    ("手上提著公事包", "ㄕㄡˇ ㄕㄤˋ ㄊㄧˊ ㄓㄜ˙ ㄍㄨㄥ ㄕˋ ㄅㄠ"),
    ("手腕戴著手錶", "ㄕㄡˇ ㄨㄢˋ ㄉㄞˋ ㄓㄜ˙ ㄕㄡˇ ㄅㄧㄠˇ"),
]

def run_tests():
    dll_path = os.path.abspath("C:/ATGprjs/ruhi/ruhi-core/target/release/ruhi.dll")
    lib = ctypes.CDLL(dll_path)
    lib.ruhi_engine_create.restype = ctypes.c_void_p
    lib.ruhi_engine_load_tsv.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
    lib.ruhi_engine_load_transitions_tsv.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
    lib.ruhi_engine_load_transitions_tsv.restype = ctypes.c_int
    lib.ruhi_engine_process_key.argtypes = [ctypes.c_void_p, ctypes.c_char]
    lib.ruhi_engine_process_key.restype = ctypes.c_char_p
    lib.ruhi_engine_reset.argtypes = [ctypes.c_void_p]

    engine = lib.ruhi_engine_create()
    tsv_path = "C:/ATGprjs/ruhi/ruhi_lexicon/data/lexicon_moe.tsv".encode("utf-8")
    lib.ruhi_engine_load_tsv(engine, tsv_path)

    dom_path = "C:/ATGprjs/ruhi/ruhi_lexicon/data/lexicon_domain.tsv".encode("utf-8")
    if os.path.exists("C:/ATGprjs/ruhi/ruhi_lexicon/data/lexicon_domain.tsv"):
        lib.ruhi_engine_load_tsv(engine, dom_path)

    trans_path = "C:/ATGprjs/ruhi/ruhi_lexicon/data/transitions_user.tsv".encode("utf-8")
    if os.path.exists("C:/ATGprjs/ruhi/ruhi_lexicon/data/transitions_user.tsv"):
        lib.ruhi_engine_load_transitions_tsv(engine, trans_path)

    print("=" * 70)
    print("🐱 Phase 2 (穿戴槽位) & Phase 3 (許聞廉博士小花貓偏正短語) 驗證開始")
    print("=" * 70)

    passed = 0
    failed = 0
    for expected, bpm_str in TEST_CASES:
        clean_exp = expected.split("(")[0]
        syls = bpm_str.split()
        keys = get_hsu_keys_for_syllables(syls)

        lib.ruhi_engine_reset(engine)
        output = ""
        for k in keys:
            ptr = lib.ruhi_engine_process_key(engine, k.encode('ascii'))
            if ptr:
                res = json.loads(ctypes.cast(ptr, ctypes.c_char_p).value.decode('utf-8'))
                if res.get("action", "").startswith("commit"):
                    output += res.get("committed_chunk", "")
                elif res.get("composed"):
                    output = res.get("composed")

        is_match = (clean_exp == output)
        if is_match:
            passed += 1
            print(f"✅ PASS: [{clean_exp}]")
        else:
            failed += 1
            print(f"❌ FAIL: 目標 [{clean_exp}] != 輸出 [{output}] (keys: '{keys}')")

    print("=" * 70)
    print(f"總結: {passed} / {len(TEST_CASES)} 通過 (通過率: {passed / len(TEST_CASES) * 100:.1f}%)")
    return failed == 0

if __name__ == "__main__":
    success = run_tests()
    sys.exit(0 if success else 1)
