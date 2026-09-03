use lyra::parser::Parser;

#[tokio::test]
async fn test_which_command_parsing() {
    let mut parser = Parser::new("which cargo");
    let stmts = parser.parse().unwrap();

    assert_eq!(stmts.len(), 1);

    if let lyra::parser::Stmt::Expr(lyra::parser::Expr::Call { name, args, .. }) = &stmts[0] {
        assert_eq!(name, "which");
        assert_eq!(args.len(), 1);

        if let lyra::parser::Expr::Literal(lyra::parser::Value::String(s)) = &args[0] {
            assert_eq!(s, "cargo");
        } else {
            panic!("Expected string literal argument");
        }
    } else {
        panic!("Expected Call expression");
    }
}

#[tokio::test]
async fn test_clear_command_parsing() {
    let mut parser = Parser::new("clear");
    let stmts = parser.parse().unwrap();

    assert_eq!(stmts.len(), 1);

    if let lyra::parser::Stmt::Expr(lyra::parser::Expr::Call { name, args, .. }) = &stmts[0] {
        assert_eq!(name, "clear");
        assert_eq!(args.len(), 0);
    } else {
        panic!("Expected Call expression");
    }
}

#[tokio::test]
async fn test_reset_command_parsing() {
    let mut parser = Parser::new("reset");
    let stmts = parser.parse().unwrap();

    assert_eq!(stmts.len(), 1);

    if let lyra::parser::Stmt::Expr(lyra::parser::Expr::Call { name, args, .. }) = &stmts[0] {
        assert_eq!(name, "reset");
        assert_eq!(args.len(), 0);
    } else {
        panic!("Expected Call expression");
    }
}
