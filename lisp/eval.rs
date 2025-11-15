// eval.rs - S-expression evaluator

extern crate alloc;
use alloc::boxed::Box;
use super::expr::Expr;
use super::env::Env;

pub struct Evaluator {
    global_env: Env,
}

impl Evaluator {
    pub fn new() -> Self {
        let env = Env::new();
        Evaluator {
            global_env: env,
        }
    }

    fn apply_builtin(name: &heapless::String<64>, args: &[Expr]) -> Result<Expr, &'static str> {
        match name.as_str() {
            "+" => {
                let sum = args.iter().try_fold(0i64, |acc, expr| {
                    match expr {
                        Expr::Number(n) => Ok(acc + n),
                        _ => Err("+ requires numbers"),
                    }
                })?;
                Ok(Expr::Number(sum))
            }
            "-" => {
                if args.is_empty() {
                    return Err("- requires at least one argument");
                }
                match &args[0] {
                    Expr::Number(first) => {
                        if args.len() == 1 {
                            Ok(Expr::Number(-first))
                        } else {
                            let diff = args[1..].iter().try_fold(*first, |acc, expr| {
                                match expr {
                                    Expr::Number(n) => Ok(acc - n),
                                    _ => Err("- requires numbers"),
                                }
                            })?;
                            Ok(Expr::Number(diff))
                        }
                    }
                    _ => Err("- requires numbers"),
                }
            }
            "*" => {
                let product = args.iter().try_fold(1i64, |acc, expr| {
                    match expr {
                        Expr::Number(n) => Ok(acc * n),
                        _ => Err("* requires numbers"),
                    }
                })?;
                Ok(Expr::Number(product))
            }
            "/" => {
                if args.len() != 2 {
                    return Err("/ requires exactly 2 arguments");
                }
                match (&args[0], &args[1]) {
                    (Expr::Number(a), Expr::Number(b)) => {
                        if *b == 0 {
                            Err("Division by zero")
                        } else {
                            Ok(Expr::Number(a / b))
                        }
                    }
                    _ => Err("/ requires numbers"),
                }
            }
            "=" => {
                if args.len() != 2 {
                    return Err("= requires exactly 2 arguments");
                }
                Ok(Expr::Bool(args[0] == args[1]))
            }
            "<" => {
                if args.len() != 2 {
                    return Err("< requires exactly 2 arguments");
                }
                match (&args[0], &args[1]) {
                    (Expr::Number(a), Expr::Number(b)) => Ok(Expr::Bool(a < b)),
                    _ => Err("< requires numbers"),
                }
            }
            ">" => {
                if args.len() != 2 {
                    return Err("> requires exactly 2 arguments");
                }
                match (&args[0], &args[1]) {
                    (Expr::Number(a), Expr::Number(b)) => Ok(Expr::Bool(a > b)),
                    _ => Err("> requires numbers"),
                }
            }
            "list" => {
                let mut new_list: heapless::Vec<Expr, 8> = heapless::Vec::new();
                for arg in args {
                    let _ = new_list.push(arg.clone());
                }
                Ok(Expr::List(Box::new(new_list)))
            }
            "car" => {
                if args.len() != 1 {
                    return Err("car requires exactly 1 argument");
                }
                match &args[0] {
                    Expr::List(items) if !items.is_empty() => Ok(items[0].clone()),
                    _ => Err("car requires a non-empty list"),
                }
            }
            "cdr" => {
                if args.len() != 1 {
                    return Err("cdr requires exactly 1 argument");
                }
                match &args[0] {
                    Expr::List(items) if !items.is_empty() => {
                        let mut new_list = heapless::Vec::new();
                        for i in 1..items.len() {
                            let _ = new_list.push(items[i].clone());
                        }
                        Ok(Expr::List(Box::new(new_list)))
                    }
                    _ => Err("cdr requires a non-empty list"),
                }
            }
            _ => Err("Unknown built-in function: {}"),
        }
    }

    pub fn eval(&mut self, expr: &Expr) -> Result<Expr, &'static str> {
        let env = &mut self.global_env;
        Self::eval_expr(expr, env)
    }

    fn eval_expr(expr: &Expr, env: &mut Env) -> Result<Expr, &'static str> {
        match expr {
            Expr::Number(_) | Expr::String(_) | Expr::Bool(_) | Expr::Nil => {
                Ok(expr.clone())
            }

            Expr::Symbol(name) => {
                env.get(name)
                    .ok_or_else(|| "Undefined symbol: {}")
            }

            Expr::List(items) if items.is_empty() => Ok(Expr::Nil),

            Expr::List(items) => {
                // Check for special forms
                if let Expr::Symbol(op) = &items[0] {
                    if op.as_str() == "quote" {
                        Self::eval_quote(&items[1..])
                    } else if op.as_str() == "define" {
                        Self::eval_define(&items[1..], env)
                    } else if op.as_str() == "if" {
                        Self::eval_if(&items[1..], env)
                    } else if op.as_str() == "lambda" {
                        Self::eval_lambda(&items[1..], env)
                    } else if op.as_str() == "begin" {
                        Self::eval_begin(&items[1..], env)
                    } else if op.as_str() == "set!" {
                        Self::eval_set(&items[1..], env)
                    } else {
                        Self::eval_application(items, env)
                    }
                } else {
                    Self::eval_application(items, env)
                }
            }
        }
    }

    fn eval_quote(args: &[Expr]) -> Result<Expr, &'static str> {
        if args.len() != 1 {
            return Err("quote requires exactly 1 argument");
        }
        Ok(args[0].clone())
    }

    fn eval_define(args: &[Expr], env: &mut Env) -> Result<Expr, &'static str> {
        if args.len() != 2 {
            return Err("define requires exactly 2 arguments");
        }

        if let Expr::Symbol(name) = &args[0] {
            let value = Self::eval_expr(&args[1], env)?;
            env.define(name.clone(), value.clone());
            Ok(value)
        } else {
            Err("define requires a symbol as first argument")
        }
    }

    fn eval_if(args: &[Expr], env: &mut Env) -> Result<Expr, &'static str> {
        if args.len() < 2 || args.len() > 3 {
            return Err("if requires 2 or 3 arguments");
        }

        let condition = Self::eval_expr(&args[0], env)?;
        if condition.is_truthy() {
            Self::eval_expr(&args[1], env)
        } else if args.len() == 3 {
            Self::eval_expr(&args[2], env)
        } else {
            Ok(Expr::Nil)
        }
    }

    fn eval_lambda(_args: &[Expr], _env: &mut Env) -> Result<Expr, &'static str> {
        // Simplified lambda - proper implementation would capture environment
        Err("lambda not yet fully implemented")
    }

    fn eval_begin(args: &[Expr], env: &mut Env) -> Result<Expr, &'static str> {
        if args.is_empty() {
            return Ok(Expr::Nil);
        }

        let mut result = Expr::Nil;
        for expr in args {
            result = Self::eval_expr(expr, env)?;
        }
        Ok(result)
    }

    fn eval_set(args: &[Expr], env: &mut Env) -> Result<Expr, &'static str> {
        if args.len() != 2 {
            return Err("set! requires exactly 2 arguments");
        }

        if let Expr::Symbol(name) = &args[0] {
            let value = Self::eval_expr(&args[1], env)?;
            env.set(name, value.clone())?;
            Ok(value)
        } else {
            Err("set! requires a symbol as first argument")
        }
    }

    fn eval_application(items: &[Expr], env: &mut Env) -> Result<Expr, &'static str> {
        // Evaluate arguments
        let mut args: heapless::Vec<Expr, 8> = heapless::Vec::new();
        for arg in &items[1..] {
            let _ = args.push(Self::eval_expr(arg, env)?);
        }

        // Check if it's a built-in function
        if let Expr::Symbol(name) = &items[0] {
            Self::apply_builtin(name, &args)
        } else {
            Err("Not a function")
        }
    }
}
