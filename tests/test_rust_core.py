
import sys
from pathlib import Path
_ROOT = Path(__file__).resolve().parent.parent
_TOOLS = _ROOT / "tools"
for p in [str(_ROOT), str(_TOOLS)]:
    if p not in sys.path:
        sys.path.insert(0, p)
"""
Test ruhi-core C-ABI integration via ctypes
"""
import ctypes
import json
import os
import sys

sys.stdout.reconfigure(encoding='utf-8')

def test_rust_core():
    dll_path = os.path.abspath("C:/ATGprjs/ruhi/ruhi-core/target/release/ruhi.dll")
    if not os.path.exists(dll_path):
        print(f"Error: {dll_path} not found")
        return

    lib = ctypes.CDLL(dll_path)

    # Set argument & return types
    lib.ruhi_engine_create.restype = ctypes.c_void_p
    lib.ruhi_engine_destroy.argtypes = [ctypes.c_void_p]

    lib.ruhi_engine_load_tsv.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
    lib.ruhi_engine_load_tsv.restype = ctypes.c_int

    lib.ruhi_engine_process_key.argtypes = [ctypes.c_void_p, ctypes.c_char]
    lib.ruhi_engine_process_key.restype = ctypes.c_char_p

    lib.ruhi_free_string.argtypes = [ctypes.c_char_p]

    print("1. Creating Rust RuhiEngine instance...")
    engine = lib.ruhi_engine_create()
    assert engine is not None

    tsv_path = "C:/ATGprjs/ruhi/ruhi_lexicon/data/lexicon_moe.tsv".encode("utf-8")
    print(f"2. Loading Lexicon TSV from {tsv_path.decode('utf-8')}...")
    count = lib.ruhi_engine_load_tsv(engine, tsv_path)
    print(f"   Loaded {count} entries into Rust RuhiEngine!")

    # Test typing: "如" = r (ㄖ) + x (ㄨ) + d (ˊ) -> ㄖㄨˊ
    # "喜" = v (ㄒ) + e (ㄧ) + f (ˇ) -> ㄒㄧˇ
    print("3. Testing Hsu dual-stroke for '如喜': 'r', 'x', 'd', 'v', 'e', 'f' ...")
    
    for key in ['r', 'x', 'd', 'v', 'e', 'f']:
        ptr = lib.ruhi_engine_process_key(engine, key.encode('ascii'))
        if ptr:
            res_str = ctypes.cast(ptr, ctypes.c_char_p).value.decode('utf-8')
            res = json.loads(res_str)
            print(f"   Key '{key}' -> action: {res.get('action')}, composed: {res.get('composed_sentence')}, candidates: {res.get('candidates')}")

    # Commit with Enter '\n'
    print("5. Committing with Enter '\\n' ...")
    ptr = lib.ruhi_engine_process_key(engine, b'\n')
    if ptr:
        res_str = ctypes.cast(ptr, ctypes.c_char_p).value.decode('utf-8')
        res = json.loads(res_str)
        print(f"   Commit result -> action: {res.get('action')}, chunk: {res.get('committed_chunk')}")

    lib.ruhi_engine_destroy(engine)
    print("✨ Rust RuhiEngine C-ABI test successfully completed!")

if __name__ == "__main__":
    test_rust_core()
