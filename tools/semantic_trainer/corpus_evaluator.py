import ctypes
import difflib
import json
import os
import sys
import time

from text_to_hsu import text_to_hsu_keystrokes

sys.stdout.reconfigure(encoding='utf-8')

class RuhiCorpusEvaluator:
    def __init__(self, dll_path, tsv_path):
        self.lib = ctypes.CDLL(os.path.abspath(dll_path))
        self.lib.ruhi_engine_create.restype = ctypes.c_void_p
        self.lib.ruhi_engine_destroy.argtypes = [ctypes.c_void_p]
        self.lib.ruhi_engine_load_tsv.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
        self.lib.ruhi_engine_load_tsv.restype = ctypes.c_int
        self.lib.ruhi_engine_process_key.argtypes = [ctypes.c_void_p, ctypes.c_char]
        self.lib.ruhi_engine_process_key.restype = ctypes.c_char_p
        self.lib.ruhi_free_string.argtypes = [ctypes.c_char_p]

        self.engine = self.lib.ruhi_engine_create()
        self.lib.ruhi_engine_load_tsv(self.engine, tsv_path.encode('utf-8'))

    def __del__(self):
        if hasattr(self, 'engine') and self.engine:
            self.lib.ruhi_engine_destroy(self.engine)

    def reset(self):
        self.lib.ruhi_engine_process_key(self.engine, b'\x1b')

    def run_keystrokes(self, keystrokes):
        self.reset()
        output_chunks = []
        for ch in keystrokes:
            ptr = self.lib.ruhi_engine_process_key(self.engine, ch.encode('ascii', errors='ignore'))
            if ptr:
                res_str = ctypes.cast(ptr, ctypes.c_char_p).value.decode('utf-8')
                res = json.loads(res_str)
                action = res.get("action", "")
                if action.startswith("commit"):
                    output_chunks.append(res.get("committed_chunk", ""))

        # Force commit remaining composition at the end
        ptr = self.lib.ruhi_engine_process_key(self.engine, b'\n')
        if ptr:
            res_str = ctypes.cast(ptr, ctypes.c_char_p).value.decode('utf-8')
            res = json.loads(res_str)
            action = res.get("action", "")
            if action.startswith("commit"):
                output_chunks.append(res.get("committed_chunk", ""))

        return ''.join(output_chunks)

    def evaluate_article(self, original_text):
        start_time = time.perf_counter()
        keystrokes = text_to_hsu_keystrokes(original_text)
        conversion_time = time.perf_counter() - start_time

        t0 = time.perf_counter()
        generated_text = self.run_keystrokes(keystrokes)
        typing_time = time.perf_counter() - t0

        # Diff Analysis
        matcher = difflib.SequenceMatcher(None, original_text, generated_text)
        ratio = matcher.ratio()

        mismatches = []
        for tag, i1, i2, j1, j2 in matcher.get_opcodes():
            if tag != 'equal':
                orig_snippet = original_text[max(0, i1-5):min(len(original_text), i2+5)]
                mismatches.append({
                    'type': tag,
                    'orig_index': (i1, i2),
                    'gen_index': (j1, j2),
                    'original': original_text[i1:i2],
                    'generated': generated_text[j1:j2],
                    'context': f"...{orig_snippet}...",
                })

        accuracy = ratio * 100.0
        cps = len(original_text) / (typing_time if typing_time > 0 else 0.001)

        report = {
            'original_length': len(original_text),
            'generated_length': len(generated_text),
            'accuracy_ratio': accuracy,
            'typing_time_seconds': typing_time,
            'chars_per_second': cps,
            'mismatch_count': len(mismatches),
            'mismatches': mismatches,
            'generated_text': generated_text,
        }
        return report

def main():
    dll_path = "C:/ATGprjs/ruhi/ruhi-core/target/release/ruhi.dll"
    tsv_path = "C:/ATGprjs/ruhi/ruhi_lexicon/data/lexicon_moe.tsv"

    evaluator = RuhiCorpusEvaluator(dll_path, tsv_path)

    test_article = (
        "我已經忘記剛剛要打什麼，但我底下會照著我現在用自然打的，以如喜再打一次。"
    )

    print("==================================================================")
    print("  RuhI (如喜) AI 語意分析師 & 詞庫自動調校流水線 - 概念驗證報告")
    print("==================================================================")
    print(f"原始文章測試文本：\n「{test_article}」\n")

    report = evaluator.evaluate_article(test_article)

    print(f"【效能指標】")
    print(f"- 原始字數：{report['original_length']} 字")
    print(f"- 輸入法模擬耗時：{report['typing_time_seconds'] * 1000:.2f} ms")
    print(f"- 記憶體吞吐率：{report['chars_per_second']:.0f} 字/秒 (相當於一秒可打數萬字)")
    print(f"- 首次直出辨識準確率 (Accuracy)：{report['accuracy_ratio']:.2f}%\n")

    print(f"【生成結果】")
    print(f"「{report['generated_text']}」\n")

    if report['mismatch_count'] == 0:
        print("✅ 100% 完美契合！完全無需人工選字。")
    else:
        print(f"【差異檢測 (共 {report['mismatch_count']} 處差異)】")
        for i, m in enumerate(report['mismatches'], 1):
            print(f"  {i}. [{m['type'].upper()}] 原文: 「{m['original']}」 vs 如喜直出: 「{m['generated']}」")
            print(f"     語境: {m['context']}")

    print("==================================================================")

if __name__ == '__main__':
    main()
