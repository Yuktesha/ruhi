#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜 (RuhI) 通用智慧型輸入法 - 核心命令列工具
tools/ruhi_semantic_analyst.py — AI 語意分析師與使用者詞庫語意規則庫 CLI
=============================================================================
全面落實《嚴禁單字權重打地鼠守則》與「防劫掠（奪舍）劇毒」治理體系。

指令模式：
  1. analyze / add  : 深度診斷自建詞與上下文，提取中立權重、引力場與防劫掠互斥規則並寫入
  2. audit          : 單純審查自建詞是否有同音別字筆誤（如「粒刻」）或劫掠風險 (Dry-Run)
  3. list           : 列出目前規則庫中所有記錄
  4. rollback       : 回滾指定的語意規則（撤銷詞條、引力與互斥條目）
  5. health         : 體檢既有 user_dict.tsv，掃描是否存在 1500~2000 等虛高先驗權重與高危險詞
"""

import argparse
import json
import os
import sys
import time
from datetime import datetime

# 終端機 UTF-8 輸出保障
try:
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
except Exception:
    pass

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
RUHI_ROOT = os.path.dirname(BASE_DIR)
if RUHI_ROOT not in sys.path:
    sys.path.insert(0, RUHI_ROOT)

from ruhi_brain.semantic_analyst import RuhiSemanticAnalyst, SemanticAnalysisVerdict


# ANSI 彩色常數
class Colors:
    HEADER = "\033[95m"
    OKBLUE = "\033[94m"
    OKCYAN = "\033[96m"
    OKGREEN = "\033[92m"
    WARNING = "\033[93m"
    FAIL = "\033[91m"
    ENDC = "\033[0m"
    BOLD = "\033[1m"
    UNDERLINE = "\033[4m"


def print_banner():
    banner = f"""{Colors.OKCYAN}{Colors.BOLD}
  ╔═══════════════════════════════════════════════════════════════════════╗
  ║    ✦ 如喜 (RuhI) AI 語意分析師與使用者詞庫防劫掠規則庫 CLI ✦          ║
  ║    AI Semantic Analyst & Anti-Hijacking User Ruleset Console          ║
  ╚═══════════════════════════════════════════════════════════════════════╝
{Colors.ENDC}"""
    print(banner)


def cmd_analyze(args, analyst: RuhiSemanticAnalyst):
    print(f"\n{Colors.BOLD}🔍 正在進行深度計算語言學診斷...{Colors.ENDC}")
    print(f"  • 候選詞目: {Colors.OKCYAN}『{args.word}』{Colors.ENDC}")
    if args.context:
        print(f"  • 語境句子: {args.context}")
    if args.bopomofo:
        print(f"  • 指定注音: {args.bopomofo}")

    verdict = analyst.analyze_custom_word(
        word=args.word,
        context=args.context or "",
        user_bpm=args.bopomofo,
        force_heuristic=args.force_heuristic
    )

    print("\n" + "=" * 75)
    print(f"{Colors.BOLD}📊 診斷結果報告 (Verdict Report){Colors.ENDC}")
    print("=" * 75)
    print(f"  • 規則識別碼 (Rule ID)  : {verdict.rule_id}")
    print(f"  • 標準注音 (Bopomofo)   : {verdict.bopomofo}")
    print(f"  • 歸一化先驗 (Prior)    : {Colors.OKGREEN}{verdict.normalized_weight} 分{Colors.ENDC} (杜絕 2000 分黑洞)")
    print(f"  • 詞性類別 (Category)   : {verdict.category}")
    print(f"  • 建議動作 (Action)     : {verdict.action_recommended}")

    # 錯字審查結果
    print(f"\n{Colors.BOLD}🛡️ 錯字防火牆審查 (Typo Audit):{Colors.ENDC}")
    if verdict.typo_audit.is_typo:
        print(f"  {Colors.FAIL}❌ 檢測到高風險筆誤！{Colors.ENDC}")
        print(f"     建議正確詞彙: {Colors.OKGREEN}『{verdict.typo_audit.suggested_correct}』{Colors.ENDC}")
        print(f"     語言學原因: {verdict.typo_audit.reason}")
    else:
        print(f"  {Colors.OKGREEN}✅ 通過錯字檢測：非典型筆誤，認定為專有名詞或合法自建詞。{Colors.ENDC}")

    # 語意引力場
    print(f"\n{Colors.BOLD}🔗 萃取之二階語意引力場 (Attractors):{Colors.ENDC}")
    if verdict.attractors:
        for a in verdict.attractors:
            print(f"  • {Colors.OKCYAN}「{a.prev_word}」 ➔ 「{a.curr_word}」{Colors.ENDC} (+{a.bonus:.1f}) [{a.reason}]")
    else:
        print("  • (無明確上下文搭配，採中立先驗獨立入庫)")

    # 防劫掠互斥罩
    print(f"\n{Colors.BOLD}⚔️ 防劫掠互斥抑制罩 (Anti-Hijack Shields):{Colors.ENDC}")
    if verdict.shields:
        for s in verdict.shields:
            print(f"  • {Colors.WARNING}「{s.prev_word}」 ➔ 「{s.curr_word}」{Colors.ENDC} ({s.penalty:.1f}) [{s.reason}]")
    else:
        print("  • (未檢測到同音競態詞劫掠風險)")

    print(f"\n📝 語言學機理總結: {verdict.linguistic_summary}")
    print("=" * 75)

    if args.dry_run:
        print(f"\n{Colors.WARNING}⚠️ 處於 Dry-Run 模式，未實際寫入磁碟。{Colors.ENDC}")
        return

    # 若是錯字且未強制加入
    if verdict.action_recommended == "WARN_POTENTIAL_TYPO" and not args.force:
        print(f"\n{Colors.FAIL}🛑 系統已阻擋本次入庫！該詞疑似為筆誤。若執意入庫，請加上 --force 參數。{Colors.ENDC}")
        return

    res = analyst.apply_verdict(verdict, commit_to_disk=True)
    print(f"\n{Colors.OKGREEN}🎉 [OK] 語意規則與中立實體已安全落地寫入！{Colors.ENDC}")
    print(f"  • 實體詞庫: {'已新增' if res['word_added'] else '既有已存在'}")
    print(f"  • 語意引力: 追加 {res['attractors_added']} 筆")
    print(f"  • 防劫掠罩: 追加 {res['shields_added']} 筆")


def cmd_list(args, analyst: RuhiSemanticAnalyst):
    rules = analyst.list_rules()
    print(f"\n{Colors.BOLD}📋 現有使用者詞庫語意規則庫 (共 {len(rules)} 筆):{Colors.ENDC}\n")
    if not rules:
        print("  (尚無語意規則記錄)")
        return

    for idx, r in enumerate(rules, 1):
        print(f"{idx}. {Colors.OKCYAN}[{r['rule_id']}]{Colors.ENDC} 詞目: {Colors.BOLD}『{r['word']}』{Colors.ENDC} ({r['bopomofo']})")
        print(f"   時間: {r['created_at']} | 權重: {r['normalized_weight']} | 類別: {r['category']}")
        print(f"   語境: {r['context_sentence']}")
        print(f"   引力場: {len(r['attractors'])} 條 | 互斥防禦: {len(r['shields'])} 條")
        print("   " + "-" * 60)


def cmd_rollback(args, analyst: RuhiSemanticAnalyst):
    print(f"\n{Colors.BOLD}🔄 正在執行語意規則回滾: {args.rule_id}{Colors.ENDC}")
    success = analyst.rollback_rule(args.rule_id)
    if success:
        print(f"{Colors.OKGREEN}✅ [OK] 規則 {args.rule_id} 及其詞條、引力場與防劫掠互斥條目已徹底抹除與回滾！{Colors.ENDC}")
    else:
        print(f"{Colors.FAIL}❌ 回滾失敗：找不到規則識別碼 {args.rule_id}{Colors.ENDC}")


def cmd_health(args, analyst: RuhiSemanticAnalyst):
    print(f"\n{Colors.BOLD}🏥 正在檢驗使用者詞庫健康狀況 (Health Audit)...{Colors.ENDC}")
    path = analyst.user_dict_path
    if not os.path.exists(path):
        print(f"  ⚠️ 找不到使用者詞庫檔案: {path}")
        return

    high_weight_words = []
    typo_hits = []
    total_words = 0

    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) >= 2:
                total_words += 1
                w = parts[0].strip()
                wt = int(parts[2].strip()) if len(parts) > 2 and parts[2].strip().isdigit() else 100

                # 檢查先驗過高（語意黑洞/劫掠風險）
                if wt >= 1500:
                    high_weight_words.append((w, wt))

                # 檢查錯字防火牆
                typo_res = analyst.audit_typo(w)
                if typo_res.is_typo:
                    typo_hits.append((w, typo_res.suggested_correct, typo_res.reason))

    print("\n" + "=" * 75)
    print(f"{Colors.BOLD}📊 詞庫健康檢查報告: {path}{Colors.ENDC}")
    print("=" * 75)
    print(f"  • 總詞庫條目數: {total_words} 筆")

    print(f"\n{Colors.BOLD}⚠️ 【先驗虛高檢測 (Unigram > 1500)】 (可能破壞 Viterbi 並引發劫掠):{Colors.ENDC}")
    if high_weight_words:
        print(f"  檢測到 {len(high_weight_words)} 筆危險高分詞：")
        for w, wt in high_weight_words[:15]:
            print(f"  • 『{w}』 (權重 {wt}) ➔ 建議透過 AI 分析師歸一化至 750~880")
        if len(high_weight_words) > 15:
            print(f"  ...其餘 {len(high_weight_words) - 15} 筆略。")
    else:
        print(f"  {Colors.OKGREEN}🎉 太棒了！未發現任何大於 1500 的危險高分黑洞詞。{Colors.ENDC}")

    print(f"\n{Colors.BOLD}🛡️ 【錯字防火牆檢測】 (潛在入庫錯字筆誤):{Colors.ENDC}")
    if typo_hits:
        print(f"  檢測到 {len(typo_hits)} 筆疑似錯字筆誤：")
        for w, corr, reason in typo_hits:
            print(f"  • 『{w}』 ➔ 建議修正為 『{corr}』 ({reason})")
    else:
        print(f"  {Colors.OKGREEN}🎉 未發現常見筆誤詞條！{Colors.ENDC}")
    print("=" * 75 + "\n")


def cmd_process(args, analyst: RuhiSemanticAnalyst):
    print(f"\n{Colors.BOLD}📥 正在檢查並處理使用者自建詞佇列 ({analyst.events_path})...{Colors.ENDC}")
    results = analyst.process_events(
        events_path=args.events_file,
        force_heuristic=args.force_heuristic
    )
    if not results:
        print(f"  {Colors.OKGREEN}目前佇列中無待處理的新自建詞事件。{Colors.ENDC}")
        return

    print(f"\n{Colors.OKGREEN}🎉 成功提煉並收錄 {len(results)} 筆自建詞語意規則：{Colors.ENDC}")
    for item in results:
        warn = f" ({item['warning']})" if item.get("warning") else ""
        print(f"  • 『{item['word']}』 [規則: {item['rule_id']}] ➔ +{item['attractors_count']} 引力, -{item['shields_count']} 互斥{warn}")
    print("=" * 75 + "\n")


def cmd_watch(args, analyst: RuhiSemanticAnalyst):
    print(f"\n{Colors.BOLD}👀 啟動 AI 語意分析師背景監控守護 (PID: {os.getpid()})...{Colors.ENDC}")
    print(f"  • 監聽佇列: {analyst.events_path}")
    print(f"  • 按 Ctrl+C 可停止守護\n")

    try:
        while True:
            results = analyst.process_events(
                events_path=args.events_file,
                force_heuristic=args.force_heuristic
            )
            if results:
                for item in results:
                    warn = f" ({item['warning']})" if item.get("warning") else ""
                    print(f"[{datetime.now().strftime('%H:%M:%S')}] 提煉自建詞 『{item['word']}』 ➔ +{item['attractors_count']} 引力, -{item['shields_count']} 互斥{warn}")
            time.sleep(1.5)
    except KeyboardInterrupt:
        print(f"\n{Colors.WARNING}AI 語意分析師監控守護已停止。{Colors.ENDC}")


def main():
    print_banner()
    parser = argparse.ArgumentParser(
        description="如喜 (RuhI) AI 語意分析師與使用者詞庫防劫掠規則庫 CLI",
        formatter_class=argparse.RawDescriptionHelpFormatter
    )

    subparsers = parser.add_subparsers(dest="command", help="子指令")

    # 1. analyze / add
    p_add = subparsers.add_parser("add", help="分析並安全加入自建詞與語意引力場")
    p_add.add_argument("--word", "-w", required=True, help="自建詞目 (如：如喜)")
    p_add.add_argument("--context", "-c", default="", help="該詞出現的上下文句子")
    p_add.add_argument("--bopomofo", "-b", default=None, help="手動指定注音序列 (選填)")
    p_add.add_argument("--dry-run", action="store_true", help="僅進行分析報告，不寫入磁碟")
    p_add.add_argument("--force", "-f", action="store_true", help="即使判定為疑似筆誤，仍強制寫入")
    p_add.add_argument("--force-heuristic", action="store_true", help="強制使用本機離線啟發式引擎")

    # 2. audit
    p_audit = subparsers.add_parser("audit", help="僅對自建詞進行錯字防火牆與劫掠風險審查 (Dry-run)")
    p_audit.add_argument("--word", "-w", required=True, help="待審查詞目")
    p_audit.add_argument("--context", "-c", default="", help="語境句子")
    p_audit.add_argument("--bopomofo", "-b", default=None, help="注音序列")
    p_audit.add_argument("--force-heuristic", action="store_true", help="強制使用本機離線啟發式引擎")

    # 3. list
    subparsers.add_parser("list", help="列出已登記之所有使用者詞庫語意規則")

    # 4. rollback
    p_rb = subparsers.add_parser("rollback", help="回滾指定的語意規則識別碼")
    p_rb.add_argument("--rule-id", "-r", required=True, help="規則識別碼 (如：rule_20261006_...)")

    # 5. health
    subparsers.add_parser("health", help="全面體檢既有 user_dict.tsv 權重與錯字風險")

    # 6. process
    p_proc = subparsers.add_parser("process", help="批次處理待審查之自建詞事件佇列 (user_word_events.jsonl)")
    p_proc.add_argument("--events-file", "-e", default=None, help="指定事件日誌檔案路徑")
    p_proc.add_argument("--force-heuristic", action="store_true", help="強制使用本機離線啟發式引擎")

    # 7. watch
    p_watch = subparsers.add_parser("watch", help="常駐監控自建詞事件佇列並自動進行語意提煉")
    p_watch.add_argument("--events-file", "-e", default=None, help="指定事件日誌檔案路徑")
    p_watch.add_argument("--force-heuristic", action="store_true", help="強制使用本機離線啟發式引擎")

    args = parser.parse_args()
    if not args.command:
        parser.print_help()
        return

    analyst = RuhiSemanticAnalyst()

    if args.command in ("add", "analyze"):
        cmd_analyze(args, analyst)
    elif args.command == "audit":
        args.dry_run = True
        args.force = False
        cmd_analyze(args, analyst)
    elif args.command == "list":
        cmd_list(args, analyst)
    elif args.command == "rollback":
        cmd_rollback(args, analyst)
    elif args.command == "health":
        cmd_health(args, analyst)
    elif args.command == "process":
        cmd_process(args, analyst)
    elif args.command == "watch":
        cmd_watch(args, analyst)


if __name__ == "__main__":
    main()
