use crate::parser::{Expr, Value};
use crate::runtime::{RuntimeError, RuntimeResult};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

pub struct Evaluator {
    env: crate::runtime::Environment,
    builtins: crate::builtins::BuiltinRegistry,
}

impl Evaluator {
    pub fn new() -> Self {
        Self {
            env: crate::runtime::Environment::new(),
            builtins: crate::builtins::BuiltinRegistry::new(),
        }
    }

    /// Returns whether `name` is implemented by Lyra itself.
    ///
    /// The interactive shell uses this to keep Lyra expressions on the
    /// structured evaluator while forwarding external invocations without
    /// first tokenizing and rebuilding their command line.
    pub fn has_builtin(&self, name: &str) -> bool {
        self.builtins.has_command(name)
    }

    /// Snapshot Lyra scalar variables for an external child process.
    pub fn external_environment(&self) -> HashMap<String, String> {
        self.env.process_variables()
    }

    pub fn eval_stmts<'a>(
        &'a mut self,
        stmts: &'a [crate::parser::Stmt],
    ) -> Pin<Box<dyn Future<Output = RuntimeResult<Value>> + 'a>> {
        Box::pin(async move {
            let mut last_value = Value::Null;

            for stmt in stmts {
                last_value = self.eval_stmt(stmt).await?;
            }

            Ok(last_value)
        })
    }

    pub fn eval_stmt<'a>(
        &'a mut self,
        stmt: &'a crate::parser::Stmt,
    ) -> Pin<Box<dyn Future<Output = RuntimeResult<Value>> + 'a>> {
        Box::pin(async move {
            use crate::parser::Stmt;

            match stmt {
                Stmt::Expr(expr) => self.eval_expr(expr).await,

                Stmt::Let { name, value } => {
                    let val = self.eval_expr(value).await?;
                    self.env.define(name.clone(), val.clone());
                    Ok(Value::Null)
                }

                Stmt::Def {
                    name,
                    params: _,
                    body: _,
                } => {
                    // 函数定义目前简化处理，后续实现
                    self.env.define(name.clone(), Value::Null);
                    Ok(Value::Null)
                }

                Stmt::If {
                    condition,
                    then_branch,
                    else_branch,
                } => {
                    let cond_val = self.eval_expr(condition).await?;

                    if cond_val.is_truthy() {
                        self.eval_stmts(then_branch).await
                    } else if let Some(else_stmts) = else_branch {
                        self.eval_stmts(else_stmts).await
                    } else {
                        Ok(Value::Null)
                    }
                }

                Stmt::For { var, iter, body } => {
                    let iter_val = self.eval_expr(iter).await?;

                    let items = match iter_val {
                        Value::List(items) => items,
                        Value::Table { rows, .. } => rows.into_iter().map(Value::Record).collect(),
                        _ => {
                            return Err(RuntimeError::TypeError {
                                expected: "list or table".to_string(),
                                got: iter_val.type_name().to_string(),
                            });
                        }
                    };

                    self.env.push_scope();

                    for item in items {
                        self.env.set(var.clone(), item);
                        self.eval_stmts(body).await?;
                    }

                    self.env.pop_scope();

                    Ok(Value::Null)
                }

                Stmt::While { condition, body } => {
                    self.env.push_scope();

                    while self.eval_expr(condition).await?.is_truthy() {
                        self.eval_stmts(body).await?;
                    }

                    self.env.pop_scope();

                    Ok(Value::Null)
                }

                Stmt::Return(expr) => {
                    if let Some(e) = expr {
                        self.eval_expr(e).await
                    } else {
                        Ok(Value::Null)
                    }
                }
            }
        })
    }

    pub fn eval_expr<'a>(
        &'a mut self,
        expr: &'a Expr,
    ) -> Pin<Box<dyn Future<Output = RuntimeResult<Value>> + 'a>> {
        Box::pin(async move {
            match expr {
                Expr::Literal(v) => Ok(v.clone()),

                Expr::Variable(name) => self
                    .env
                    .get(name)
                    .ok_or_else(|| RuntimeError::UndefinedVariable(name.clone())),

                Expr::Binary { left, op, right } => {
                    let left_val = self.eval_expr(left).await?;
                    let right_val = self.eval_expr(right).await?;
                    self.eval_binary_op(&left_val, op, &right_val)
                }

                Expr::Unary { op, expr } => {
                    let val = self.eval_expr(expr).await?;
                    self.eval_unary_op(op, &val)
                }

                Expr::Call { name, args, flags } => self.eval_call(name, args, flags).await,

                Expr::Pipeline { stages } => self.eval_pipeline(stages).await,

                Expr::List(items) => {
                    let mut values = Vec::new();
                    for item in items {
                        values.push(self.eval_expr(item).await?);
                    }
                    Ok(Value::List(values))
                }

                Expr::Record(fields) => {
                    let mut record = HashMap::new();
                    for (key, value_expr) in fields {
                        let value = self.eval_expr(value_expr).await?;
                        record.insert(key.clone(), value);
                    }
                    Ok(Value::Record(record))
                }

                Expr::Index { expr, index } => {
                    let container = self.eval_expr(expr).await?;
                    let idx = self.eval_expr(index).await?;

                    match (container, idx) {
                        (Value::List(items), Value::Number(n)) => {
                            let index = n as usize;
                            items.get(index).cloned().ok_or_else(|| {
                                RuntimeError::Custom("Index out of bounds".to_string())
                            })
                        }
                        (Value::Record(fields), Value::String(key)) => fields
                            .get(&key)
                            .cloned()
                            .ok_or_else(|| RuntimeError::Custom(format!("Key not found: {}", key))),
                        _ => Err(RuntimeError::TypeError {
                            expected: "list[number] or record[string]".to_string(),
                            got: "invalid indexing".to_string(),
                        }),
                    }
                }

                Expr::Field { expr, field } => {
                    let val = self.eval_expr(expr).await?;

                    match val {
                        Value::Record(fields) => fields.get(field).cloned().ok_or_else(|| {
                            RuntimeError::Custom(format!("Field not found: {}", field))
                        }),
                        _ => Err(RuntimeError::TypeError {
                            expected: "record".to_string(),
                            got: val.type_name().to_string(),
                        }),
                    }
                }
            }
        })
    }

    fn eval_binary_op(
        &self,
        left: &Value,
        op: &crate::parser::BinaryOp,
        right: &Value,
    ) -> RuntimeResult<Value> {
        use crate::parser::BinaryOp;

        match (left, op, right) {
            // 算术运算
            (Value::Number(l), BinaryOp::Add, Value::Number(r)) => Ok(Value::Number(l + r)),
            (Value::Number(l), BinaryOp::Sub, Value::Number(r)) => Ok(Value::Number(l - r)),
            (Value::Number(l), BinaryOp::Mul, Value::Number(r)) => Ok(Value::Number(l * r)),
            (Value::Number(l), BinaryOp::Div, Value::Number(r)) => {
                if *r == 0.0 {
                    Err(RuntimeError::Custom("Division by zero".to_string()))
                } else {
                    Ok(Value::Number(l / r))
                }
            }
            (Value::Number(l), BinaryOp::Mod, Value::Number(r)) => Ok(Value::Number(l % r)),

            // 字符串连接
            (Value::String(l), BinaryOp::Add, Value::String(r)) => {
                Ok(Value::String(format!("{}{}", l, r)))
            }

            // 比较运算
            (Value::Number(l), BinaryOp::Eq, Value::Number(r)) => Ok(Value::Bool(l == r)),
            (Value::Number(l), BinaryOp::NotEq, Value::Number(r)) => Ok(Value::Bool(l != r)),
            (Value::Number(l), BinaryOp::Gt, Value::Number(r)) => Ok(Value::Bool(l > r)),
            (Value::Number(l), BinaryOp::Lt, Value::Number(r)) => Ok(Value::Bool(l < r)),
            (Value::Number(l), BinaryOp::GtEq, Value::Number(r)) => Ok(Value::Bool(l >= r)),
            (Value::Number(l), BinaryOp::LtEq, Value::Number(r)) => Ok(Value::Bool(l <= r)),

            (Value::String(l), BinaryOp::Eq, Value::String(r)) => Ok(Value::Bool(l == r)),
            (Value::String(l), BinaryOp::NotEq, Value::String(r)) => Ok(Value::Bool(l != r)),

            (Value::Bool(l), BinaryOp::Eq, Value::Bool(r)) => Ok(Value::Bool(l == r)),
            (Value::Bool(l), BinaryOp::NotEq, Value::Bool(r)) => Ok(Value::Bool(l != r)),

            // 逻辑运算
            (l, BinaryOp::And, r) => Ok(Value::Bool(l.is_truthy() && r.is_truthy())),
            (l, BinaryOp::Or, r) => Ok(Value::Bool(l.is_truthy() || r.is_truthy())),

            _ => Err(RuntimeError::TypeError {
                expected: "compatible types for operation".to_string(),
                got: format!("{} {:?} {}", left.type_name(), op, right.type_name()),
            }),
        }
    }

    fn eval_unary_op(&self, op: &crate::parser::UnaryOp, val: &Value) -> RuntimeResult<Value> {
        use crate::parser::UnaryOp;

        match (op, val) {
            (UnaryOp::Not, v) => Ok(Value::Bool(!v.is_truthy())),
            (UnaryOp::Neg, Value::Number(n)) => Ok(Value::Number(-n)),
            _ => Err(RuntimeError::TypeError {
                expected: "compatible type for unary operation".to_string(),
                got: val.type_name().to_string(),
            }),
        }
    }

    fn eval_call<'a>(
        &'a mut self,
        name: &'a str,
        args: &'a [Expr],
        flags: &'a HashMap<String, Expr>,
    ) -> Pin<Box<dyn Future<Output = RuntimeResult<Value>> + 'a>> {
        Box::pin(async move {
            // 求值参数
            let mut arg_values = Vec::new();
            for arg in args {
                arg_values.push(self.eval_expr(arg).await?);
            }

            // 求值 flags
            let mut flag_values = HashMap::new();
            for (key, value) in flags {
                flag_values.insert(key.clone(), self.eval_expr(value).await?);
            }

            // 执行内建命令
            self.builtins.execute(name, arg_values, flag_values).await
        })
    }

    fn eval_pipeline<'a>(
        &'a mut self,
        stages: &'a [Expr],
    ) -> Pin<Box<dyn Future<Output = RuntimeResult<Value>> + 'a>> {
        Box::pin(async move {
            if stages.is_empty() {
                return Ok(Value::Null);
            }

            // 第一阶段：没有输入
            let mut value = self.eval_expr(&stages[0]).await?;

            // 后续阶段：管道输入
            for stage in &stages[1..] {
                // 设置 $in 变量为上一阶段的输出
                self.env.set("in".to_string(), value.clone());

                // 执行当前阶段
                value = self.eval_expr(stage).await?;
            }

            Ok(value)
        })
    }
}

impl Default for Evaluator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    #[tokio::test]
    async fn test_eval_literal() {
        let mut eval = Evaluator::new();
        let expr = Expr::Literal(Value::Number(42.0));

        let result = eval.eval_expr(&expr).await.unwrap();
        assert_eq!(result, Value::Number(42.0));
    }

    #[tokio::test]
    async fn test_eval_binary_op() {
        use crate::parser::BinaryOp;

        let mut eval = Evaluator::new();
        let expr = Expr::Binary {
            left: Box::new(Expr::Literal(Value::Number(10.0))),
            op: BinaryOp::Add,
            right: Box::new(Expr::Literal(Value::Number(32.0))),
        };

        let result = eval.eval_expr(&expr).await.unwrap();
        assert_eq!(result, Value::Number(42.0));
    }

    #[tokio::test]
    async fn test_eval_let() {
        let mut eval = Evaluator::new();
        let mut parser = Parser::new("let x = 42");
        let stmts = parser.parse().unwrap();

        eval.eval_stmts(&stmts).await.unwrap();

        let x = eval.env.get("x").unwrap();
        assert_eq!(x, Value::Number(42.0));
    }
}
