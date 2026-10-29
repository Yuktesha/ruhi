// SPDX-License-Identifier: Apache-2.0
// RuhiBridge.swift — 如喜・龍龕 (RuhI Longkan) iOS / macOS 原生 Swift C-ABI 橋接封裝
// 直接搭配 `ruhi-core/include/ruhi.h` (C-ABI) 使用，零第三方 SDK 依賴

import Foundation

/// 對應 `ruhi_engine_process_key_str` 回傳之組字與候選字結構
public struct RuhiEngineResponse: Codable {
    public let action: String
    public let mode: String
    public let preedit: String
    public let composed: String
    public let candidates: [String]
    public let active_candidate: String
    public let candidate_idx: Int
    public let cursor_pos: Int
    public let committed_chunk: String
}

/// Rust `RuhiEngine` C-ABI 的執行緒安全 Swift 封裝（供 Host App 與 IME Keyboard Extension 共用）
public final class RuhiBridge {
    private var enginePtr: OpaquePointer?
    private let queue = DispatchQueue(label: "org.ruhos.ruhi.engine.serial")

    public init(dataDirPath: String? = nil) {
        if let dir = dataDirPath {
            self.enginePtr = dir.withCString { cStr in
                ruhi_engine_create_with_dir(cStr)
            }
        } else {
            self.enginePtr = ruhi_engine_create()
        }
    }

    deinit {
        if let ptr = enginePtr {
            ruhi_engine_destroy(ptr)
            enginePtr = nil
        }
    }

    /// 切換輸入法方案（如 `"longkan"`, `"four_corner"`, `"hsu"`, `"array30"`, `"cangjie"`）
    public func setKeyboardMode(_ mode: String) {
        guard let ptr = enginePtr else { return }
        queue.sync {
            _ = mode.withCString { cStr in
                ruhi_engine_set_keyboard_mode(ptr, cStr)
            }
        }
    }

    /// 處理按鍵並回傳組字與候選字結構
    public func processKey(_ key: String) -> RuhiEngineResponse? {
        guard let ptr = enginePtr else { return nil }
        return queue.sync {
            guard let rawJsonPtr = key.withCString({ ruhi_engine_process_key_str(ptr, $0) }) else {
                return nil
            }
            defer { ruhi_free_string(rawJsonPtr) }
            let jsonString = String(cString: rawJsonPtr)
            guard let data = jsonString.data(using: .utf8) else { return nil }
            return try? JSONDecoder().decode(RuhiEngineResponse.self, from: data)
        }
    }
}
