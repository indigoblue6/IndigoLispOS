// hotreload.rs - Hot reload functionality for Lisp code

extern crate alloc;
use alloc::vec::Vec;
use alloc::string::String as StdString;
use alloc::format;
use heapless::String;

const MAX_HISTORY_LEN: usize = 100;
const MAX_CODE_LEN: usize = 2048;

/// Code change entry for history tracking
#[derive(Clone)]
pub struct CodeChange {
    pub timestamp: u64,
    pub symbol_name: String<64>,
    pub old_code: Option<String<MAX_CODE_LEN>>,
    pub new_code: String<MAX_CODE_LEN>,
    pub change_type: ChangeType,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ChangeType {
    Define,
    Redefine,
    Undefine,
}

/// Hot reload manager
pub struct HotReloadManager {
    history: Vec<CodeChange>,
    max_history: usize,
}

impl HotReloadManager {
    pub fn new() -> Self {
        HotReloadManager {
            history: Vec::new(),
            max_history: MAX_HISTORY_LEN,
        }
    }

    /// Record a code change
    pub fn record_change(
        &mut self,
        timestamp: u64,
        symbol_name: String<64>,
        old_code: Option<String<MAX_CODE_LEN>>,
        new_code: String<MAX_CODE_LEN>,
        change_type: ChangeType,
    ) {
        let change = CodeChange {
            timestamp,
            symbol_name,
            old_code,
            new_code,
            change_type,
        };

        self.history.push(change);

        // Limit history size
        if self.history.len() > self.max_history {
            self.history.remove(0);
        }
    }

    /// Get history count
    pub fn history_count(&self) -> usize {
        self.history.len()
    }

    /// Get recent changes
    pub fn get_recent_changes(&self, count: usize) -> &[CodeChange] {
        let start = if self.history.len() > count {
            self.history.len() - count
        } else {
            0
        };
        &self.history[start..]
    }

    /// Find changes for a specific symbol
    pub fn find_symbol_changes(&self, symbol_name: &str) -> Vec<&CodeChange> {
        self.history
            .iter()
            .filter(|c| c.symbol_name.as_str() == symbol_name)
            .collect()
    }

    /// Rollback to previous version of a symbol
    pub fn get_previous_version(&self, symbol_name: &str) -> Option<String<MAX_CODE_LEN>> {
        // Find the most recent change for this symbol
        for change in self.history.iter().rev() {
            if change.symbol_name.as_str() == symbol_name {
                return change.old_code.clone();
            }
        }
        None
    }

    /// Clear history
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Export history as string (for debugging/logging)
    pub fn export_history(&self) -> StdString {
        let mut result = StdString::from("Hot Reload History:\n");
        for (i, change) in self.history.iter().enumerate() {
            result.push_str(&format!(
                "{}. [{}] {} '{}'\n",
                i + 1,
                change.timestamp,
                match change.change_type {
                    ChangeType::Define => "DEFINE",
                    ChangeType::Redefine => "REDEFINE",
                    ChangeType::Undefine => "UNDEFINE",
                },
                change.symbol_name
            ));
        }
        result
    }
}

/// Auto-reload configuration
pub struct AutoReloadConfig {
    pub enabled: bool,
    pub watch_symbols: Vec<String<64>>,
    pub checkpoint_interval: u64, // in milliseconds
}

impl AutoReloadConfig {
    pub fn new() -> Self {
        AutoReloadConfig {
            enabled: false,
            watch_symbols: Vec::new(),
            checkpoint_interval: 5000, // 5 seconds default
        }
    }

    pub fn enable(&mut self) {
        self.enabled = true;
    }

    pub fn disable(&mut self) {
        self.enabled = false;
    }

    pub fn add_watch_symbol(&mut self, symbol: String<64>) {
        if !self.watch_symbols.contains(&symbol) {
            self.watch_symbols.push(symbol);
        }
    }

    pub fn remove_watch_symbol(&mut self, symbol: &str) {
        self.watch_symbols.retain(|s| s.as_str() != symbol);
    }
}
