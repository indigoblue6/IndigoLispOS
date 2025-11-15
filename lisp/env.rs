// env.rs - Environment for variable bindings

use heapless::{String, FnvIndexMap};
use super::expr::Expr;

const MAX_BINDINGS: usize = 32;
const MAX_SYMBOL_LEN: usize = 64;

pub struct Env {
    bindings: FnvIndexMap<String<MAX_SYMBOL_LEN>, Expr, MAX_BINDINGS>,
}

impl Env {
    pub fn new() -> Self {
        Env {
            bindings: FnvIndexMap::new(),
        }
    }

    pub fn define(&mut self, name: String<MAX_SYMBOL_LEN>, value: Expr) {
        let _ = self.bindings.insert(name, value);
    }

    pub fn get(&self, name: &String<MAX_SYMBOL_LEN>) -> Option<Expr> {
        self.bindings.get(name).cloned()
    }

    pub fn set(&mut self, name: &String<MAX_SYMBOL_LEN>, value: Expr) -> Result<(), &'static str> {
        if self.bindings.contains_key(name) {
            let _ = self.bindings.insert(name.clone(), value);
            Ok(())
        } else {
            Err("Undefined variable")
        }
    }
}
