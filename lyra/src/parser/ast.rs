use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    // 字面量
    Literal(Value),

    // 变量
    Variable(String),

    // 二元运算
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
    },

    // 一元运算
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },

    // 函数调用
    Call {
        name: String,
        args: Vec<Expr>,
        flags: HashMap<String, Expr>,
    },

    // 管道
    Pipeline {
        stages: Vec<Expr>,
    },

    // 列表
    List(Vec<Expr>),

    // 记录
    Record(HashMap<String, Expr>),

    // 索引
    Index {
        expr: Box<Expr>,
        index: Box<Expr>,
    },

    // 字段访问
    Field {
        expr: Box<Expr>,
        field: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    // 表达式语句
    Expr(Expr),

    // 变量绑定
    Let {
        name: String,
        value: Expr,
    },

    // 函数定义
    Def {
        name: String,
        params: Vec<String>,
        body: Vec<Stmt>,
    },

    // 条件
    If {
        condition: Expr,
        then_branch: Vec<Stmt>,
        else_branch: Option<Vec<Stmt>>,
    },

    // 循环
    For {
        var: String,
        iter: Expr,
        body: Vec<Stmt>,
    },

    While {
        condition: Expr,
        body: Vec<Stmt>,
    },

    // 返回
    Return(Option<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    String(String),
    Number(f64),
    Bool(bool),
    Null,
    List(Vec<Value>),
    Record(HashMap<String, Value>),
    Table {
        columns: Vec<String>,
        rows: Vec<HashMap<String, Value>>,
    },
}

impl Value {
    pub fn type_name(&self) -> &str {
        match self {
            Value::String(_) => "string",
            Value::Number(_) => "number",
            Value::Bool(_) => "bool",
            Value::Null => "null",
            Value::List(_) => "list",
            Value::Record(_) => "record",
            Value::Table { .. } => "table",
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Null => false,
            Value::Number(n) => *n != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::List(l) => !l.is_empty(),
            Value::Record(r) => !r.is_empty(),
            Value::Table { rows, .. } => !rows.is_empty(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    NotEq,
    Gt,
    Lt,
    GtEq,
    LtEq,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOp {
    Not,
    Neg,
}
