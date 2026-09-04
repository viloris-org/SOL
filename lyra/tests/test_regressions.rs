use lyra::parser::{BinaryOp, Expr, Parser, Stmt, Value};
use lyra::runtime::Evaluator;
use lyra::{builtins::BuiltinRegistry, completion::FileCompleter};

fn parse_call(
    input: &str,
) -> (
    Vec<Expr>,
    std::collections::HashMap<String, Expr>,
    Vec<Expr>,
) {
    let mut parser = Parser::new(input);
    let statements = parser.parse().expect("command should parse");
    match statements.into_iter().next().expect("one statement") {
        Stmt::Expr(Expr::Call {
            args, flags, argv, ..
        }) => (args, flags, argv),
        other => panic!("expected command call, got {other:?}"),
    }
}

#[test]
fn preserves_argument_boundaries_and_characters() {
    let (args, _, _) = parse_call("cp source.txt destination.txt");
    assert_eq!(
        args,
        vec![
            Expr::Literal(Value::String("source.txt".to_string())),
            Expr::Literal(Value::String("destination.txt".to_string())),
        ]
    );

    let (args, _, _) = parse_call("rm ~/foo@bar");
    assert_eq!(
        args,
        vec![Expr::Literal(Value::String("~/foo@bar".to_string()))]
    );
}

#[test]
fn parses_builtin_options_and_preserves_external_argv() {
    let (args, flags, argv) = parse_call("tool before --all --count=3 -rv after");
    assert_eq!(args.len(), 2);
    assert_eq!(flags.get("all"), Some(&Expr::Literal(Value::Bool(true))));
    assert_eq!(flags.get("count"), Some(&Expr::Literal(Value::Number(3.0))));
    assert_eq!(flags.get("r"), Some(&Expr::Literal(Value::Bool(true))));
    assert_eq!(flags.get("v"), Some(&Expr::Literal(Value::Bool(true))));
    assert_eq!(argv.len(), 5);
    assert_eq!(argv[0], Expr::Literal(Value::String("before".to_string())));
    assert_eq!(argv[1], Expr::Literal(Value::String("--all".to_string())));
    assert_eq!(argv[4], Expr::Literal(Value::String("after".to_string())));
}

#[test]
fn parses_absolute_and_relative_executable_paths() {
    for (input, expected) in [
        ("/usr/bin/printf hello", "/usr/bin/printf"),
        ("./script argument", "./script"),
    ] {
        let mut parser = Parser::new(input);
        let statements = parser.parse().expect("executable path should parse");
        assert!(matches!(
            statements.first(),
            Some(Stmt::Expr(Expr::Call { name, .. })) if name == expected
        ));
    }

    let mut parser = Parser::new("/usr/bin/true&&echo ok");
    let statements = parser.parse().expect("adjacent operator should parse");
    assert!(matches!(
        statements.first(),
        Some(Stmt::Expr(Expr::Binary {
            op: BinaryOp::And,
            ..
        }))
    ));
}

#[tokio::test]
async fn false_statement_uses_the_builtin() {
    let mut parser = Parser::new("false");
    let statements = parser.parse().expect("false should parse");
    assert_eq!(
        Evaluator::new().eval_stmts(&statements).await.unwrap(),
        Value::Bool(false)
    );
}

#[tokio::test]
async fn logical_operators_short_circuit_side_effects() {
    let path = std::env::temp_dir().join(format!("lyra-short-circuit-{}", std::process::id()));
    let expr = Expr::Binary {
        left: Box::new(Expr::Literal(Value::Bool(false))),
        op: BinaryOp::And,
        right: Box::new(Expr::Call {
            name: "touch".to_string(),
            args: vec![Expr::Literal(Value::String(
                path.to_string_lossy().into_owned(),
            ))],
            flags: Default::default(),
            argv: vec![],
        }),
    };

    let result = Evaluator::new()
        .eval_expr(&expr)
        .await
        .expect("expression should evaluate");
    assert_eq!(result, Value::Bool(false));
    assert!(!path.exists());
}

#[tokio::test]
async fn parsed_command_status_drives_logical_operators() {
    let root = std::env::temp_dir().join(format!("lyra-logical-status-{}", std::process::id()));
    let skipped = root.join("skipped");
    let recovered = root.join("recovered");
    let created_directory = root.join("created-directory");
    let continued = root.join("continued");
    std::fs::create_dir_all(&root).expect("create logical fixture");

    let input = format!(
        "false && touch {}; false || touch {}; mkdir {} && touch {}",
        skipped.display(),
        recovered.display(),
        created_directory.display(),
        continued.display()
    );
    let mut parser = Parser::new(&input);
    let statements = parser.parse().expect("logical commands should parse");
    Evaluator::new()
        .eval_stmts(&statements)
        .await
        .expect("logical commands should run");

    assert!(!skipped.exists());
    assert!(recovered.exists());
    assert!(created_directory.is_dir());
    assert!(continued.exists());
    std::fs::remove_dir_all(&root).expect("remove logical fixture");
}

#[tokio::test]
async fn failed_loop_restores_the_outer_scope() {
    let mut evaluator = Evaluator::new();
    let statement = Stmt::For {
        var: "loop_item".to_string(),
        iter: Expr::List(vec![Expr::Literal(Value::Number(1.0))]),
        body: vec![Stmt::Expr(Expr::Call {
            name: "definitely_missing_lyra_command".to_string(),
            args: vec![],
            flags: Default::default(),
            argv: vec![],
        })],
    };

    assert!(evaluator.eval_stmt(&statement).await.is_err());
    assert!(
        evaluator
            .eval_expr(&Expr::Variable("loop_item".to_string()))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn pipelines_transfer_builtin_and_external_output() {
    let mut parser = Parser::new("echo hello | grep ell");
    let statements = parser.parse().expect("pipeline should parse");
    let result = Evaluator::new()
        .eval_stmts(&statements)
        .await
        .expect("builtin pipeline should run");
    assert_eq!(result, Value::String("hello\n".to_string()));

    let mut parser = Parser::new("printf -- hello | wc -c");
    let statements = parser.parse().expect("pipeline should parse");
    let result = Evaluator::new()
        .eval_stmts(&statements)
        .await
        .expect("external pipeline should run");
    assert_eq!(result, Value::String("       5 stdin\n".to_string()));

    let mut parser = Parser::new("basename path/to/file.txt | grep file");
    let statements = parser
        .parse()
        .expect("system builtin pipeline should parse");
    let result = Evaluator::new()
        .eval_stmts(&statements)
        .await
        .expect("system builtin pipeline should run");
    assert_eq!(result, Value::String("file.txt\n".to_string()));
}

#[test]
fn nested_path_completion_keeps_the_prefix() {
    let root = std::env::temp_dir().join(format!("lyra-completion-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create completion fixture");
    std::fs::write(root.join("file.txt"), "test").expect("write completion fixture");

    let line = format!("cat {}/fi", root.display());
    let suggestions = FileCompleter::new().complete(&line, line.len());
    let expected = format!("{}/file.txt", root.display());
    let found = suggestions
        .iter()
        .any(|suggestion| suggestion.value == expected);
    std::fs::remove_dir_all(&root).expect("remove completion fixture");
    assert!(found, "completion should preserve the absolute path prefix");
}

#[tokio::test]
async fn recursive_copy_rejects_a_destination_inside_the_source() {
    let root = std::env::temp_dir().join(format!("lyra-copy-guard-{}", std::process::id()));
    let source = root.join("source");
    let destination = source.join("nested");
    std::fs::create_dir_all(&source).expect("create copy fixture");
    std::fs::write(source.join("file.txt"), "test").expect("write copy fixture");

    let mut flags = std::collections::HashMap::new();
    flags.insert("r".to_string(), Value::Bool(true));
    let result = BuiltinRegistry::new()
        .execute(
            "cp",
            vec![
                Value::String(source.to_string_lossy().into_owned()),
                Value::String(destination.to_string_lossy().into_owned()),
            ],
            flags,
            vec![],
        )
        .await;
    let destination_created = destination.exists();
    std::fs::remove_dir_all(&root).expect("remove copy fixture");

    assert!(result.is_err());
    assert!(!destination_created);
}

#[tokio::test]
async fn recursive_copy_accepts_a_safe_relative_destination() {
    let root = std::path::PathBuf::from("target").join(format!(
        "lyra-relative-copy-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let source = root.join("source");
    let destination = root.join("destination");
    std::fs::create_dir_all(&source).expect("create relative copy fixture");
    std::fs::write(source.join("file.txt"), "test").expect("write relative copy fixture");

    let mut flags = std::collections::HashMap::new();
    flags.insert("r".to_string(), Value::Bool(true));
    BuiltinRegistry::new()
        .execute(
            "cp",
            vec![
                Value::String(source.to_string_lossy().into_owned()),
                Value::String(destination.to_string_lossy().into_owned()),
            ],
            flags,
            vec![],
        )
        .await
        .expect("safe relative copy should succeed");

    assert_eq!(
        std::fs::read_to_string(destination.join("file.txt")).unwrap(),
        "test"
    );
    std::fs::remove_dir_all(root).expect("remove relative copy fixture");
}
