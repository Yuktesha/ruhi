//
// RuhiBridge.swift — 如喜・龍龕 (RuhI Longkan v0.7.0+) iOS / macOS 原生 Swift + StoreKit 2 + App Group 橋接封裝
// 直接搭配 `ruhi-core/include/ruhi.h` (C-ABI) 使用，零第三方 SDK 依賴（符合 App Store "Data Not Collected" 純淨隱私標章）
//

import Foundation
import StoreKit

/// 對應 `ruhi_engine_get_entitlement_json` 回傳之結構化授權與九九圓滿進度狀態
public struct RuhiEntitlementState: Codable {
    public let is_licensed: Bool
    public let is_pro_unlocked: Bool
    public let is_trial_expired_both: Bool
    public let days_elapsed: UInt64
    public let target_days: UInt64
    public let tier2_target_days: UInt64
    public let total_committed_chars: UInt64
    public let target_chars: UInt64
    public let tier2_target_chars: UInt64
    public let store_sku: String
    public let price_label: String
    public let tier_name: String
    public let passcode: String
    public let machine_fingerprint: String
    public let summary_badge: String
}

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
    /// Apple App Group 識別碼（Host App 與 Keyboard Extension 共用同一份 `entitlement_state.json`）
    public static let appGroupId = "group.org.ruhos.ruhi"

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
        self.bindAppGroupEntitlementPath()
    }

    deinit {
        if let ptr = enginePtr {
            ruhi_engine_destroy(ptr)
            enginePtr = nil
        }
    }

    /// 將授權與九九圓滿字數計數檔指向 App Group 共用容器，確保主程式購買後輸入法 Extension 0 秒同步生效
    public func bindAppGroupEntitlementPath() {
        guard let ptr = enginePtr else { return }
        if let containerURL = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: Self.appGroupId) {
            let statePath = containerURL.appendingPathComponent("entitlement_state.json").path
            queue.sync {
                _ = statePath.withCString { cStr in
                    ruhi_engine_set_entitlement_path(ptr, cStr)
                }
            }
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

    /// 處理按鍵並自動累計九九圓滿字數
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

    /// 讀取當前九九圓滿（81日 & 81,000字）進度與三階段動態 SKU (`ruhi_lifetime_tier1_099` / `199` / `299`)
    public func getEntitlementState() -> RuhiEntitlementState? {
        guard let ptr = enginePtr else { return nil }
        return queue.sync {
            guard let rawJsonPtr = ruhi_engine_get_entitlement_json(ptr) else { return nil }
            defer { ruhi_free_string(rawJsonPtr) }
            let jsonString = String(cString: rawJsonPtr)
            guard let data = jsonString.data(using: .utf8) else { return nil }
            return try? JSONDecoder().decode(RuhiEntitlementState.self, from: data)
        }
    }

    /// 驗證並啟用使用者貼上之跨平台通行碼 (`RUHI-LK70-XXXX-XXXX-XXXX`)
    public func activatePasscode(_ code: String) -> Bool {
        guard let ptr = enginePtr else { return false }
        return queue.sync {
            let res = code.withCString { cStr in
                ruhi_engine_activate_passcode(ptr, cStr)
            }
            return res == 1
        }
    }

    /// 由 StoreKit 2 `originalID` 決定性簽發並啟用跨平台通行碼
    public func issuePasscodeFromStoreTransaction(originalID: UInt64) -> String? {
        guard let ptr = enginePtr else { return nil }
        let txString = "APPLE-SK2-\(originalID)"
        return queue.sync {
            guard let codePtr = txString.withCString({ ruhi_engine_issue_passcode_from_store_tx(ptr, $0) }) else {
                return nil
            }
            defer { ruhi_free_string(codePtr) }
            return String(cString: codePtr)
        }
    }
}

/// Apple 原生 StoreKit 2 購買與「回復購買」管理器（依 Rust 動態判別之三階段 SKU 自動拉取對應價格）
@available(iOS 15.0, macOS 12.0, *)
@MainActor
public final class RuhiStoreKitManager: ObservableObject {
    @Published public private(set) var activeProduct: Product?
    @Published public private(set) var issuedPasscode: String = ""
    @Published public private(set) var statusMessage: String = ""

    private let bridge: RuhiBridge

    public init(bridge: RuhiBridge) {
        self.bridge = bridge
    }

    /// 自動向 Rust 引擎詢問目前處於哪個階段 (`$0.99` / `$1.99` / `$2.99`)，並僅向 App Store 拉取該階段對應之單一商品
    public func loadCurrentTierProduct() async {
        guard let state = bridge.getEntitlementState() else { return }
        if state.is_licensed {
            self.issuedPasscode = state.passcode
            return
        }
        do {
            let products = try await Product.products(for: [state.store_sku])
            self.activeProduct = products.first
        } catch {
            self.statusMessage = "無法載入商店商品資訊：\(error.localizedDescription)"
        }
    }

    /// 執行購買並於本機直接驗證 JWS 數位簽章，0 秒簽發 `RUHI-LK70-XXXX-XXXX-XXXX` 跨平台通行碼
    public func purchaseCurrentTier() async -> String? {
        guard let product = activeProduct else { return nil }
        do {
            let result = try await product.purchase()
            switch result {
            case .success(let verification):
                if case .verified(let transaction) = verification {
                    let passcode = bridge.issuePasscodeFromStoreTransaction(originalID: transaction.originalID)
                    await transaction.finish()
                    if let code = passcode {
                        self.issuedPasscode = code
                        self.statusMessage = "✅ 購買成功！您的跨平台終身通行碼：\(code)"
                    }
                    return passcode
                }
            case .userCancelled, .pending:
                break
            @unknown default:
                break
            }
        } catch {
            self.statusMessage = "購買未完成：\(error.localizedDescription)"
        }
        return nil
    }

    /// 換手機或忘記通行碼時：點擊「回復購買 (Restore Purchases)」，由相同 `originalID` 決定性重算回同一組通行碼
    public func restorePurchaseAndRecoverPasscode() async -> String? {
        let allSkus: Set<String> = [
            "ruhi_lifetime_tier1_099",
            "ruhi_lifetime_tier2_199",
            "ruhi_lifetime_tier3_299"
        ]
        for await verification in Transaction.currentEntitlements {
            if case .verified(let transaction) = verification,
               allSkus.contains(transaction.productID) {
                if let code = bridge.issuePasscodeFromStoreTransaction(originalID: transaction.originalID) {
                    self.issuedPasscode = code
                    self.statusMessage = "✅ 已成功回復購買！您的跨平台終身通行碼：\(code)"
                    return code
                }
            }
        }
        self.statusMessage = "尚未在此 Apple ID 找到歷史購買紀錄。"
        return nil
    }
}
