//! 泛型狀態快照復原與重做管理員 (`undo_manager.rs`)
//!
//! 對應 Python 版 `_lib/UniversalUI.py` 之 `UndoManager`。
//! 透過 `Clone` 建立狀態隔離快照，確保應用程式修改當前狀態時不會污染歷史堆疊。

/// 泛型復原/重做歷史管理員
#[derive(Debug, Clone)]
pub struct UndoManager<T: Clone> {
    limit: usize,
    history: Vec<T>,
    redo_stack: Vec<T>,
}

impl<T: Clone> Default for UndoManager<T> {
    fn default() -> Self {
        Self::new(32)
    }
}

impl<T: Clone> UndoManager<T> {
    /// 建立具有指定歷史上限（預設建議 32 步）之復原管理員。
    pub fn new(limit: usize) -> Self {
        Self {
            limit: limit.max(2),
            history: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// 記錄當前狀態快照，並清空重做分支堆疊。
    pub fn snapshot(&mut self, state: &T) {
        self.history.push(state.clone());
        if self.history.len() > self.limit {
            self.history.remove(0);
        }
        self.redo_stack.clear();
    }

    /// 是否可執行復原 (`Undo`)（需至少包含當前狀態與前一個歷史狀態）。
    pub fn can_undo(&self) -> bool {
        self.history.len() >= 2
    }

    /// 是否可執行重做 (`Redo`)。
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// 執行復原 (`Ctrl+Z`)，回傳前一個狀態之複本。
    pub fn undo(&mut self) -> Option<T> {
        if !self.can_undo() {
            return None;
        }
        if let Some(current) = self.history.pop() {
            self.redo_stack.push(current);
        }
        self.history.last().cloned()
    }

    /// 執行重做 (`Ctrl+Y` / `Ctrl+Shift+Z`)，回傳下一個狀態之複本。
    pub fn redo(&mut self) -> Option<T> {
        let next = self.redo_stack.pop()?;
        self.history.push(next.clone());
        Some(next)
    }

    /// 檢視目前堆疊頂端狀態。
    pub fn current(&self) -> Option<&T> {
        self.history.last()
    }

    /// 清空所有復原與重做紀錄。
    pub fn clear(&mut self) {
        self.history.clear();
        self.redo_stack.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_undo_redo_lifecycle() {
        let mut mgr = UndoManager::new(4);
        mgr.snapshot(&"State 1".to_string());
        mgr.snapshot(&"State 2".to_string());
        mgr.snapshot(&"State 3".to_string());

        assert!(mgr.can_undo());
        assert!(!mgr.can_redo());

        assert_eq!(mgr.undo(), Some("State 2".to_string()));
        assert!(mgr.can_redo());
        assert_eq!(mgr.undo(), Some("State 1".to_string()));
        assert!(!mgr.can_undo());

        assert_eq!(mgr.redo(), Some("State 2".to_string()));
        // 新增分支後應清空 redo_stack
        mgr.snapshot(&"State 2B".to_string());
        assert!(!mgr.can_redo());
        assert_eq!(mgr.undo(), Some("State 2".to_string()));
    }
}
