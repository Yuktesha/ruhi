import ctypes
import json
import os
import sys

sys.stdout.reconfigure(encoding='utf-8')

dll_path = os.path.abspath("C:/ATGprjs/ruhi/ruhi-core/target/release/ruhi.dll")
lib = ctypes.CDLL(dll_path)

lib.ruhi_engine_create_with_dir.restype = ctypes.c_void_p
lib.ruhi_engine_create_with_dir.argtypes = [ctypes.c_char_p]
lib.ruhi_engine_destroy.argtypes = [ctypes.c_void_p]
lib.ruhi_engine_process_key.argtypes = [ctypes.c_void_p, ctypes.c_char]
lib.ruhi_engine_process_key.restype = ctypes.c_char_p
lib.ruhi_free_string.argtypes = [ctypes.c_char_p]

data_dir = "C:/ATGprjs/ruhi/ruhi_lexicon/data".encode("utf-8")
engine = lib.ruhi_engine_create_with_dir(data_dir)

# Also load domain, mcbopomofo, etc.
lib.ruhi_engine_load_tsv.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
lib.ruhi_engine_load_tsv.restype = ctypes.c_int
for f in ["lexicon_domain.tsv", "lexicon_mcbopomofo.tsv", "lexicon_moe.tsv"]:
    p = f"C:/ATGprjs/ruhi/ruhi_lexicon/data/{f}".encode("utf-8")
    lib.ruhi_engine_load_tsv(engine, p)

def test_sentence(name, keys):
    lib.ruhi_engine_process_key(engine, b'\x1b')
    last_res = {}
    for ch in keys:
        ptr = lib.ruhi_engine_process_key(engine, ch.encode('ascii'))
        if ptr:
            res_str = ctypes.cast(ptr, ctypes.c_char_p).value.decode('utf-8')
            last_res = json.loads(res_str)
            lib.ruhi_free_string(ptr)
    composed = last_res.get("composed", "")
    preedit = last_res.get("preedit", "")
    print(f"[{name}] Keys: {keys} -> Composed: 「{composed}」 | Preedit: 「{preedit}」")

# Test 1: 後續看他回
# 後: hoj, 續: cuj, 看: kmj, 他: tr (or trd?), 回: hxad
test_sentence("後續看他回 (tr)", "hojcujkmjtrhxad")
test_sentence("看他回 (tr)", "kmjtrhxad")
test_sentence("看他 (tr)", "kmjtr")
test_sentence("他回 (tr)", "trhxad")

# Test 2: 這是所有版圖
test_sentence("這是 (jgjs)", "jgjs")
test_sentence("這世 (jgjs)", "jgjs")
test_sentence("這是所有 (jgjs sxf eof)", "jgjssxfeof")

# Test 3: 重中之重
# 重: jxlj, 中: jxl, 之: j , 重: jxlj
test_sentence("重中之重 (jxlj jxl j jxlj)", "jxljjxljjxlj")

# Test 4: 除了我
# 除: vud, 了: ls, 我: xhf
test_sentence("除了我 (vud ls xhf)", "vudlsxhf")

lib.ruhi_engine_destroy(engine)
