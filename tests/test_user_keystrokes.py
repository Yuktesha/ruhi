
import sys
from pathlib import Path
_ROOT = Path(__file__).resolve().parent.parent
_TOOLS = _ROOT / "tools"
for p in [str(_ROOT), str(_TOOLS)]:
    if p not in sys.path:
        sys.path.insert(0, p)
"""
Test user keystroke sequence with updated ruhi-core
"""
import ctypes
import json
import os
import sys

sys.stdout.reconfigure(encoding='utf-8')

def test():
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

    # First clause from user typing:
    # "我已經忘記剛剛要打什麼"
    # user typed: xhf ef jel  xkj jej gk  gk  ewj
    seq = "xhfefjel xkjjejgk gk ewj"
    print("Typing keys:", seq)
    last_res = None
    for ch in seq:
        ptr = lib.ruhi_engine_process_key(engine, ch.encode('ascii'))
        if ptr:
            res_str = ctypes.cast(ptr, ctypes.c_char_p).value.decode('utf-8')
            last_res = json.loads(res_str)

    print("Result preedit:", last_res.get('preedit'))
    print("Result composed:", last_res.get('composed'))
    print("Result candidates:", last_res.get('candidates')[:5] if last_res.get('candidates') else [])

    lib.ruhi_engine_destroy(engine)

if __name__ == "__main__":
    test()
