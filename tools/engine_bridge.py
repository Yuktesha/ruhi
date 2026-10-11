#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜沙盒輸入座 (RuhI Sandbox Input Seat)
engine_bridge.py — Rust 引擎 ctypes 橋接層
=============================================================================
透過 ctypes 呼叫 ruhi-core.dll 的 C-ABI 介面，提供 Python 端完整的輸入法引擎功能。
"""

import ctypes
import json
import os
import sys

try:
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
except Exception:
    pass
from dataclasses import dataclass, field
from typing import List, Optional


@dataclass
class EngineResponse:
    """引擎回應資料結構 (Engine response data structure)"""
    action: str = ""
    mode: str = ""
    preedit: str = ""          # 注音組字中的顯示文字 (bopomofo preedit)
    composed: str = ""         # Viterbi 選字後的組成句 (composed sentence)
    candidates: List[str] = field(default_factory=list)  # 候選字詞清單
    active_candidate: str = ""  # 目前選中的候選字
    candidate_idx: int = 0     # 候選字索引
    cursor_pos: int = 0        # 游標位置
    q_press_count: int = 0     # Q鍵連按次數
    committed_chunk: str = ""  # 已確定送出的文字片段


class RuhiEngineBridge:
    """
    Rust 引擎橋接器 (Rust engine bridge via ctypes)
    
    用法：
        bridge = RuhiEngineBridge()
        resp = bridge.process_key('d')  # 送入許氏鍵碼
        print(resp.preedit, resp.candidates)
    """

    def __init__(self, dll_path: Optional[str] = None, data_dir: Optional[str] = None, user_dict_path: Optional[str] = None, enabled_packs: Optional[List[str]] = None):
        """
        初始化引擎橋接 (Initialize engine bridge)
        
        Args:
            dll_path: ruhi-core DLL 路徑，預設自動偵測
            data_dir: 詞庫資料目錄，預設自動偵測
            user_dict_path: 使用者自建詞庫檔案路徑，預設自動偵測
            enabled_packs: 啟用的專業/語境選配外掛詞庫包清單 (例如 ['pack_philosophy_ananda'])
        """
        # 自動偵測 DLL 路徑 (Auto-detect DLL path)
        if dll_path is None:
            base = os.path.dirname(os.path.abspath(__file__))
            ruhi_root = os.path.abspath(os.path.join(base, ".."))
            candidates = [
                os.path.join(ruhi_root, "ruhi-core", "target", "release", "libruhi.so"),
                os.path.join(ruhi_root, "ruhi-core", "target", "debug", "libruhi.so"),
                os.path.join(ruhi_root, "ruhi-core", "target", "release", "ruhi.dll"),
                os.path.join(ruhi_root, "ruhi-core", "target", "debug", "ruhi.dll"),
                os.path.join(ruhi_root, "ruhi-core", "target", "release", "libruhi.dylib"),
                os.path.join(ruhi_root, "ruhi-core", "target", "debug", "libruhi.dylib"),
                os.path.join(ruhi_root, "ruhi-core", "target", "release", "ruhi_core.dll"),
                os.path.join(ruhi_root, "ruhi-core", "target", "debug", "ruhi_core.dll"),
            ]
            for p in candidates:
                if os.path.exists(p):
                    dll_path = p
                    break
            if dll_path is None:
                raise FileNotFoundError(
                    f"找不到 ruhi-core DLL，已搜尋：\n" +
                    "\n".join(f"  - {p}" for p in candidates)
                )

        # 自動偵測資料目錄 (Auto-detect data directory)
        if data_dir is None:
            base = os.path.dirname(os.path.abspath(__file__))
            ruhi_root = os.path.abspath(os.path.join(base, ".."))
            data_dir = os.path.join(ruhi_root, "ruhi_lexicon", "data")

        # 自動偵測使用者自建詞庫路徑 (Auto-detect user dict path)
        if user_dict_path is None:
            base = os.path.dirname(os.path.abspath(__file__))
            ruhi_root = os.path.abspath(os.path.join(base, ".."))
            candidates = [
                os.path.join(ruhi_root, "user_dict.tsv"),
                os.path.join(base, "user_dict.tsv"),
            ]
            user_dict_path = next((p for p in candidates if os.path.exists(p)), candidates[0])

        self._dll_path = dll_path
        self._data_dir = data_dir
        self._user_dict_path = user_dict_path
        self._enabled_packs = enabled_packs or []
        self._engine = None
        self._dll = None

        self._load_dll()

    def _load_dll(self):
        """載入 DLL 並設定函式簽名 (Load DLL and configure function signatures)"""
        self._dll = ctypes.CDLL(self._dll_path)

        # ruhi_engine_create_with_dir(path: *const c_char) -> *mut RuhiEngine
        self._dll.ruhi_engine_create_with_dir.argtypes = [ctypes.c_char_p]
        self._dll.ruhi_engine_create_with_dir.restype = ctypes.c_void_p

        # ruhi_engine_create() -> *mut RuhiEngine
        self._dll.ruhi_engine_create.argtypes = []
        self._dll.ruhi_engine_create.restype = ctypes.c_void_p

        # ruhi_engine_destroy(engine: *mut RuhiEngine)
        self._dll.ruhi_engine_destroy.argtypes = [ctypes.c_void_p]
        self._dll.ruhi_engine_destroy.restype = None

        # ruhi_engine_load_tsv(engine, path) -> i32
        self._dll.ruhi_engine_load_tsv.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
        self._dll.ruhi_engine_load_tsv.restype = ctypes.c_int32

        # ruhi_engine_load_symbols_tsv(engine, path) -> i32
        self._dll.ruhi_engine_load_symbols_tsv.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
        self._dll.ruhi_engine_load_symbols_tsv.restype = ctypes.c_int32

        # ruhi_engine_load_transitions_tsv(engine, path) -> i32
        self._dll.ruhi_engine_load_transitions_tsv.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
        self._dll.ruhi_engine_load_transitions_tsv.restype = ctypes.c_int32

        # ruhi_engine_process_key(engine, key_code: c_char) -> *mut c_char
        self._dll.ruhi_engine_process_key.argtypes = [ctypes.c_void_p, ctypes.c_char]
        self._dll.ruhi_engine_process_key.restype = ctypes.c_void_p

        # ruhi_engine_process_key_str(engine, key_cstr: *const c_char) -> *mut c_char
        self._dll.ruhi_engine_process_key_str.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
        self._dll.ruhi_engine_process_key_str.restype = ctypes.c_void_p

        # ruhi_engine_reset(engine)
        self._dll.ruhi_engine_reset.argtypes = [ctypes.c_void_p]
        self._dll.ruhi_engine_reset.restype = None

        # ruhi_free_string(ptr: *mut c_char)
        self._dll.ruhi_free_string.argtypes = [ctypes.c_void_p]
        self._dll.ruhi_free_string.restype = None

        # 建立引擎實體 (Create engine instance)
        self._create_engine()

    def _create_engine(self):
        """建立引擎並載入詞庫 (Create engine and load lexicon)"""
        if self._data_dir and os.path.isdir(self._data_dir):
            path_bytes = self._data_dir.encode("utf-8")
            self._engine = self._dll.ruhi_engine_create_with_dir(path_bytes)
        else:
            self._engine = self._dll.ruhi_engine_create()

        if not self._engine:
            raise RuntimeError("ruhi_engine_create 回傳 null，引擎建立失敗")

        # 載入詞庫 (Load lexicons)
        # 1. McBopomofo 詞庫（主力，17 萬詞帶詞頻權重，MIT 授權）
        mcbpmf_path = os.path.join(self._data_dir, "lexicon_mcbopomofo.tsv")
        if os.path.exists(mcbpmf_path):
            count = self._dll.ruhi_engine_load_tsv(
                self._engine, mcbpmf_path.encode("utf-8")
            )
            print(f"[engine_bridge] 載入 McBopomofo 詞庫 {count} 筆")

        # 2. MOE 詞庫（補充）
        lexicon_path = os.path.join(self._data_dir, "lexicon_moe.tsv")
        if os.path.exists(lexicon_path):
            count = self._dll.ruhi_engine_load_tsv(
                self._engine, lexicon_path.encode("utf-8")
            )
            print(f"[engine_bridge] 載入 MOE 詞庫 {count} 筆")

        # 2.5 領域專業詞庫 (Tier 2 Domain Lexicon，補充)
        domain_lex_path = os.path.join(self._data_dir, "lexicon_domain.tsv")
        if os.path.exists(domain_lex_path):
            count = self._dll.ruhi_engine_load_tsv(
                self._engine, domain_lex_path.encode("utf-8")
            )
            print(f"[engine_bridge] 載入領域專業詞庫 {count} 筆 from {domain_lex_path}")

        # 2.8 專業/語境選配外掛詞庫 (Pluggable Packs: 隨包發行、手動/自動裝卸)
        packs_dir = os.path.join(self._data_dir, "packs")
        if self._enabled_packs and os.path.exists(packs_dir):
            for pack_name in self._enabled_packs:
                target_file = pack_name if pack_name.endswith(".tsv") else f"{pack_name}.tsv"
                pack_path = os.path.join(packs_dir, target_file)
                if os.path.exists(pack_path):
                    count = self._dll.ruhi_engine_load_tsv(
                        self._engine, pack_path.encode("utf-8")
                    )
                    print(f"[engine_bridge] 載入專業外掛詞庫 {count} 筆 from {target_file}")

        # 3. 使用者自建詞庫 (User Custom Lexicon，最高優先)
        if self._user_dict_path and os.path.exists(self._user_dict_path):
            count = self._dll.ruhi_engine_load_tsv(
                self._engine, self._user_dict_path.encode("utf-8")
            )
            print(f"[engine_bridge] 載入使用者自建詞庫 {count} 筆 from {self._user_dict_path}")

        # 載入符號表 (Load symbols)
        symbols_path = os.path.join(self._data_dir, "symbols.tsv")
        if os.path.exists(symbols_path):
            count = self._dll.ruhi_engine_load_symbols_tsv(
                self._engine, symbols_path.encode("utf-8")
            )
            print(f"[engine_bridge] 載入符號 {count} 筆 from {symbols_path}")

        # 載入使用者轉移矩陣 (Load user transitions)
        transitions_path = os.path.join(self._data_dir, "transitions_user.tsv")
        if os.path.exists(transitions_path):
            self._dedup_transitions(transitions_path)
            count = self._dll.ruhi_engine_load_transitions_tsv(
                self._engine, transitions_path.encode("utf-8")
            )
            print(f"[engine_bridge] 載入轉移矩陣 {count} 筆 from {transitions_path}")

    def list_available_packs(self) -> List[str]:
        """列出所有可用的專業/語境選配外掛詞庫包 (List all available pluggable packs)"""
        packs_dir = os.path.join(self._data_dir, "packs")
        if not os.path.exists(packs_dir):
            return []
        return [f[:-4] for f in os.listdir(packs_dir) if f.endswith(".tsv") and not f.startswith(".")]

    def load_pack(self, pack_name: str) -> int:
        """手動掛載指定的專業外掛詞庫包 (Mount specified pluggable pack)"""
        packs_dir = os.path.join(self._data_dir, "packs")
        target_file = pack_name if pack_name.endswith(".tsv") else f"{pack_name}.tsv"
        pack_path = os.path.join(packs_dir, target_file)
        if not os.path.exists(pack_path):
            print(f"[engine_bridge] 警告：找不到外掛詞庫包 {target_file}")
            return 0
        count = self._dll.ruhi_engine_load_tsv(self._engine, pack_path.encode("utf-8"))
        if pack_name not in self._enabled_packs:
            self._enabled_packs.append(pack_name)
        print(f"[engine_bridge] 成功掛載外掛詞庫包 {count} 筆 from {target_file}")
        return count

    def reload_user_dict(self, path: Optional[str] = None) -> int:
        """
        熱重載使用者自建詞庫 (Hot reload user custom lexicon)
        
        Returns:
            載入筆數
        """
        target = path or self._user_dict_path
        if not target or not os.path.exists(target) or not self._engine:
            return 0

        try:
            count = self._dll.ruhi_engine_load_tsv(
                self._engine, target.encode("utf-8")
            )
            print(f"[engine_bridge] 熱重載使用者詞庫完成，共 {count} 筆")
            return count
        except Exception as e:
            print(f"[engine_bridge] 熱重載使用者詞庫失敗: {e}")
            return 0

    def reload_transitions(self, path: Optional[str] = None) -> int:
        """
        熱重載轉移矩陣 (Hot reload transitions matrix)
        """
        target = path or os.path.join(self._data_dir, "transitions_user.tsv")
        if not target or not os.path.exists(target) or not self._engine:
            return 0

        try:
            self._dedup_transitions(target)
            count = self._dll.ruhi_engine_load_transitions_tsv(
                self._engine, target.encode("utf-8")
            )
            print(f"[engine_bridge] 熱重載轉移矩陣完成，共 {count} 筆")
            return count
        except Exception as e:
            print(f"[engine_bridge] 熱重載轉移矩陣失敗: {e}")
            return 0


    @staticmethod
    def _dedup_transitions(path: str):
        """
        去重轉移矩陣 TSV：同鍵 (前驅詞, 當前詞) 保留最高權重的那筆，回寫檔案。
        (Deduplicate transitions TSV: for duplicate keys, keep highest weight entry)
        """
        header_lines = []
        entries = {}  # key: (prev, curr) → value: full line string
        weights = {}  # key: (prev, curr) → value: weight (float)
        total = 0
        duplicates = 0

        with open(path, "r", encoding="utf-8") as f:
            for line in f:
                stripped = line.rstrip("\r\n")
                if not stripped or stripped.startswith("#"):
                    header_lines.append(stripped)
                    continue
                total += 1
                parts = stripped.split("\t")
                if len(parts) >= 3:
                    key = (parts[0].strip(), parts[1].strip())
                    try:
                        w = float(parts[2].strip())
                    except ValueError:
                        w = 0.0

                    if key in entries:
                        duplicates += 1
                        # 保留較高權重
                        if w > weights[key]:
                            entries[key] = stripped
                            weights[key] = w
                    else:
                        entries[key] = stripped
                        weights[key] = w
                else:
                    # 格式不對的行，用行本身當 key 保留
                    entries[(stripped, "")] = stripped
                    weights[(stripped, "")] = 0.0

        if duplicates == 0:
            return  # 沒有重複，不回寫

        # 回寫去重後的檔案
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            for h in header_lines:
                f.write(h + "\n")
            for line in entries.values():
                f.write(line + "\n")

        after = len(entries)
        print(f"[engine_bridge] 轉移矩陣去重：{total} → {after}（移除 {duplicates} 筆重複）")

    def process_key(self, ch: str) -> EngineResponse:
        """
        送入單一按鍵至引擎 (Send a single key to the engine)
        
        Args:
            ch: 按鍵字元（如 'd', 'j', ' '）
        Returns:
            EngineResponse 引擎回應
        """
        if not self._engine:
            return EngineResponse()

        try:
            key_byte = ch.encode("ascii")[0:1]
        except (UnicodeEncodeError, IndexError):
            # 防禦性保護：非 ASCII 字元安全略過，絕不拋出 UnicodeEncodeError 導致 Tkinter 崩潰
            return EngineResponse()

        raw_ptr = self._dll.ruhi_engine_process_key(self._engine, key_byte)

        if not raw_ptr:
            return EngineResponse()

        try:
            json_str = ctypes.cast(raw_ptr, ctypes.c_char_p).value.decode("utf-8")
            data = json.loads(json_str)
            return EngineResponse(**{k: v for k, v in data.items() if k in EngineResponse.__dataclass_fields__})
        except (json.JSONDecodeError, UnicodeDecodeError) as e:
            print(f"[engine_bridge] JSON 解析失敗: {e}")
            return EngineResponse()
        finally:
            self._dll.ruhi_free_string(raw_ptr)

    def process_key_str(self, key_str: str) -> EngineResponse:
        """
        送入字串按鍵至引擎 (Send a string key to the engine, e.g. "Ctrl+.")
        
        Args:
            key_str: 按鍵字串（如 "Ctrl+.", "Ctrl+Shift+,"）
        Returns:
            EngineResponse 引擎回應
        """
        if not self._engine:
            return EngineResponse()

        raw_ptr = self._dll.ruhi_engine_process_key_str(
            self._engine, key_str.encode("utf-8")
        )

        if not raw_ptr:
            return EngineResponse()

        try:
            json_str = ctypes.cast(raw_ptr, ctypes.c_char_p).value.decode("utf-8")
            data = json.loads(json_str)
            return EngineResponse(**{k: v for k, v in data.items() if k in EngineResponse.__dataclass_fields__})
        except (json.JSONDecodeError, UnicodeDecodeError) as e:
            print(f"[engine_bridge] JSON 解析失敗: {e}")
            return EngineResponse()
        finally:
            self._dll.ruhi_free_string(raw_ptr)

    def reset(self):
        """重置引擎緩衝區 (Reset engine buffer)"""
        if self._engine:
            self._dll.ruhi_engine_reset(self._engine)

    def destroy(self):
        """銷毀引擎實體 (Destroy engine instance)"""
        if self._engine:
            self._dll.ruhi_engine_destroy(self._engine)
            self._engine = None

    def __del__(self):
        self.destroy()


# === 快速測試 (Quick test) ===
if __name__ == "__main__":
    print("=== RuhI Engine Bridge 測試 ===")
    try:
        bridge = RuhiEngineBridge()
        print(f"引擎建立成功，DLL: {bridge._dll_path}")
        print(f"資料目錄: {bridge._data_dir}")

        # 測試：打 "ㄨ ㄛˇ" (我) = 許氏鍵碼 "xk" + 聲調 "f"
        for key in ['x', 'k', 'f']:
            resp = bridge.process_key(key)
            print(f"  key='{key}' → action={resp.action}, preedit={resp.preedit}, "
                  f"composed={resp.composed}, candidates={resp.candidates[:5]}")

        bridge.destroy()
        print("測試完成！")
    except Exception as e:
        print(f"測試失敗: {e}")
