pub mod builtins;
pub mod completion;
pub mod highlighter;
pub mod history;
pub mod lexer;
pub mod parser;
pub mod prompt;
pub mod runtime;

use anyhow::Result;
use completion::{HorizontalCompleter, LyraCompleter};
use highlighter::LyraHighlighter;
use history::HistoryManager;
use parser::Parser;
use prompt::PromptRenderer;
use reedline::{
    ColumnarMenu, DefaultPrompt, FileBackedHistory, KeyCode, KeyModifiers, MenuBuilder, Reedline,
    ReedlineEvent, ReedlineMenu, Signal,
};
use runtime::Evaluator;

const COMPLETION_COLUMNS: u16 = 4;

pub struct Lyra {
    evaluator: Evaluator,
    prompt: PromptRenderer,
    history: HistoryManager,
}

impl Lyra {
    pub fn new() -> Self {
        Self {
            evaluator: Evaluator::new(),
            prompt: PromptRenderer::new(),
            history: HistoryManager::new().expect("Failed to initialize history"),
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        // Set up history backend
        let history_file = directories::ProjectDirs::from("org", "viloris", "lyra")
            .map(|dirs| dirs.data_dir().join("history.txt"))
            .unwrap_or_else(|| std::path::PathBuf::from(".lyra_history"));

        if let Some(parent) = history_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let history = Box::new(
            FileBackedHistory::with_file(1000, history_file)
                .expect("Failed to create history backend"),
        );

        // Create a columnar menu for completions
        let completion_menu = Box::new(
            ColumnarMenu::default()
                .with_name("completion_menu")
                .with_columns(COMPLETION_COLUMNS)
                .with_column_width(Some(20))
                .with_column_padding(2),
        );

        // Configure keybindings for tab completion
        // Start with default Emacs keybindings, then add Tab
        let mut keybindings = reedline::default_emacs_keybindings();
        keybindings.add_binding(
            KeyModifiers::NONE,
            KeyCode::Tab,
            ReedlineEvent::UntilFound(vec![
                ReedlineEvent::Menu("completion_menu".to_string()),
                ReedlineEvent::MenuNext,
            ]),
        );
        keybindings.add_binding(
            KeyModifiers::SHIFT,
            KeyCode::BackTab,
            ReedlineEvent::MenuPrevious,
        );

        let mut line_editor = Reedline::create()
            .with_completer(Box::new(HorizontalCompleter::new(
                LyraCompleter::new(),
                COMPLETION_COLUMNS,
            )))
            .with_highlighter(Box::new(LyraHighlighter::new()))
            .with_history(history)
            .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
            .with_edit_mode(Box::new(reedline::Emacs::new(keybindings)));

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

                    let result = self.execute(line).await;
                    let exit_status = if result.is_ok() { Some(0) } else { Some(1) };

                    // Add to our internal history manager
                    let _ = self.history.add(line.to_string(), exit_status);

                    match result {
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
        if let Some(invocation) = builtins::external::inspect_external_invocation(input)? {
            // Leading environment assignments are shell syntax even when the
            // command that follows happens to share a Lyra builtin's name.
            // Unknown commands must also retain their original command line:
            // parsing them as Lyra expressions loses common CLI syntax such as
            // quoted arguments, ordered flags, URLs and redirections.
            if invocation.has_environment_assignments
                || (!is_lyra_language_keyword(&invocation.command)
                    && !self.evaluator.has_builtin(&invocation.command))
            {
                let environment = self.evaluator.external_environment();
                builtins::external::execute_external_line(input, &environment).await?;
                return Ok(());
            }
        }

        let mut parser = Parser::new(input);
        let stmts = parser.parse()?;

        let result = self.evaluator.eval_stmts(&stmts).await?;

        // 如果结果是表格，打印表格
        if let parser::Value::Table { columns, rows } = result {
            self.print_table(&columns, &rows);
        }

        Ok(())
    }

    fn print_table(
        &self,
        columns: &[String],
        rows: &[std::collections::HashMap<String, parser::Value>],
    ) {
        use parser::Value;

        if rows.is_empty() {
            return;
        }

        // 计算列宽
        let mut widths: std::collections::HashMap<String, usize> =
            columns.iter().map(|c| (c.clone(), c.len())).collect();

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
                    widths.entry(col.clone()).and_modify(|w| *w = (*w).max(len));
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

fn is_lyra_language_keyword(word: &str) -> bool {
    matches!(word, "let" | "def" | "if" | "for" | "while" | "return")
}

impl Default for Lyra {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn external_cli_keeps_raw_syntax_and_receives_lyra_variables() {
        let path = std::env::temp_dir().join(format!(
            "lyra-routing-{}-{}.txt",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let path_string = path.to_string_lossy();
        let quoted_path = shell_words::quote(&path_string);
        let mut lyra = Lyra::new();

        lyra.execute(r#"let lyra_cli_value = "space value""#)
            .await
            .unwrap();
        lyra.execute(&format!("printf '%s' \"$lyra_cli_value\" > {quoted_path}"))
            .await
            .unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(contents, "space value");
    }
}
