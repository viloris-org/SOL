use logos::Logos;

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\r]+")]
pub enum Token {
    // 字面量
    #[regex(r#""([^"\\]|\\["\\bnfrt])*""#, |lex| lex.slice()[1..lex.slice().len()-1].to_string())]
    String(String),
    
    #[regex(r"-?[0-9]+\.?[0-9]*", |lex| lex.slice().parse().ok())]
    Number(f64),
    
    #[token("true", |_| true)]
    #[token("false", |_| false)]
    Bool(bool),
    
    // 关键字
    #[token("let")]
    Let,
    
    #[token("def")]
    Def,
    
    #[token("if")]
    If,
    
    #[token("else")]
    Else,
    
    #[token("for")]
    For,
    
    #[token("in")]
    In,
    
    #[token("while")]
    While,
    
    #[token("return")]
    Return,
    
    #[token("null")]
    Null,
    
    // 标识符 - 必须在关键字之后
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_-]*", |lex| lex.slice().to_string())]
    Ident(String),
    
    // 运算符
    #[token("+")]
    Plus,
    
    #[token("-")]
    Minus,
    
    #[token("*")]
    Star,
    
    #[token("/")]
    Slash,
    
    #[token("%")]
    Percent,
    
    #[token("==")]
    Eq,
    
    #[token("!=")]
    NotEq,
    
    #[token(">")]
    Gt,
    
    #[token("<")]
    Lt,
    
    #[token(">=")]
    GtEq,
    
    #[token("<=")]
    LtEq,
    
    #[token("&&")]
    And,
    
    #[token("||")]
    Or,
    
    #[token("!")]
    Not,
    
    #[token("=")]
    Assign,
    
    // 分隔符
    #[token("(")]
    LParen,
    
    #[token(")")]
    RParen,
    
    #[token("{")]
    LBrace,
    
    #[token("}")]
    RBrace,
    
    #[token("[")]
    LBracket,
    
    #[token("]")]
    RBracket,
    
    #[token(",")]
    Comma,
    
    #[token(":")]
    Colon,
    
    #[token(";")]
    Semicolon,
    
    #[token("|")]
    Pipe,
    
    #[token(".")]
    Dot,
    
    // 特殊
    #[token("$")]
    Dollar,
    
    #[token("->")]
    Arrow,
    
    #[token("..")]
    DotDot,
    
    // 控制
    #[token("\n")]
    Newline,
    
    #[token("--")]
    DoubleDash,
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Token::String(s) => write!(f, "\"{}\"", s),
            Token::Number(n) => write!(f, "{}", n),
            Token::Bool(b) => write!(f, "{}", b),
            Token::Ident(s) => write!(f, "{}", s),
            _ => write!(f, "{:?}", self),
        }
    }
}
