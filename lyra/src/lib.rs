pub mod lexer;
pub mod parser;
pub mod runtime;
pub mod builtins;
pub mod prompt;

use parser::Parser;
use runtime::Evaluator;
use prompt::PromptRenderer;
use reedline::{DefaultPrompt, Reedline, Signal};
use anyhow::Result;

pub struct Lyra {
    evaluator: Evaluator,
    prompt: PromptRenderer,
}

impl Lyra {
    pub fn new() -> Self {
        Self {
            evaluator: Evaluator::new(),
            prompt: PromptRenderer::new(),
        }
    }
    
    pub async fn run(&mut self) -> Result<()> {
        let mut line_editor = Reedline::create();
        
        loop {
            let prompt_text = self.prompt.render();
            let prompt = DefaultPrompt::new(
                reedline::DefaultPromptSegment::Basic(prompt_text),
                reedline::DefaultPromptSegment::Empty,
            );
            
            let sig = line_editor.read_line(&prompt)?;
            
            match sig {
                Signal::Success(buffer) => {
                    let line = buffer.trim();
                    
                    if line.is_empty() {
                        continue;
                    }
                    
                    match self.execute(line).await {
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("Error: {}", e);
                        }
                    }
                }
                Signal::CtrlD | Signal::CtrlC => {
                    println!();
                    break;
                }
            }
        }
        
        Ok(())
    }
    
    pub async fn execute(&mut self, input: &str) -> Result<()> {
        let mut parser = Parser::new(input);
        let stmts = parser.parse()?;
        
        let result = self.evaluator.eval_stmts(&stmts).await?;
        
        // 如果结果是表格，打印表格
        if let parser::Value::Table { columns, rows } = result {
            self.print_table(&columns, &rows);
        }
        
        Ok(())
    }
    
    fn print_table(&self, columns: &[String], rows: &[std::collections::HashMap<String, parser::Value>]) {
        use parser::Value;
        
        if rows.is_empty() {
            return;
        }
        
        // 计算列宽
        let mut widths: std::collections::HashMap<String, usize> = columns
            .iter()
            .map(|c| (c.clone(), c.len()))
            .collect();
        
        for row in rows {
            for col in columns {
                if let Some(val) = row.get(col) {
                    let len = match val {
                        Value::String(s) => s.len(),
                        Value::Number(n) => n.to_string().len(),
                        Value::Bool(b) => b.to_string().len(),
                        Value::Null => 4, // "null"
                        _ => 10,
                    };
                    widths.entry(col.clone())
                        .and_modify(|w| *w = (*w).max(len));
                }
            }
        }
        
        // 打印表头
        print!("│");
        for col in columns {
            let width = widths.get(col).unwrap_or(&10);
            print!(" {:width$} │", col, width = width);
        }
        println!();
        
        // 打印分隔线
        print!("├");
        for col in columns {
            let width = widths.get(col).unwrap_or(&10);
            print!("─{}─┼", "─".repeat(*width));
        }
        println!("\x08┤");
        
        // 打印数据行
        for row in rows {
            print!("│");
            for col in columns {
                let width = widths.get(col).unwrap_or(&10);
                let val_str = match row.get(col) {
                    Some(Value::String(s)) => s.clone(),
                    Some(Value::Number(n)) => n.to_string(),
                    Some(Value::Bool(b)) => b.to_string(),
                    Some(Value::Null) => "null".to_string(),
                    Some(_) => "...".to_string(),
                    None => "".to_string(),
                };
                print!(" {:width$} │", val_str, width = width);
            }
            println!();
        }
    }
}

impl Default for Lyra {
    fn default() -> Self {
        Self::new()
    }
}
