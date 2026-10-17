#!/usr/bin/env bash
# RuhI (如喜) 通用多語系智慧型輸入法 - Linux 核心引擎與詞庫建置腳本
# 支援 Ubuntu, Debian, Arch Linux, Fedora 等現代 Linux 發行版
set -e

echo -e "\033[1;36m========================================================\033[0m"
echo -e "\033[1;36m  正在為 Linux 建置 RuhI (如喜) 核心引擎與詞庫...\033[0m"
echo -e "\033[1;36m========================================================\033[0m"

# 1. 檢查 Rust / Cargo 編譯工具鏈
if ! command -v cargo &> /dev/null; then
    echo -e "\033[1;31m[錯誤] 未偵測到 Cargo！請先安裝 Rust 工具鏈 (https://rustup.rs)\033[0m"
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo -e "\033[0;32m[*] 專案根目錄: $ROOT_DIR\033[0m"

# 2. 編譯 ruhi-core (產生 libruhi.so)
echo -e "\033[0;32m[*] 正在編譯純 Rust 核心引擎 (ruhi-core)... \033[0m"
cd "$ROOT_DIR/ruhi-core"
cargo build --release

SO_PATH="$ROOT_DIR/ruhi-core/target/release/libruhi.so"
if [ ! -f "$SO_PATH" ]; then
    echo -e "\033[1;31m[錯誤] 編譯完成但未找到 $SO_PATH\033[0m"
    exit 1
fi
echo -e "\033[1;32m[OK] 已成功生成核心動態連結庫: $SO_PATH\033[0m"

# 3. 部署詞庫至標準使用者資料目錄 (XDG Spec)
TARGET_DATA_DIR="$HOME/.local/share/ruhi/data"
mkdir -p "$TARGET_DATA_DIR"
echo -e "\033[0;32m[*] 正在同步 24 萬+ 詞條教育部與麥氏標準詞庫至 $TARGET_DATA_DIR...\033[0m"

cp -r "$ROOT_DIR/ruhi_lexicon/data/"* "$TARGET_DATA_DIR/"
echo -e "\033[1;32m[OK] 詞庫部署完成！\033[0m"

# 4. 執行快速冒煙測試
echo -e "\033[0;32m[*] 正在執行單元測試驗證...\033[0m"
cargo test --release --lib -- --quiet

echo -e "\033[1;36m========================================================\033[0m"
echo -e "\033[1;32m  【建置成功！】RuhI 核心與詞庫已就緒！\033[0m"
echo -e "  - 核心共享庫: $SO_PATH"
echo -e "  - 詞庫目錄:   $TARGET_DATA_DIR"
echo -e "  - Fcitx5 整合規格請參閱 docs/RUHI_LINUX_FCITX5_ARCHITECTURE.md"
echo -e "\033[1;36m========================================================\033[0m"
