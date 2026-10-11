
import sys
from pathlib import Path
_ROOT = Path(__file__).resolve().parent.parent
_TOOLS = _ROOT / "tools"
for p in [str(_ROOT), str(_TOOLS)]:
    if p not in sys.path:
        sys.path.insert(0, p)
import ctypes
import json
import os
import sys

sys.stdout.reconfigure(encoding='utf-8')

def run_simulation():
    dll_path = os.path.abspath("C:/ATGprjs/ruhi/ruhi-core/target/release/ruhi.dll")
    lib = ctypes.CDLL(dll_path)

    lib.ruhi_engine_create.restype = ctypes.c_void_p
    lib.ruhi_engine_destroy.argtypes = [ctypes.c_void_p]
    lib.ruhi_engine_load_tsv.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
    lib.ruhi_engine_load_tsv.restype = ctypes.c_int
    lib.ruhi_engine_process_key.argtypes = [ctypes.c_void_p, ctypes.c_char]
    lib.ruhi_engine_process_key.restype = ctypes.c_char_p
    lib.ruhi_free_string.argtypes = [ctypes.c_char_p]

    engine = lib.ruhi_engine_create()
    tsv_path = "C:/ATGprjs/ruhi/ruhi_lexicon/data/lexicon_moe.tsv".encode("utf-8")
    lib.ruhi_engine_load_tsv(engine, tsv_path)

    test_words = [
        ("我", "xhf"),
        ("已經", "efjel"),
        ("忘記", "xkjjej"),
        ("剛剛", "gkgk"),
        ("要", "ewj"),
        ("打", "drf"),
        ("什麼", "vmdms"),
        ("但在", "dnjzyj"),
        ("自然", "zjrnd"),
        ("一次", "e aj"),
    ]

    print("=== Testing individual word sequences ===")
    for expected, keys in test_words:
        lib.ruhi_engine_process_key(engine, b'\x1b')
        output = ""
        for ch in keys:
            ptr = lib.ruhi_engine_process_key(engine, ch.encode('ascii'))
            if ptr:
                res_str = ctypes.cast(ptr, ctypes.c_char_p).value.decode('utf-8')
                res = json.loads(res_str)
                if res.get("action", "").startswith("commit"):
                    output += res.get("committed_chunk", "")
                elif res.get("composed"):
                    output = res.get("composed")
        print(f"Target: {expected:10} | Keys: {keys:10} | Output: {output}")

    lib.ruhi_engine_destroy(engine)

if __name__ == "__main__":
    run_simulation()
