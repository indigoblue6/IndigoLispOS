// parser.rs - S-expression parser

extern crate alloc;
use alloc::boxed::Box;
use heapless::{String, Vec};
use super::expr::Expr;

const MAX_ERROR_LEN: usize = 64;

pub struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str) -> Self {
        Parser {
            input,
            pos: 0,
        }
    }

    pub fn parse(&mut self) -> Result<Expr, String<MAX_ERROR_LEN>> {
        self.skip_whitespace();
        
        if self.pos >= self.input.len() {
            let mut err = String::new();
            let _ = err.push_str("Unexpected end of input");
            return Err(err);
        }

        let ch = self.current_char()?;
        
        match ch {
            '(' => {
                self.pos += 1;
                self.parse_list()
            }
            ')' => {
                let mut err = String::new();
                let _ = err.push_str("Unexpected ')'");
                Err(err)
            }  
            _ => self.parse_atom(),
        }
    }

    fn current_char(&self) -> Result<char, String<MAX_ERROR_LEN>> {
        self.input.chars().nth(self.pos)
            .ok_or_else(|| {
                let mut err = String::new();
                let _ = err.push_str("Unexpected end of input");
                err
            })
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            if let Some(ch) = self.input.chars().nth(self.pos) {
                if ch.is_whitespace() {
                    self.pos += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }

    fn parse_list(&mut self) -> Result<Expr, String<MAX_ERROR_LEN>> {
        let mut items: Vec<Expr, 8> = Vec::new();

        loop {
            self.skip_whitespace();
            
            if self.pos >= self.input.len() {
                let mut err = String::new();
                let _ = err.push_str("Unclosed list");
                return Err(err);
            }

            if self.current_char()? == ')' {
                self.pos += 1;
                return Ok(Expr::List(Box::new(items)));
            }

            items.push(self.parse()?);
        }
    }

    fn parse_atom(&mut self) -> Result<Expr, String<MAX_ERROR_LEN>> {
        let start = self.pos;
        
        // Read until whitespace or special char
        while self.pos < self.input.len() {
            if let Some(ch) = self.input.chars().nth(self.pos) {
                if ch.is_whitespace() || ch == '(' || ch == ')' {
                    break;
                }
                self.pos += 1;
            } else {
                break;
            }
        }

        let token = &self.input[start..self.pos];
        
        // Try to parse as number
        if let Ok(n) = token.parse::<i64>() {
            return Ok(Expr::Number(n));
        }

        // Check for boolean
        match token {
            "true" => return Ok(Expr::Bool(true)),
            "false" => return Ok(Expr::Bool(false)),
            "nil" => return Ok(Expr::Nil),
            _ => {}
        }

        // Check for string literal
        if token.starts_with('"') && token.ends_with('"') && token.len() > 1 {
            let mut s = String::new();
            let _ = s.push_str(&token[1..token.len()-1]);
            return Ok(Expr::String(s));
        }

        // Otherwise, it's a symbol
        let mut sym = String::new();
        let _ = sym.push_str(token);
        Ok(Expr::Symbol(sym))
    }
}
