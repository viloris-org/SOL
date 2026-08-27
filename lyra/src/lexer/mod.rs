pub mod token;

pub use token::Token;
use logos::Logos;

pub struct Lexer {
    tokens: Vec<Token>,
    current: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        let mut tokens = Vec::new();
        let mut lex = Token::lexer(input);
        
        while let Some(token) = lex.next() {
            match token {
                Ok(tok) => tokens.push(tok),
                Err(_) => {
                    // 跳过无法识别的字符
                    eprintln!("Lexer error at position {}", lex.span().start);
                }
            }
        }
        
        Self {
            tokens,
            current: 0,
        }
    }
    
    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }
    
    pub fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.current)
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
