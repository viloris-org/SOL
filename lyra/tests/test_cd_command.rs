use lyra::parser::Parser;

#[tokio::test]
async fn test_cd_with_argument() {
    let mut parser = Parser::new("cd docs");
    let stmts = parser.parse().unwrap();

    assert_eq!(stmts.len(), 1);

    // Verify it's a command call with one argument
    if let lyra::parser::Stmt::Expr(lyra::parser::Expr::Call { name, args, .. }) = &stmts[0] {
        assert_eq!(name, "cd");
        assert_eq!(args.len(), 1);

        // Verify the argument is "docs"
        if let lyra::parser::Expr::Literal(lyra::parser::Value::String(s)) = &args[0] {
            assert_eq!(s, "docs");
        } else {
            panic!("Expected string literal argument");
        }
    } else {
        panic!("Expected Call expression");
    }
}

#[tokio::test]
async fn test_cd_with_path() {
    let mut parser = Parser::new("cd /home/user/projects");
    let stmts = parser.parse().unwrap();

    assert_eq!(stmts.len(), 1);

    if let lyra::parser::Stmt::Expr(lyra::parser::Expr::Call { name, args, .. }) = &stmts[0] {
        assert_eq!(name, "cd");
        assert_eq!(args.len(), 1);

        if let lyra::parser::Expr::Literal(lyra::parser::Value::String(s)) = &args[0] {
            assert_eq!(s, "/home/user/projects");
        } else {
            panic!("Expected string literal argument");
        }
    } else {
        panic!("Expected Call expression");
    }
}

#[tokio::test]
async fn test_ls_with_argument() {
    let mut parser = Parser::new("ls docs");
    let stmts = parser.parse().unwrap();

    assert_eq!(stmts.len(), 1);

    if let lyra::parser::Stmt::Expr(lyra::parser::Expr::Call { name, args, .. }) = &stmts[0] {
        assert_eq!(name, "ls");
        assert_eq!(args.len(), 1);

        if let lyra::parser::Expr::Literal(lyra::parser::Value::String(s)) = &args[0] {
            assert_eq!(s, "docs");
        } else {
            panic!("Expected string literal argument");
        }
    } else {
        panic!("Expected Call expression");
    }
}
