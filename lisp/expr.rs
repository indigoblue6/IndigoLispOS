// expr.rs - S-expression types

extern crate alloc;
use alloc::boxed::Box;
use heapless::{String, Vec};

const MAX_SYMBOL_LEN: usize = 64;
const MAX_STRING_LEN: usize = 256;
const MAX_LIST_ITEMS: usize = 8;  // Reduced from 32 to save heap space
const MAX_PARAMS: usize = 4;      // Max lambda parameters

#[derive(Clone)]
pub enum Expr {
    Number(i64),
    Symbol(String<MAX_SYMBOL_LEN>),
    String(String<MAX_STRING_LEN>),
    Bool(bool),
    List(Box<Vec<Expr, MAX_LIST_ITEMS>>),
    Nil,
    // Lambda: (parameters, body, captured_env_snapshot)
    Lambda(Box<(Vec<String<MAX_SYMBOL_LEN>, MAX_PARAMS>, Vec<Expr, MAX_LIST_ITEMS>, Option<EnvSnapshot>)>),
    // Macro: (parameters, body) - no environment capture, expands at compile-time
    Macro(Box<(Vec<String<MAX_SYMBOL_LEN>, MAX_PARAMS>, Vec<Expr, MAX_LIST_ITEMS>)>),
}

// Simplified environment snapshot for closures
#[derive(Clone)]
pub struct EnvSnapshot {
    pub bindings: Vec<(String<MAX_SYMBOL_LEN>, Expr), 16>,
}

impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Expr::Number(a), Expr::Number(b)) => a == b,
            (Expr::Symbol(a), Expr::Symbol(b)) => a == b,
            (Expr::String(a), Expr::String(b)) => a == b,
            (Expr::Bool(a), Expr::Bool(b)) => a == b,
            (Expr::List(a), Expr::List(b)) => a == b,
            (Expr::Nil, Expr::Nil) => true,
            (Expr::Lambda(..), Expr::Lambda(..)) => false, // Lambdas not comparable
            (Expr::Macro(..), Expr::Macro(..)) => false,   // Macros not comparable
            _ => false,
        }
    }
}

impl Expr {
    pub fn is_truthy(&self) -> bool {
        !matches!(self, Expr::Bool(false) | Expr::Nil)
    }

    pub fn display(&self) -> String<256> {
        let mut result = String::new();
        match self {
            Expr::Number(n) => {
                let _ = write!(result, "{}", n);
            }
            Expr::Symbol(s) => {
                let _ = result.push_str(s);
            }
            Expr::String(s) => {
                let _ = result.push('"');
                let _ = result.push_str(s);
                let _ = result.push('"');
            }
            Expr::Bool(b) => {
                let _ = result.push_str(if *b { "true" } else { "false" });
            }
            Expr::List(_items) => {
                let _ = result.push_str("(...)");
            }
            Expr::Nil => {
                let _ = result.push_str("nil");
            }
            Expr::Lambda(..) => {
                let _ = result.push_str("<lambda>");
            }
            Expr::Macro(..) => {
                let _ = result.push_str("<macro>");
            }
        }
        result
    }
}

use core::fmt::Write;
