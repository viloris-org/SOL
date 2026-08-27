use lyra::Lyra;

#[tokio::main]
async fn main() {
    println!("=== Lyra Shell - 外部命令测试 ===\n");

    let mut shell = Lyra::new();

    // 测试外部命令
    println!("测试 1: 运行 uname -s");
    shell.execute("uname -s").await.ok();
    println!();

    println!("测试 2: 运行 whoami");
    shell.execute("whoami").await.ok();
    println!();

    println!("测试 3: 运行 date");
    shell.execute("date").await.ok();
    println!();

    println!("Test 4: Combine builtin commands and arithmetic");
    shell.execute("let result = 100 - 58").await.ok();
    shell.execute("echo Result: $result").await.ok();
    println!();

    println!("=== 外部命令测试完成！===");
}
