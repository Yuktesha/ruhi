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
ruhi_sandbox/test_phase_4.py
驗證 Phase 4: 動補結構與時態體標記引擎
包含：
1. 可能補語肯定與否定對抗 (看得懂/看不懂, 買得起/買不起, 睡得著/睡不著...)
2. 複合趨向補語 (站起來, 坐下來, 拿出來, 堅持下去...)
3. 時量與動量補語拓撲 (睡了八個小時覺, 住了整整三年, 等了半個小時, 看了好幾遍, 去了好幾次)
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

PHASE_4_CASES = [
    # ── 可能補語 肯定與否定 ──
    ("看得懂", "ㄎㄢˋ ˙ㄉㄜ ㄉㄨㄥˇ"),
    ("看不懂", "ㄎㄢˋ ㄅㄨˋ ㄉㄨㄥˇ"),
    ("看得到", "ㄎㄢˋ ˙ㄉㄜ ㄉㄠˋ"),
    ("看不到", "ㄎㄢˋ ㄅㄨˋ ㄉㄠˋ"),
    ("聽得懂", "ㄊㄧㄥ ˙ㄉㄜ ㄉㄨㄥˇ"),
    ("聽不懂", "ㄊㄧㄥ ㄅㄨˋ ㄉㄨㄥˇ"),
    ("買得起", "ㄇㄞˇ ˙ㄉㄜ ㄑㄧˇ"),
    ("買不起", "ㄇㄞˇ ㄅㄨˋ ㄑㄧˇ"),
    ("吃得飽", "ㄔ ˙ㄉㄜ ㄅㄠˇ"),
    ("吃不飽", "ㄔ ㄅㄨˋ ㄅㄠˇ"),
    ("吃得下", "ㄔ ˙ㄉㄜ ㄒㄧㄚˋ"),
    ("吃不下", "ㄔ ㄅㄨˋ ㄒㄧㄚˋ"),
    ("睡得著", "ㄕㄨㄟˋ ˙ㄉㄜ ㄓㄠˊ"),
    ("睡不著", "ㄕㄨㄟˋ ㄅㄨˋ ㄓㄠˊ"),
    ("受得了", "ㄕㄡˋ ˙ㄉㄜ ㄌㄧㄠˇ"),
    ("受不了", "ㄕㄡˋ ㄅㄨˋ ㄌㄧㄠˇ"),
    ("記得住", "ㄐㄧˋ ˙ㄉㄜ ㄓㄨˋ"),
    ("記不住", "ㄐㄧˋ ㄅㄨˋ ㄓㄨˋ"),
    ("走得動", "ㄗㄡˇ ˙ㄉㄜ ㄉㄨㄥˋ"),
    ("走不動", "ㄗㄡˇ ㄅㄨˋ ㄉㄨㄥˋ"),

    # ── 趨向補語 ──
    ("站起來", "ㄓㄢˋ ㄑㄧˇ ㄌㄞˊ"),
    ("坐下來", "ㄗㄨㄛˋ ㄒㄧㄚˋ ㄌㄞˊ"),
    ("拿出來", "ㄋㄚˊ ㄔㄨ ㄌㄞˊ"),
    ("拿進去", "ㄋㄚˊ ㄐㄧㄣˋ ㄑㄩˋ"),
    ("堅持下去", "ㄐㄧㄢ ㄔˊ ㄒㄧㄚˋ ㄑㄩˋ"),
    ("活下去", "ㄏㄨㄛˊ ㄒㄧㄚˋ ㄑㄩˋ"),
    ("想起來", "ㄒㄧㄤˇ ㄑㄧˇ ㄌㄞˊ"),

    # ── 時量與動量補語 ──
    ("睡了八個小時覺", "ㄕㄨㄟˋ ˙ㄌㄜ ㄅㄚ ˙ㄍㄜ ㄒㄧㄠˇ ㄕˊ ㄐㄧㄠˋ"),
    ("住了整整三年", "ㄓㄨˋ ˙ㄌㄜ ㄓㄥˇ ㄓㄥˇ ㄙㄢ ㄋㄧㄢˊ"),
    ("等了半個小時", "ㄉㄥˇ ˙ㄌㄜ ㄅㄢˋ ˙ㄍㄜ ㄒㄧㄠˇ ㄕˊ"),
    ("看了好幾遍", "ㄎㄢˋ ˙ㄌㄜ ㄏㄠˇ ㄐㄧˇ ㄅㄧㄢˋ"),
    ("去了好幾次", "ㄑㄩˋ ˙ㄌㄜ ㄏㄠˇ ㄐㄧˇ ㄘˋ"),
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
    print("⚡ Phase 4 (動補結構與時態體標記引擎) 驗證開始")
    print("=" * 70)

    passed = 0
    failed = 0
    for expected, bpm_str in PHASE_4_CASES:
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
    print(f"總結: {passed} / {len(PHASE_4_CASES)} 通過 (通過率: {passed / len(PHASE_4_CASES) * 100:.1f}%)")
    return failed == 0

if __name__ == "__main__":
    success = run_tests()
    sys.exit(0 if success else 1)
