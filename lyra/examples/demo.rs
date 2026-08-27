use lyra::Lyra;

#[tokio::main]
async fn main() {
    println!("=== Lyra Shell - Phase 1 MVP 演示 ===\n");

    let mut shell = Lyra::new();

    // 测试 1: Echo
    println!("测试 1: echo 命令");
    shell.execute("echo Hello from Lyra!").await.ok();
    println!();

    // 测试 2: 变量
    println!("测试 2: 变量");
    shell.execute("let x = 42").await.ok();
    shell.execute("echo $x").await.ok();
    println!();

    // 测试 3: 算术
    println!("测试 3: 算术表达式");
    shell.execute("let y = 10 + 32").await.ok();
    shell.execute("echo $y").await.ok();
    println!();

    // 测试 4: pwd
    println!("测试 4: pwd 命令");
    shell.execute("pwd").await.ok();
    println!();

    // 测试 5: ls
    println!("测试 5: ls 命令（简单列表）");
    shell.execute("ls").await.ok();
    println!();

    // 测试 6: ls --long
    println!("测试 6: ls --long 命令（表格格式）");
    shell.execute("ls --long").await.ok();
    println!();

    // 测试 7: 列表
    println!("测试 7: 列表和循环");
    shell
        .execute("for item in [1, 2, 3] { echo $item }")
        .await
        .ok();
    println!();

    // 测试 8: 条件
    println!("测试 8: if 条件");
    shell.execute("if true { echo \"条件为真\" }").await.ok();
    println!();

    println!("=== 所有测试完成！===");
}
