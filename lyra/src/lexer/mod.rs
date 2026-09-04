pub mod token;

use logos::Logos;
use std::ops::Range;
pub use token::Token;

pub struct Lexer {
    input: String,
    tokens: Vec<Token>,
    spans: Vec<Range<usize>>,
    current: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        let mut tokens = Vec::new();
        let mut spans = Vec::new();
        let mut lex = Token::lexer(input);

        while let Some(token) = lex.next() {
            match token {
                Ok(tok) => {
                    tokens.push(tok);
                    spans.push(lex.span());
                }
                Err(_) => unreachable!("the Unknown token must cover non-whitespace input"),
            }
        }

        Self {
            input: input.to_string(),
            tokens,
            spans,
            current: 0,
        }
    }

    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    pub fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.current)
    }

    pub fn peek_span(&self) -> Option<Range<usize>> {
        self.spans.get(self.current).cloned()
    }

    pub fn source(&self, span: Range<usize>) -> &str {
        &self.input[span]
    }

    pub fn current_index(&self) -> usize {
        self.current
    }

    pub fn token_at(&self, index: usize) -> Option<(&Token, Range<usize>)> {
        Some((self.tokens.get(index)?, self.spans.get(index)?.clone()))
    }

    pub fn advance(&mut self) -> Option<Token> {
        if self.current < self.tokens.len() {
            let token = self.tokens[self.current].clone();
            self.current += 1;
            Some(token)
        } else {
            None
        }
    }

    pub fn is_at_end(&self) -> bool {
        self.current >= self.tokens.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_simple_command() {
        let lexer = Lexer::new("echo hello");
        let tokens = lexer.tokens();

        assert_eq!(tokens.len(), 2);
        assert!(matches!(tokens[0], Token::Ident(_)));
        assert!(matches!(tokens[1], Token::Ident(_)));
    }

    #[test]
    fn test_tokenize_pipeline() {
        let lexer = Lexer::new("ls | grep test");
        let tokens = lexer.tokens();

        assert!(tokens.contains(&Token::Pipe));
    }

    #[test]
    fn test_tokenize_string() {
        let lexer = Lexer::new(r#"echo "hello world""#);
        let tokens = lexer.tokens();

        assert!(matches!(tokens[1], Token::String(_)));
    }

    #[test]
    fn test_tokenize_number() {
        let lexer = Lexer::new("42 3.14 -5");
        let tokens = lexer.tokens();

        assert_eq!(tokens.len(), 3);
        assert!(matches!(tokens[0], Token::Number(_)));
        assert!(matches!(tokens[1], Token::Number(_)));
        assert!(matches!(tokens[2], Token::Number(_)));
    }
}
