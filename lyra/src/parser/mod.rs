pub mod ast;
pub mod error;

pub use ast::*;
pub use error::*;

use crate::lexer::{Lexer, Token};
use std::collections::HashMap;

pub struct Parser {
    lexer: Lexer,
}

impl Parser {
    pub fn new(input: &str) -> Self {
        Self {
            lexer: Lexer::new(input),
        }
    }
    
    pub fn parse(&mut self) -> ParseResult<Vec<Stmt>> {
        let mut stmts = Vec::new();
        
        while !self.lexer.is_at_end() {
            // 跳过换行符
            if matches!(self.lexer.peek(), Some(Token::Newline)) {
                self.lexer.advance();
                continue;
            }
            
            stmts.push(self.parse_stmt()?);
        }
        
        Ok(stmts)
    }
    
    fn parse_stmt(&mut self) -> ParseResult<Stmt> {
        match self.lexer.peek() {
            Some(Token::Let) => self.parse_let(),
            Some(Token::Def) => self.parse_def(),
            Some(Token::If) => self.parse_if(),
            Some(Token::For) => self.parse_for(),
            Some(Token::While) => self.parse_while(),
            Some(Token::Return) => self.parse_return(),
            _ => {
                let expr = self.parse_pipeline()?;
                Ok(Stmt::Expr(expr))
            }
        }
    }
    
    fn parse_let(&mut self) -> ParseResult<Stmt> {
        self.expect_token(Token::Let)?;
        
        let name = match self.lexer.advance() {
            Some(Token::Ident(s)) => s,
            Some(tok) => return Err(ParseError::ExpectedToken {
                expected: "identifier".to_string(),
                found: tok.to_string(),
            }),
            None => return Err(ParseError::UnexpectedEof),
        };
        
        self.expect_token(Token::Assign)?;
        
        let value = self.parse_pipeline()?;
        
        Ok(Stmt::Let { name, value })
    }
    
    fn parse_def(&mut self) -> ParseResult<Stmt> {
        self.expect_token(Token::Def)?;
        
        let name = match self.lexer.advance() {
            Some(Token::Ident(s)) => s,
            Some(tok) => return Err(ParseError::ExpectedToken {
                expected: "identifier".to_string(),
                found: tok.to_string(),
            }),
            None => return Err(ParseError::UnexpectedEof),
        };
        
        self.expect_token(Token::LParen)?;
        
        let mut params = Vec::new();
        while !matches!(self.lexer.peek(), Some(Token::RParen)) {
            match self.lexer.advance() {
                Some(Token::Ident(s)) => params.push(s),
                Some(tok) => return Err(ParseError::ExpectedToken {
                    expected: "parameter name".to_string(),
                    found: tok.to_string(),
                }),
                None => return Err(ParseError::UnexpectedEof),
            }
            
            if matches!(self.lexer.peek(), Some(Token::Comma)) {
                self.lexer.advance();
            }
        }
        
        self.expect_token(Token::RParen)?;
        self.expect_token(Token::LBrace)?;
        
        let mut body = Vec::new();
        while !matches!(self.lexer.peek(), Some(Token::RBrace)) {
            if matches!(self.lexer.peek(), Some(Token::Newline)) {
                self.lexer.advance();
                continue;
            }
            body.push(self.parse_stmt()?);
        }
        
        self.expect_token(Token::RBrace)?;
        
        Ok(Stmt::Def { name, params, body })
    }
    
    fn parse_if(&mut self) -> ParseResult<Stmt> {
        self.expect_token(Token::If)?;
        
        let condition = self.parse_expr()?;
        
        self.expect_token(Token::LBrace)?;
        
        let mut then_branch = Vec::new();
        while !matches!(self.lexer.peek(), Some(Token::RBrace)) {
            if matches!(self.lexer.peek(), Some(Token::Newline)) {
                self.lexer.advance();
                continue;
            }
            then_branch.push(self.parse_stmt()?);
        }
        
        self.expect_token(Token::RBrace)?;
        
        let else_branch = if matches!(self.lexer.peek(), Some(Token::Else)) {
            self.lexer.advance();
            self.expect_token(Token::LBrace)?;
            
            let mut branch = Vec::new();
            while !matches!(self.lexer.peek(), Some(Token::RBrace)) {
                if matches!(self.lexer.peek(), Some(Token::Newline)) {
                    self.lexer.advance();
                    continue;
                }
                branch.push(self.parse_stmt()?);
            }
            
            self.expect_token(Token::RBrace)?;
            Some(branch)
        } else {
            None
        };
        
        Ok(Stmt::If {
            condition,
            then_branch,
            else_branch,
        })
    }
    
    fn parse_for(&mut self) -> ParseResult<Stmt> {
        self.expect_token(Token::For)?;
        
        let var = match self.lexer.advance() {
            Some(Token::Ident(s)) => s,
            Some(tok) => return Err(ParseError::ExpectedToken {
                expected: "variable name".to_string(),
                found: tok.to_string(),
            }),
            None => return Err(ParseError::UnexpectedEof),
        };
        
        self.expect_token(Token::In)?;
        
        let iter = self.parse_expr()?;
        
        self.expect_token(Token::LBrace)?;
        
        let mut body = Vec::new();
        while !matches!(self.lexer.peek(), Some(Token::RBrace)) {
            if matches!(self.lexer.peek(), Some(Token::Newline)) {
                self.lexer.advance();
                continue;
            }
            body.push(self.parse_stmt()?);
        }
        
        self.expect_token(Token::RBrace)?;
        
        Ok(Stmt::For { var, iter, body })
    }
    
    fn parse_while(&mut self) -> ParseResult<Stmt> {
        self.expect_token(Token::While)?;
        
        let condition = self.parse_expr()?;
        
        self.expect_token(Token::LBrace)?;
        
        let mut body = Vec::new();
        while !matches!(self.lexer.peek(), Some(Token::RBrace)) {
            if matches!(self.lexer.peek(), Some(Token::Newline)) {
                self.lexer.advance();
                continue;
            }
            body.push(self.parse_stmt()?);
        }
        
        self.expect_token(Token::RBrace)?;
        
        Ok(Stmt::While { condition, body })
    }
    
    fn parse_return(&mut self) -> ParseResult<Stmt> {
        self.expect_token(Token::Return)?;
        
        if matches!(self.lexer.peek(), Some(Token::Newline) | None) {
            Ok(Stmt::Return(None))
        } else {
            let expr = self.parse_expr()?;
            Ok(Stmt::Return(Some(expr)))
        }
    }
    
    fn parse_pipeline(&mut self) -> ParseResult<Expr> {
        let mut stages = vec![self.parse_expr()?];
        
        while matches!(self.lexer.peek(), Some(Token::Pipe)) {
            self.lexer.advance();
            stages.push(self.parse_expr()?);
        }
        
        if stages.len() == 1 {
            Ok(stages.into_iter().next().unwrap())
        } else {
            Ok(Expr::Pipeline { stages })
        }
    }
    
    fn parse_expr(&mut self) -> ParseResult<Expr> {
        self.parse_or()
    }
    
    fn parse_or(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_and()?;
        
        while matches!(self.lexer.peek(), Some(Token::Or)) {
            self.lexer.advance();
            let right = self.parse_and()?;
            left = Expr::Binary {
                left: Box::new(left),
                op: BinaryOp::Or,
                right: Box::new(right),
            };
        }
        
        Ok(left)
    }
    
    fn parse_and(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_comparison()?;
        
        while matches!(self.lexer.peek(), Some(Token::And)) {
            self.lexer.advance();
            let right = self.parse_comparison()?;
            left = Expr::Binary {
                left: Box::new(left),
                op: BinaryOp::And,
                right: Box::new(right),
            };
        }
        
        Ok(left)
    }
    
    fn parse_comparison(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_addition()?;
        
        while let Some(token) = self.lexer.peek() {
            let op = match token {
                Token::Eq => BinaryOp::Eq,
                Token::NotEq => BinaryOp::NotEq,
                Token::Gt => BinaryOp::Gt,
                Token::Lt => BinaryOp::Lt,
                Token::GtEq => BinaryOp::GtEq,
                Token::LtEq => BinaryOp::LtEq,
                _ => break,
            };
            
            self.lexer.advance();
            let right = self.parse_addition()?;
            left = Expr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        
        Ok(left)
    }
    
    fn parse_addition(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_multiplication()?;
        
        while let Some(token) = self.lexer.peek() {
            let op = match token {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _ => break,
            };
            
            self.lexer.advance();
            let right = self.parse_multiplication()?;
            left = Expr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        
        Ok(left)
    }
    
    fn parse_multiplication(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_unary()?;
        
        while let Some(token) = self.lexer.peek() {
            let op = match token {
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                Token::Percent => BinaryOp::Mod,
                _ => break,
            };
            
            self.lexer.advance();
            let right = self.parse_unary()?;
            left = Expr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        
        Ok(left)
    }
    
    fn parse_unary(&mut self) -> ParseResult<Expr> {
        match self.lexer.peek() {
            Some(Token::Not) => {
                self.lexer.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(expr),
                })
            }
            Some(Token::Minus) => {
                self.lexer.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::Unary {
                    op: UnaryOp::Neg,
                    expr: Box::new(expr),
                })
            }
            _ => self.parse_postfix(),
        }
    }
    
    fn parse_postfix(&mut self) -> ParseResult<Expr> {
        let mut expr = self.parse_primary()?;
        
        loop {
            match self.lexer.peek() {
                Some(Token::LBracket) => {
                    self.lexer.advance();
                    let index = self.parse_expr()?;
                    self.expect_token(Token::RBracket)?;
                    expr = Expr::Index {
                        expr: Box::new(expr),
                        index: Box::new(index),
                    };
                }
                Some(Token::Dot) => {
                    self.lexer.advance();
                    match self.lexer.advance() {
                        Some(Token::Ident(field)) => {
                            expr = Expr::Field {
                                expr: Box::new(expr),
                                field,
                            };
                        }
                        Some(tok) => return Err(ParseError::ExpectedToken {
                            expected: "field name".to_string(),
                            found: tok.to_string(),
                        }),
                        None => return Err(ParseError::UnexpectedEof),
                    }
                }
                _ => break,
            }
        }
        
        Ok(expr)
    }
    
    fn parse_primary(&mut self) -> ParseResult<Expr> {
        match self.lexer.peek() {
            Some(Token::Number(n)) => {
                let n = *n;
                self.lexer.advance();
                Ok(Expr::Literal(Value::Number(n)))
            }
            Some(Token::String(s)) => {
                let s = s.clone();
                self.lexer.advance();
                Ok(Expr::Literal(Value::String(s)))
            }
            Some(Token::Bool(b)) => {
                let b = *b;
                self.lexer.advance();
                Ok(Expr::Literal(Value::Bool(b)))
            }
            Some(Token::Null) => {
                self.lexer.advance();
                Ok(Expr::Literal(Value::Null))
            }
            Some(Token::Dollar) => {
                self.lexer.advance();
                match self.lexer.advance() {
                    Some(Token::Ident(name)) => Ok(Expr::Variable(name)),
                    Some(tok) => Err(ParseError::ExpectedToken {
                        expected: "variable name".to_string(),
                        found: tok.to_string(),
                    }),
                    None => Err(ParseError::UnexpectedEof),
                }
            }
            Some(Token::Ident(_)) => self.parse_call(),
            Some(Token::LBracket) => self.parse_list(),
            Some(Token::LBrace) => self.parse_record(),
            Some(Token::LParen) => {
                self.lexer.advance();
                let expr = self.parse_expr()?;
                self.expect_token(Token::RParen)?;
                Ok(expr)
            }
            Some(tok) => Err(ParseError::UnexpectedToken(tok.to_string())),
            None => Err(ParseError::UnexpectedEof),
        }
    }
    
    fn parse_call(&mut self) -> ParseResult<Expr> {
        let name = match self.lexer.advance() {
            Some(Token::Ident(s)) => s,
            Some(tok) => return Err(ParseError::ExpectedToken {
                expected: "command name".to_string(),
                found: tok.to_string(),
            }),
            None => return Err(ParseError::UnexpectedEof),
        };
        
        let mut args = Vec::new();
        let mut flags = HashMap::new();
        
        loop {
            match self.lexer.peek() {
                Some(Token::Pipe) | Some(Token::Newline) | None => break,
                Some(Token::Semicolon) | Some(Token::RBrace) | Some(Token::RParen) => break,
                Some(Token::DoubleDash) => {
                    self.lexer.advance();
                    // 后面的都是参数
                    while let Some(token) = self.lexer.peek() {
                        match token {
                            Token::Pipe | Token::Newline | Token::Semicolon => break,
                            _ => {
                                args.push(self.parse_primary()?);
                            }
                        }
                    }
                    break;
                }
                Some(Token::Minus) => {
                    // 可能是 flag 或负数
                    self.lexer.advance();
                    match self.lexer.peek() {
                        Some(Token::Minus) => {
                            // --flag
                            self.lexer.advance();
                            match self.lexer.advance() {
                                Some(Token::Ident(flag_name)) => {
                                    // 检查是否有值
                                    if matches!(self.lexer.peek(), Some(Token::Assign)) {
                                        self.lexer.advance();
                                        let value = self.parse_primary()?;
                                        flags.insert(flag_name, value);
                                    } else {
                                        flags.insert(flag_name, Expr::Literal(Value::Bool(true)));
                                    }
                                }
                                Some(tok) => return Err(ParseError::ExpectedToken {
                                    expected: "flag name".to_string(),
                                    found: tok.to_string(),
                                }),
                                None => return Err(ParseError::UnexpectedEof),
                            }
                        }
                        Some(Token::Number(_)) => {
                            // 负数
                            let num_expr = self.parse_primary()?;
                            args.push(Expr::Unary {
                                op: UnaryOp::Neg,
                                expr: Box::new(num_expr),
                            });
                        }
                        Some(Token::Ident(flag_name)) => {
                            // -f 短 flag
                            let flag_name = flag_name.clone();
                            self.lexer.advance();
                            flags.insert(flag_name, Expr::Literal(Value::Bool(true)));
                        }
                        _ => {
                            return Err(ParseError::InvalidSyntax("Invalid flag syntax".to_string()));
                        }
                    }
                }
                _ => {
                    args.push(self.parse_primary()?);
                }
            }
        }
        
        Ok(Expr::Call { name, args, flags })
    }
    
    fn parse_list(&mut self) -> ParseResult<Expr> {
        self.expect_token(Token::LBracket)?;
        
        let mut items = Vec::new();
        
        while !matches!(self.lexer.peek(), Some(Token::RBracket)) {
            items.push(self.parse_expr()?);
            
            if matches!(self.lexer.peek(), Some(Token::Comma)) {
                self.lexer.advance();
            }
        }
        
        self.expect_token(Token::RBracket)?;
        
        Ok(Expr::List(items))
    }
    
    fn parse_record(&mut self) -> ParseResult<Expr> {
        self.expect_token(Token::LBrace)?;
        
        let mut fields = HashMap::new();
        
        while !matches!(self.lexer.peek(), Some(Token::RBrace)) {
            let key = match self.lexer.advance() {
                Some(Token::Ident(s)) => s,
                Some(Token::String(s)) => s,
                Some(tok) => return Err(ParseError::ExpectedToken {
                    expected: "field name".to_string(),
                    found: tok.to_string(),
                }),
                None => return Err(ParseError::UnexpectedEof),
            };
            
            self.expect_token(Token::Colon)?;
            
            let value = self.parse_expr()?;
            
            fields.insert(key, value);
            
            if matches!(self.lexer.peek(), Some(Token::Comma)) {
                self.lexer.advance();
            }
        }
        
        self.expect_token(Token::RBrace)?;
        
        Ok(Expr::Record(fields))
    }
    
    fn expect_token(&mut self, expected: Token) -> ParseResult<()> {
        match self.lexer.peek() {
            Some(token) if std::mem::discriminant(token) == std::mem::discriminant(&expected) => {
                self.lexer.advance();
                Ok(())
            }
            Some(token) => Err(ParseError::ExpectedToken {
                expected: format!("{:?}", expected),
                found: token.to_string(),
            }),
            None => Err(ParseError::UnexpectedEof),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_simple_command() {
        let mut parser = Parser::new("echo hello");
        let stmts = parser.parse().unwrap();
        
        assert_eq!(stmts.len(), 1);
        assert!(matches!(stmts[0], Stmt::Expr(Expr::Call { .. })));
    }
    
    #[test]
    fn test_parse_pipeline() {
        let mut parser = Parser::new("ls | grep test");
        let stmts = parser.parse().unwrap();
        
        assert_eq!(stmts.len(), 1);
        assert!(matches!(stmts[0], Stmt::Expr(Expr::Pipeline { .. })));
    }
    
    #[test]
    fn test_parse_let() {
        let mut parser = Parser::new("let x = 42");
        let stmts = parser.parse().unwrap();
        
        assert_eq!(stmts.len(), 1);
        assert!(matches!(stmts[0], Stmt::Let { .. }));
    }
    
    #[test]
    fn test_parse_binary_expr() {
        let mut parser = Parser::new("echo (1 + 2)");
        let stmts = parser.parse().unwrap();
        
        assert_eq!(stmts.len(), 1);
    }
}
