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
            if matches!(self.lexer.peek(), Some(Token::Newline | Token::Semicolon)) {
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
            // At statement position these names are shell commands. They remain
            // boolean literals inside expressions such as `if true { ... }`.
            Some(Token::Bool(_)) => {
                let first = self.parse_call()?;
                let first = self.parse_and_tail(first)?;
                let first = self.parse_or_tail(first)?;
                let expr = self.finish_pipeline(first)?;
                Ok(Stmt::Expr(expr))
            }
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
            Some(tok) => {
                return Err(ParseError::ExpectedToken {
                    expected: "identifier".to_string(),
                    found: tok.to_string(),
                });
            }
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
            Some(tok) => {
                return Err(ParseError::ExpectedToken {
                    expected: "identifier".to_string(),
                    found: tok.to_string(),
                });
            }
            None => return Err(ParseError::UnexpectedEof),
        };

        self.expect_token(Token::LParen)?;

        let mut params = Vec::new();
        while !matches!(self.lexer.peek(), Some(Token::RParen)) {
            match self.lexer.advance() {
                Some(Token::Ident(s)) => params.push(s),
                Some(tok) => {
                    return Err(ParseError::ExpectedToken {
                        expected: "parameter name".to_string(),
                        found: tok.to_string(),
                    });
                }
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
            if matches!(self.lexer.peek(), Some(Token::Newline | Token::Semicolon)) {
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
            if matches!(self.lexer.peek(), Some(Token::Newline | Token::Semicolon)) {
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
                if matches!(self.lexer.peek(), Some(Token::Newline | Token::Semicolon)) {
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
            Some(tok) => {
                return Err(ParseError::ExpectedToken {
                    expected: "variable name".to_string(),
                    found: tok.to_string(),
                });
            }
            None => return Err(ParseError::UnexpectedEof),
        };

        self.expect_token(Token::In)?;

        let iter = self.parse_expr()?;

        self.expect_token(Token::LBrace)?;

        let mut body = Vec::new();
        while !matches!(self.lexer.peek(), Some(Token::RBrace)) {
            if matches!(self.lexer.peek(), Some(Token::Newline | Token::Semicolon)) {
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
            if matches!(self.lexer.peek(), Some(Token::Newline | Token::Semicolon)) {
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

        if matches!(
            self.lexer.peek(),
            Some(Token::Newline | Token::Semicolon | Token::RBrace) | None
        ) {
            Ok(Stmt::Return(None))
        } else {
            let expr = self.parse_expr()?;
            Ok(Stmt::Return(Some(expr)))
        }
    }

    fn parse_pipeline(&mut self) -> ParseResult<Expr> {
        let first = self.parse_expr()?;
        self.finish_pipeline(first)
    }

    fn finish_pipeline(&mut self, first: Expr) -> ParseResult<Expr> {
        let mut stages = vec![first];

        while matches!(self.lexer.peek(), Some(Token::Pipe)) {
            self.lexer.advance();
            let stage = if matches!(self.lexer.peek(), Some(Token::Bool(_))) {
                self.parse_call()?
            } else {
                self.parse_expr()?
            };
            stages.push(stage);
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
        let left = self.parse_and()?;
        self.parse_or_tail(left)
    }

    fn parse_or_tail(&mut self, mut left: Expr) -> ParseResult<Expr> {
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
        let left = self.parse_comparison()?;
        self.parse_and_tail(left)
    }

    fn parse_and_tail(&mut self, mut left: Expr) -> ParseResult<Expr> {
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
                        Some(tok) => {
                            return Err(ParseError::ExpectedToken {
                                expected: "field name".to_string(),
                                found: tok.to_string(),
                            });
                        }
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
            Some(Token::Ident(_) | Token::Slash | Token::Dot | Token::DotDot) => self.parse_call(),
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
            Some(Token::Bool(value)) => value.to_string(),
            Some(Token::Slash | Token::Dot | Token::DotDot) => {
                // Put the first token back conceptually by using its span and
                // consume the complete adjacent command word.
                let first_index = self.lexer.current_index() - 1;
                let (_, first_span) = self
                    .lexer
                    .token_at(first_index)
                    .ok_or(ParseError::UnexpectedEof)?;
                let mut end = first_span.end;
                while let Some((token, span)) = self.lexer.token_at(self.lexer.current_index()) {
                    if span.start != end
                        || matches!(
                            token,
                            Token::Pipe
                                | Token::And
                                | Token::Or
                                | Token::Newline
                                | Token::Semicolon
                                | Token::RBrace
                                | Token::RParen
                        )
                    {
                        break;
                    }
                    end = span.end;
                    self.lexer.advance();
                }
                self.lexer.source(first_span.start..end).to_string()
            }
            Some(tok) => {
                return Err(ParseError::ExpectedToken {
                    expected: "command name".to_string(),
                    found: tok.to_string(),
                });
            }
            None => return Err(ParseError::UnexpectedEof),
        };

        let mut args = Vec::new();
        let mut flags = HashMap::new();
        let mut argv = Vec::new();
        let mut parse_options = true;

        loop {
            match self.lexer.peek() {
                Some(Token::Pipe | Token::And | Token::Or | Token::Newline) | None => break,
                Some(Token::Semicolon) | Some(Token::RBrace) | Some(Token::RParen) => break,
                _ => {
                    if parse_options {
                        let raw = self.peek_word().ok_or_else(|| {
                            ParseError::InvalidSyntax("Unable to read command argument".to_string())
                        })?;

                        if raw == "--" {
                            self.consume_word();
                            argv.push(Expr::Literal(Value::String(raw)));
                            parse_options = false;
                            continue;
                        }

                        if let Some(option) = raw.strip_prefix("--")
                            && !option.is_empty()
                        {
                            self.consume_word();
                            let (name, value) = Self::parse_long_option(option)?;
                            flags.insert(name, value);
                            argv.push(Expr::Literal(Value::String(raw)));
                            continue;
                        }

                        if raw.starts_with('-') && raw != "-" && raw.parse::<f64>().is_err() {
                            self.consume_word();
                            Self::parse_short_options(&raw, &mut flags)?;
                            argv.push(Expr::Literal(Value::String(raw)));
                            continue;
                        }
                    }

                    let arg = self.parse_arg()?;
                    argv.push(arg.clone());
                    args.push(arg);
                }
            }
        }

        Ok(Expr::Call {
            name,
            args,
            flags,
            argv,
        })
    }

    fn parse_long_option(option: &str) -> ParseResult<(String, Expr)> {
        let (name, value) = option
            .split_once('=')
            .map_or((option, None), |(name, value)| (name, Some(value)));

        if name.is_empty() {
            return Err(ParseError::InvalidSyntax("Empty long option".to_string()));
        }

        Ok((
            name.to_string(),
            value.map_or(Expr::Literal(Value::Bool(true)), Self::option_value_to_expr),
        ))
    }

    fn parse_short_options(raw: &str, flags: &mut HashMap<String, Expr>) -> ParseResult<()> {
        let option = raw
            .strip_prefix('-')
            .ok_or_else(|| ParseError::InvalidSyntax("Invalid short option".to_string()))?;
        let (names, value) = option
            .split_once('=')
            .map_or((option, None), |(names, value)| (names, Some(value)));

        if names.is_empty() {
            return Err(ParseError::InvalidSyntax("Empty short option".to_string()));
        }

        if let Some(value) = value {
            if names.chars().count() != 1 {
                return Err(ParseError::InvalidSyntax(format!(
                    "Option value requires a single short option: {raw}"
                )));
            }
            flags.insert(names.to_string(), Self::option_value_to_expr(value));
        } else {
            for name in names.chars() {
                flags.insert(name.to_string(), Expr::Literal(Value::Bool(true)));
            }
        }

        Ok(())
    }

    fn option_value_to_expr(word: &str) -> Expr {
        if let Ok(number) = word.parse::<f64>() {
            Expr::Literal(Value::Number(number))
        } else if word == "true" {
            Expr::Literal(Value::Bool(true))
        } else if word == "false" {
            Expr::Literal(Value::Bool(false))
        } else if word == "null" {
            Expr::Literal(Value::Null)
        } else {
            Expr::Literal(Value::String(word.to_string()))
        }
    }

    /// Return the exact source text for the current whitespace-delimited word.
    fn peek_word(&self) -> Option<String> {
        let start = self.lexer.peek_span()?.start;
        let mut end = start;
        let mut index = self.lexer.current_index();

        while let Some((token, span)) = self.lexer.token_at(index) {
            if span.start != end && end != start {
                break;
            }
            if matches!(
                token,
                Token::Pipe
                    | Token::And
                    | Token::Or
                    | Token::Newline
                    | Token::Semicolon
                    | Token::RBrace
                    | Token::RParen
            ) {
                break;
            }
            end = span.end;
            index += 1;
        }

        (end > start).then(|| self.lexer.source(start..end).to_string())
    }

    fn consume_word(&mut self) {
        let Some(first) = self.lexer.peek_span() else {
            return;
        };
        let mut end = first.start;

        while let Some(span) = self.lexer.peek_span() {
            if span.start != end && end != first.start {
                break;
            }
            if matches!(
                self.lexer.peek(),
                Some(
                    Token::Pipe
                        | Token::And
                        | Token::Or
                        | Token::Newline
                        | Token::Semicolon
                        | Token::RBrace
                        | Token::RParen
                )
            ) {
                break;
            }
            end = span.end;
            self.lexer.advance();
        }
    }

    // Parse a command argument while preserving the exact unquoted word.
    fn parse_arg(&mut self) -> ParseResult<Expr> {
        match self.lexer.peek() {
            Some(Token::String(s)) => {
                let s = s.clone();
                self.lexer.advance();
                Ok(Expr::Literal(Value::String(s)))
            }
            Some(Token::Dollar) => {
                let raw = self.peek_word().ok_or(ParseError::UnexpectedEof)?;
                if let Some(name) = raw.strip_prefix('$')
                    && !name.is_empty()
                    && name
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                {
                    self.consume_word();
                    Ok(Expr::Variable(name.to_string()))
                } else {
                    self.consume_word();
                    Ok(Expr::Literal(Value::String(raw)))
                }
            }
            Some(Token::LBracket) => self.parse_list(),
            Some(Token::LBrace) => self.parse_record(),
            Some(Token::LParen) => {
                self.lexer.advance();
                let expr = self.parse_expr()?;
                self.expect_token(Token::RParen)?;
                Ok(expr)
            }
            Some(_) => {
                let raw = self.peek_word().ok_or(ParseError::UnexpectedEof)?;
                self.consume_word();
                Ok(Expr::Literal(Value::String(raw)))
            }
            None => Err(ParseError::UnexpectedEof),
        }
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
                Some(tok) => {
                    return Err(ParseError::ExpectedToken {
                        expected: "field name".to_string(),
                        found: tok.to_string(),
                    });
                }
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
