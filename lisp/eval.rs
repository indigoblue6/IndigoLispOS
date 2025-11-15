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
            "spawn" => {
                // (spawn function-expr)
                // Spawns a new task running the given function
                if args.len() != 1 {
                    return Err("spawn requires exactly 1 argument");
                }
                
                // For now, return a task ID placeholder
                // TODO: Implement actual task spawning with Lisp closure
                Ok(Expr::Number(1))
            }
            "task-id" => {
                // Return current task ID (always 0 for now)
                Ok(Expr::Number(0))
            }
            "sleep" => {
                // (sleep milliseconds)
                if args.len() != 1 {
                    return Err("sleep requires exactly 1 argument");
                }
                match &args[0] {
                    Expr::Number(ms) => {
                        use crate::drivers::timer;
                        timer::TIMER.delay_ms(*ms as u32);
                        Ok(Expr::Nil)
                    }
                    _ => Err("sleep requires a number"),
                }
            }
            "ticks" => {
                // Return system tick count
                use crate::drivers::timer;
                let ticks = timer::TIMER.get_ticks();
                // Limit to i64::MAX to avoid overflow
                if ticks > i64::MAX as u64 {
                    Ok(Expr::Number(i64::MAX))
                } else {
                    Ok(Expr::Number(ticks as i64))
                }
            }
            _ => Err("Unknown built-in function"),
        }
    }

    pub fn eval(&mut self, expr: &Expr) -> Result<Expr, &'static str> {
        let env = &mut self.global_env;
        Self::eval_expr(expr, env)
    }

    fn eval_expr(expr: &Expr, env: &mut Env) -> Result<Expr, &'static str> {
        match expr {
            Expr::Number(_) | Expr::String(_) | Expr::Bool(_) | Expr::Nil | Expr::Lambda(..) | Expr::Macro(..) => {
                Ok(expr.clone())
            }

            Expr::Symbol(name) => {
                env.get(name)
                    .ok_or_else(|| "Undefined symbol")
            }

            Expr::List(items) if items.is_empty() => Ok(Expr::Nil),

            Expr::List(items) => {
                // Check for special forms
                if let Expr::Symbol(op) = &items[0] {
                    if op.as_str() == "quote" {
                        Self::eval_quote(&items[1..])
                    } else if op.as_str() == "define" {
                        Self::eval_define(&items[1..], env)
                    } else if op.as_str() == "defmacro" {
                        Self::eval_defmacro(&items[1..], env)
                    } else if op.as_str() == "if" {
                        Self::eval_if(&items[1..], env)
                    } else if op.as_str() == "lambda" {
                        Self::eval_lambda(&items[1..], env)
                    } else if op.as_str() == "begin" {
                        Self::eval_begin(&items[1..], env)
                    } else if op.as_str() == "set!" {
                        Self::eval_set(&items[1..], env)
                    } else {
                        // Check if it's a macro and expand it
                        if let Some(Expr::Macro(..)) = env.get(op) {
                            let expanded = Self::expand_macro(items, env)?;
                            Self::eval_expr(&expanded, env)
                        } else {
                            Self::eval_application(items, env)
                        }
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

    fn eval_defmacro(args: &[Expr], env: &mut Env) -> Result<Expr, &'static str> {
        if args.len() < 3 {
            return Err("defmacro requires at least 3 arguments");
        }

        // (defmacro name (params...) body...)
        if let Expr::Symbol(name) = &args[0] {
            // Second arg is parameter list
            let params = match &args[1] {
                Expr::List(items) => {
                    let mut params: heapless::Vec<heapless::String<64>, 4> = heapless::Vec::new();
                    for item in items.iter() {
                        if let Expr::Symbol(s) = item {
                            let _ = params.push(s.clone());
                        } else {
                            return Err("macro parameters must be symbols");
                        }
                    }
                    params
                }
                _ => return Err("defmacro requires parameter list"),
            };

            // Rest are body expressions
            let mut body: heapless::Vec<Expr, 8> = heapless::Vec::new();
            for expr in &args[2..] {
                let _ = body.push(expr.clone());
            }

            let macro_value = Expr::Macro(Box::new((params, body)));
            env.define(name.clone(), macro_value.clone());
            Ok(macro_value)
        } else {
            Err("defmacro requires a symbol as first argument")
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

    fn eval_lambda(args: &[Expr], env: &mut Env) -> Result<Expr, &'static str> {
        if args.len() < 2 {
            return Err("lambda requires at least 2 arguments");
        }

        // First arg is parameter list
        let params = match &args[0] {
            Expr::List(items) => {
                let mut params: heapless::Vec<heapless::String<64>, 4> = heapless::Vec::new();
                for item in items.iter() {
                    if let Expr::Symbol(s) = item {
                        let _ = params.push(s.clone());
                    } else {
                        return Err("lambda parameters must be symbols");
                    }
                }
                params
            }
            _ => return Err("lambda requires parameter list"),
        };

        // Rest are body expressions
        let mut body: heapless::Vec<Expr, 8> = heapless::Vec::new();
        for expr in &args[1..] {
            let _ = body.push(expr.clone());
        }

        // Capture current environment
        let env_snapshot = Some(env.snapshot());

        Ok(Expr::Lambda(Box::new((params, body, env_snapshot))))
    }

    fn expand_macro(items: &[Expr], env: &mut Env) -> Result<Expr, &'static str> {
        if let Expr::Symbol(name) = &items[0] {
            if let Some(Expr::Macro(macro_data)) = env.get(name) {
                let (params, body) = &*macro_data;
                
                // Bind arguments (unevaluated) to parameters
                if items.len() - 1 != params.len() {
                    return Err("macro argument count mismatch");
                }

                let mut macro_env = Env::new();
                for (i, param) in params.iter().enumerate() {
                    macro_env.define(param.clone(), items[i + 1].clone());
                }

                // Evaluate macro body with substituted arguments
                let mut result = Expr::Nil;
                for expr in body.iter() {
                    result = Self::eval_expr(expr, &mut macro_env)?;
                }
                Ok(result)
            } else {
                Err("Not a macro")
            }
        } else {
            Err("Macro name must be a symbol")
        }
    }

    fn apply_lambda(
        lambda_data: &(heapless::Vec<heapless::String<64>, 4>, heapless::Vec<Expr, 8>, Option<super::expr::EnvSnapshot>),
        args: &[Expr],
        current_env: &Env,
    ) -> Result<Expr, &'static str> {
        let (params, body, env_snapshot) = lambda_data;

        if args.len() != params.len() {
            return Err("lambda argument count mismatch");
        }

        // Create new environment: start with captured environment, 
        // then extend with current global environment for recursive calls
        let mut lambda_env = if let Some(snapshot) = env_snapshot {
            Env::from_snapshot(snapshot)
        } else {
            Env::new()
        };
        
        // Extend with current environment to support recursion
        lambda_env.extend(&current_env.snapshot());

        // Bind arguments to parameters (these override any captured values)
        for (i, param) in params.iter().enumerate() {
            lambda_env.define(param.clone(), args[i].clone());
        }

        // Evaluate body
        let mut result = Expr::Nil;
        for expr in body.iter() {
            result = Self::eval_expr(expr, &mut lambda_env)?;
        }
        Ok(result)
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

    fn is_builtin(name: &str) -> bool {
        matches!(name, 
            "+" | "-" | "*" | "/" | "=" | "<" | ">" | 
            "cons" | "list" | "car" | "cdr" | 
            "spawn" | "task-id" | "sleep" | "ticks"
        )
    }

    fn eval_application(items: &[Expr], env: &mut Env) -> Result<Expr, &'static str> {
        // Check if the operator is a symbol (builtin function name)
        let operator = if let Expr::Symbol(name) = &items[0] {
            // Check if it's a builtin function
            if Self::is_builtin(name.as_str()) {
                items[0].clone() // Don't evaluate builtin function names
            } else {
                // It's a user-defined function or lambda
                Self::eval_expr(&items[0], env)?
            }
        } else {
            // Evaluate the operator (e.g., lambda expression)
            Self::eval_expr(&items[0], env)?
        };

        // Evaluate arguments
        let mut args: heapless::Vec<Expr, 8> = heapless::Vec::new();
        for arg in &items[1..] {
            let _ = args.push(Self::eval_expr(arg, env)?);
        }

        // Apply based on operator type
        match operator {
            Expr::Lambda(lambda_data) => {
                Self::apply_lambda(&lambda_data, &args, env)
            }
            Expr::Symbol(name) => {
                Self::apply_builtin(&name, &args)
            }
            _ => Err("Not a function")
        }
    }
}
