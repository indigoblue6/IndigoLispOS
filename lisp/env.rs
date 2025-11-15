// env.rs - Environment for variable bindings

use heapless::{String, FnvIndexMap, Vec};
use super::expr::{Expr, EnvSnapshot};

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

    // Create a snapshot of current environment for closures
    pub fn snapshot(&self) -> EnvSnapshot {
        let mut bindings = Vec::new();
        for (k, v) in self.bindings.iter() {
            let _ = bindings.push((k.clone(), v.clone()));
        }
        EnvSnapshot { bindings }
    }

    // Create new environment from snapshot
    pub fn from_snapshot(snapshot: &EnvSnapshot) -> Self {
        let mut env = Env::new();
        for (name, value) in &snapshot.bindings {
            env.define(name.clone(), value.clone());
        }
        env
    }

    // Extend environment with new bindings
    pub fn extend(&mut self, snapshot: &EnvSnapshot) {
        for (name, value) in &snapshot.bindings {
            self.define(name.clone(), value.clone());
        }
    }

    // Get all binding names for tab completion
    pub fn get_binding_names(&self) -> Vec<&str, MAX_BINDINGS> {
        let mut names = Vec::new();
        for (name, _) in self.bindings.iter() {
            let _ = names.push(name.as_str());
        }
        names
    }
}
