use lyra::Lyra;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("Lyra Shell v0.1.0");
    println!("Type 'exit' to quit\n");
    
    let mut shell = Lyra::new();
    shell.run().await?;
    
    Ok(())
}
