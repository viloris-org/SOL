use std::env;

pub struct PromptRenderer {
    show_git: bool,
}

impl PromptRenderer {
    pub fn new() -> Self {
        Self { show_git: true }
    }

    pub fn render(&self) -> String {
        let mut parts = Vec::new();

        // 符号
        let symbol = "λ";
        parts.push(symbol.to_string());

        // 当前目录
        if let Ok(cwd) = env::current_dir() {
            let home = env::var("HOME").ok();
            let path = if let Some(ref home_path) = home {
                cwd.to_string_lossy().replace(home_path, "~")
            } else {
                cwd.to_string_lossy().to_string()
            };
            parts.push(path);
        }

        // Git 分支 (简化版本，后续可以增强)
        if self.show_git
            && let Some(branch) = self.detect_git_branch()
        {
            parts.push(format!("({})", branch));
        }

        format!("{} ", parts.join(" "))
    }

    fn detect_git_branch(&self) -> Option<String> {
        // 简单的 git 分支检测
        use std::process::Command;

        let output = Command::new("git")
            .args(["branch", "--show-current"])
            .output()
            .ok()?;

        if output.status.success() {
            let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !branch.is_empty() {
                return Some(branch);
            }
        }

        None
    }
}

impl Default for PromptRenderer {
    fn default() -> Self {
        Self::new()
    }
}
