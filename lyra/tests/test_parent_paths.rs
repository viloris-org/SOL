use lyra::parser::Parser;

#[test]
fn test_parse_ls_with_parent_directory() {
    // Test parsing "ls .."
    let input = "ls ..";
    let mut parser = Parser::new(input);
    let ast = parser.parse();
    assert!(ast.is_ok(), "Failed to parse 'ls ..': {:?}", ast.err());
}

#[test]
fn test_parse_cd_to_parent_directory() {
    // Test parsing "cd .."
    let input = "cd ..";
    let mut parser = Parser::new(input);
    let ast = parser.parse();
    assert!(ast.is_ok(), "Failed to parse 'cd ..': {:?}", ast.err());
}

#[test]
fn test_parse_cat_relative_paths() {
    // Test "cat ../file.txt" and "cat ./file.txt"
    let input1 = "cat ../parent.txt";
    let mut parser1 = Parser::new(input1);
    let ast1 = parser1.parse();
    assert!(
        ast1.is_ok(),
        "Failed to parse 'cat ../parent.txt': {:?}",
        ast1.err()
    );

    let input2 = "cat ./current.txt";
    let mut parser2 = Parser::new(input2);
    let ast2 = parser2.parse();
    assert!(
        ast2.is_ok(),
        "Failed to parse 'cat ./current.txt': {:?}",
        ast2.err()
    );

    let input3 = "cat ../../another.txt";
    let mut parser3 = Parser::new(input3);
    let ast3 = parser3.parse();
    assert!(
        ast3.is_ok(),
        "Failed to parse 'cat ../../another.txt': {:?}",
        ast3.err()
    );
}

#[test]
fn test_parse_paths_with_dotdot() {
    // Test various commands with ..
    let tests = vec![
        "ls ..",
        "cd ..",
        "cat ../file.txt",
        "cp ../src.txt ../dst.txt",
        "mv ../old.txt ./new.txt",
        "rm ../temp.txt",
        "mkdir ../newdir",
        "touch ../newfile.txt",
    ];

    for input in tests {
        let mut parser = Parser::new(input);
        let ast = parser.parse();
        assert!(ast.is_ok(), "Failed to parse '{}': {:?}", input, ast.err());
    }
}
