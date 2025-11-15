// repl.rs - Advanced REPL with tab completion, history, and multi-line support

use heapless::{String, Vec};

const MAX_INPUT_LEN: usize = 512;
const MAX_HISTORY: usize = 32;
const MAX_LINE_LEN: usize = 256;

// ANSI escape codes
const ESC: &str = "\x1b";
const CLEAR_LINE: &str = "\x1b[2K\r";
const CURSOR_SAVE: &str = "\x1b[s";
const CURSOR_RESTORE: &str = "\x1b[u";

// Special keys (ANSI escape sequences)
const KEY_UP: [u8; 3] = [0x1b, 0x5b, 0x41];      // ESC [ A
const KEY_DOWN: [u8; 3] = [0x1b, 0x5b, 0x42];    // ESC [ B
const KEY_RIGHT: [u8; 3] = [0x1b, 0x5b, 0x43];   // ESC [ C
const KEY_LEFT: [u8; 3] = [0x1b, 0x5b, 0x44];    // ESC [ D

// Keywords for tab completion (80+ keywords)
const KEYWORDS: &[&str] = &[
    // Special forms
    "define", "lambda", "if", "quote", "begin", "set!", "defmacro",
    // Arithmetic
    "+", "-", "*", "/", "mod", "abs", "max", "min",
    // Comparison
    "=", "<", ">", "<=", ">=", "eq?", "equal?",
    // Boolean
    "and", "or", "not",
    // List operations
    "list", "car", "cdr", "cons", "null?", "length", "append", "reverse",
    "map", "filter", "reduce", "fold", "foldr", "foldl",
    // Type predicates
    "number?", "symbol?", "string?", "list?", "pair?", "boolean?", "nil?",
    // Common functions
    "let", "let*", "letrec", "cond", "case", "when", "unless",
    // Math functions
    "sqrt", "expt", "log", "exp", "sin", "cos", "tan",
    "floor", "ceiling", "truncate", "round",
    // String operations
    "string-length", "string-append", "substring", "string-ref",
    "string=?", "string<?", "string>?",
    // I/O (planned)
    "display", "newline", "write", "read", "print",
    // Common Lisp style
    "defun", "defvar", "defconst", "setq",
    // Additional utilities
    "apply", "eval", "load", "exit", "help", "version",
];

pub struct ReplEditor {
    buffer: String<MAX_INPUT_LEN>,
    cursor_pos: usize,
    history: Vec<String<MAX_LINE_LEN>, MAX_HISTORY>,
    history_index: Option<usize>,
    temp_buffer: String<MAX_INPUT_LEN>, // For when navigating history
}

impl ReplEditor {
    pub fn new() -> Self {
        ReplEditor {
            buffer: String::new(),
            cursor_pos: 0,
            history: Vec::new(),
            history_index: None,
            temp_buffer: String::new(),
        }
    }

    pub fn read_line<F>(&mut self, uart_getc: F, uart_putc: &dyn Fn(u8)) -> Option<String<MAX_INPUT_LEN>>
    where
        F: Fn() -> u8,
    {
        self.buffer.clear();
        self.cursor_pos = 0;
        self.history_index = None;

        let mut escape_seq = [0u8; 3];
        let mut escape_pos = 0;

        loop {
            let c = uart_getc();

            // Handle escape sequences
            if escape_pos > 0 || c == 0x1b {
                escape_seq[escape_pos] = c;
                escape_pos += 1;

                if escape_pos == 3 {
                    self.handle_escape_sequence(&escape_seq, uart_putc);
                    escape_pos = 0;
                }
                continue;
            }

            match c {
                b'\r' | b'\n' => {
                    uart_putc(b'\n');
                    if !self.buffer.is_empty() {
                        self.add_to_history();
                    }
                    return Some(self.buffer.clone());
                }
                b'\t' => {
                    // Tab completion
                    self.handle_tab_completion(uart_putc);
                }
                0x7f | 0x08 => {
                    // Backspace
                    if self.cursor_pos > 0 {
                        self.cursor_pos -= 1;
                        // Rebuild buffer without character at cursor_pos
                        let mut new_buffer = String::new();
                        for (i, ch) in self.buffer.chars().enumerate() {
                            if i != self.cursor_pos {
                                let _ = new_buffer.push(ch);
                            }
                        }
                        self.buffer = new_buffer;
                        self.redraw_line(uart_putc);
                    }
                }
                0x04 => {
                    // Ctrl+D - Delete character at cursor
                    if self.cursor_pos < self.buffer.len() {
                        // Rebuild buffer without character at cursor_pos
                        let mut new_buffer = String::new();
                        for (i, ch) in self.buffer.chars().enumerate() {
                            if i != self.cursor_pos {
                                let _ = new_buffer.push(ch);
                            }
                        }
                        self.buffer = new_buffer;
                        self.redraw_line(uart_putc);
                    }
                }
                0x03 => {
                    // Ctrl+C - Cancel input
                    uart_putc(b'^');
                    uart_putc(b'C');
                    uart_putc(b'\n');
                    self.buffer.clear();
                    self.cursor_pos = 0;
                    return None;
                }
                0x01 => {
                    // Ctrl+A - Beginning of line
                    self.move_cursor_to(0, uart_putc);
                }
                0x05 => {
                    // Ctrl+E - End of line
                    self.move_cursor_to(self.buffer.len(), uart_putc);
                }
                0x0b => {
                    // Ctrl+K - Kill to end of line
                    self.buffer.truncate(self.cursor_pos);
                    self.redraw_line(uart_putc);
                }
                0x15 => {
                    // Ctrl+U - Kill entire line
                    self.buffer.clear();
                    self.cursor_pos = 0;
                    self.redraw_line(uart_putc);
                }
                0x20..=0x7e => {
                    // Printable characters
                    if self.buffer.len() < MAX_INPUT_LEN - 1 {
                        // heapless::String doesn't have insert, so we rebuild
                        let mut new_buffer = String::new();
                        for (i, ch) in self.buffer.chars().enumerate() {
                            if i == self.cursor_pos {
                                let _ = new_buffer.push(c as char);
                            }
                            let _ = new_buffer.push(ch);
                        }
                        if self.cursor_pos >= self.buffer.len() {
                            let _ = new_buffer.push(c as char);
                        }
                        self.buffer = new_buffer;
                        self.cursor_pos += 1;
                        self.redraw_line(uart_putc);
                    }
                }
                _ => {
                    // Ignore other characters
                }
            }
        }
    }

    fn handle_escape_sequence(&mut self, seq: &[u8; 3], uart_putc: &dyn Fn(u8)) {
        if seq == &KEY_UP {
            self.history_up(uart_putc);
        } else if seq == &KEY_DOWN {
            self.history_down(uart_putc);
        } else if seq == &KEY_LEFT {
            if self.cursor_pos > 0 {
                self.cursor_pos -= 1;
                uart_putc(0x1b);
                uart_putc(b'[');
                uart_putc(b'D');
            }
        } else if seq == &KEY_RIGHT {
            if self.cursor_pos < self.buffer.len() {
                self.cursor_pos += 1;
                uart_putc(0x1b);
                uart_putc(b'[');
                uart_putc(b'C');
            }
        }
    }

    fn history_up(&mut self, uart_putc: &dyn Fn(u8)) {
        if self.history.is_empty() {
            return;
        }

        // Save current buffer when starting history navigation
        if self.history_index.is_none() {
            self.temp_buffer.clear();
            let _ = self.temp_buffer.push_str(&self.buffer);
        }

        let new_index = match self.history_index {
            None => self.history.len() - 1,
            Some(idx) if idx > 0 => idx - 1,
            Some(idx) => idx,
        };

        self.history_index = Some(new_index);
        self.buffer.clear();
        let _ = self.buffer.push_str(&self.history[new_index]);
        self.cursor_pos = self.buffer.len();
        self.redraw_line(uart_putc);
    }

    fn history_down(&mut self, uart_putc: &dyn Fn(u8)) {
        match self.history_index {
            None => return,
            Some(idx) if idx < self.history.len() - 1 => {
                self.history_index = Some(idx + 1);
                self.buffer.clear();
                let _ = self.buffer.push_str(&self.history[idx + 1]);
                self.cursor_pos = self.buffer.len();
                self.redraw_line(uart_putc);
            }
            Some(_) => {
                // Restore temp buffer
                self.history_index = None;
                self.buffer.clear();
                let _ = self.buffer.push_str(&self.temp_buffer);
                self.cursor_pos = self.buffer.len();
                self.redraw_line(uart_putc);
            }
        }
    }

    fn handle_tab_completion(&mut self, uart_putc: &dyn Fn(u8)) {
        // Find the current word
        let mut word_start = self.cursor_pos;
        while word_start > 0 {
            let prev_char = self.buffer.as_bytes()[word_start - 1];
            if prev_char == b' ' || prev_char == b'(' || prev_char == b')' {
                break;
            }
            word_start -= 1;
        }

        if word_start == self.cursor_pos {
            return; // No word to complete
        }

        let word = &self.buffer[word_start..self.cursor_pos];
        
        // Find matching keywords
        let mut matches: Vec<&str, 16> = Vec::new();
        for keyword in KEYWORDS {
            if keyword.starts_with(word) {
                let _ = matches.push(keyword);
                if matches.is_full() {
                    break;
                }
            }
        }

        if matches.is_empty() {
            return; // No matches
        }

        if matches.len() == 1 {
            // Single match - complete it
            let completion = matches[0];
            let to_insert = &completion[word.len()..];
            
            // Rebuild buffer with completion
            let mut new_buffer = String::new();
            let _ = new_buffer.push_str(&self.buffer[..self.cursor_pos]);
            let _ = new_buffer.push_str(to_insert);
            let _ = new_buffer.push_str(&self.buffer[self.cursor_pos..]);
            self.cursor_pos += to_insert.len();
            self.buffer = new_buffer;
            
            self.redraw_line(uart_putc);
        } else {
            // Multiple matches - show them
            uart_putc(b'\n');
            for (i, m) in matches.iter().enumerate() {
                for ch in m.bytes() {
                    uart_putc(ch);
                }
                uart_putc(b' ');
                if (i + 1) % 8 == 0 {
                    uart_putc(b'\n');
                }
            }
            uart_putc(b'\n');
            // Redraw prompt and buffer
            uart_putc(b'>');
            uart_putc(b' ');
            for ch in self.buffer.as_bytes() {
                uart_putc(*ch);
            }
            // Move cursor to correct position
            let chars_after = self.buffer.len() - self.cursor_pos;
            for _ in 0..chars_after {
                uart_putc(0x1b);
                uart_putc(b'[');
                uart_putc(b'D');
            }
        }
    }

    fn redraw_line(&self, uart_putc: &dyn Fn(u8)) {
        // Clear line
        uart_putc(b'\r');
        uart_putc(0x1b);
        uart_putc(b'[');
        uart_putc(b'K');
        
        // Redraw prompt
        uart_putc(b'>');
        uart_putc(b' ');
        
        // Redraw buffer
        for ch in self.buffer.as_bytes() {
            uart_putc(*ch);
        }
        
        // Move cursor to correct position
        let chars_after = self.buffer.len() - self.cursor_pos;
        for _ in 0..chars_after {
            uart_putc(0x1b);
            uart_putc(b'[');
            uart_putc(b'D');
        }
    }

    fn move_cursor_to(&mut self, pos: usize, uart_putc: &dyn Fn(u8)) {
        let pos = pos.min(self.buffer.len());
        if pos < self.cursor_pos {
            for _ in 0..(self.cursor_pos - pos) {
                uart_putc(0x1b);
                uart_putc(b'[');
                uart_putc(b'D');
            }
        } else if pos > self.cursor_pos {
            for _ in 0..(pos - self.cursor_pos) {
                uart_putc(0x1b);
                uart_putc(b'[');
                uart_putc(b'C');
            }
        }
        self.cursor_pos = pos;
    }

    fn add_to_history(&mut self) {
        // Don't add empty lines or duplicates
        if self.buffer.is_empty() {
            return;
        }

        // Check if it's the same as the last entry
        if let Some(last) = self.history.last() {
            if last.as_str() == self.buffer.as_str() {
                return;
            }
        }

        let mut entry = String::new();
        let _ = entry.push_str(&self.buffer);

        if self.history.is_full() {
            self.history.remove(0);
        }
        let _ = self.history.push(entry);
    }

    pub fn get_history(&self) -> &Vec<String<MAX_LINE_LEN>, MAX_HISTORY> {
        &self.history
    }

    pub fn load_history(&mut self, history: &[String<MAX_LINE_LEN>]) {
        self.history.clear();
        for item in history {
            if !self.history.is_full() {
                let _ = self.history.push(item.clone());
            }
        }
    }
}

// Check if parentheses are balanced
pub fn is_balanced(input: &str) -> bool {
    let mut count = 0;
    let mut in_string = false;
    let mut escape = false;

    for ch in input.chars() {
        if escape {
            escape = false;
            continue;
        }

        match ch {
            '\\' if in_string => escape = true,
            '"' => in_string = !in_string,
            '(' if !in_string => count += 1,
            ')' if !in_string => {
                count -= 1;
                if count < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }

    count == 0
}
