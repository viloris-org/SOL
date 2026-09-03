use crate::parser::Value;
use crate::runtime::RuntimeResult;
use async_trait::async_trait;
use std::collections::HashMap;

#[async_trait]
pub trait Builtin: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str {
        "No description available"
    }
    async fn execute(
        &self,
        args: Vec<Value>,
        flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value>;
}

pub struct BuiltinRegistry {
    commands: HashMap<String, Box<dyn Builtin>>,
}

impl BuiltinRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            commands: HashMap::new(),
        };

        // Basic commands
        registry.register(Box::new(crate::builtins::Echo));
        registry.register(Box::new(crate::builtins::Ls));
        registry.register(Box::new(crate::builtins::Cd));
        registry.register(Box::new(crate::builtins::Pwd));
        registry.register(Box::new(crate::builtins::Exit));
        registry.register(Box::new(crate::builtins::Which));
        registry.register(Box::new(crate::builtins::Clear));
        registry.register(Box::new(crate::builtins::Reset));
        registry.register(Box::new(crate::builtins::Help));

        // File operations
        registry.register(Box::new(crate::builtins::Cat));
        registry.register(Box::new(crate::builtins::Cp));
        registry.register(Box::new(crate::builtins::Mv));
        registry.register(Box::new(crate::builtins::Rm));
        registry.register(Box::new(crate::builtins::Mkdir));
        registry.register(Box::new(crate::builtins::Touch));

        // Text utilities
        registry.register(Box::new(crate::builtins::Grep));
        registry.register(Box::new(crate::builtins::Head));
        registry.register(Box::new(crate::builtins::Tail));
        registry.register(Box::new(crate::builtins::Wc));
        registry.register(Box::new(crate::builtins::Sort));
        registry.register(Box::new(crate::builtins::Uniq));

        // System utilities
        registry.register(Box::new(crate::builtins::Env));
        registry.register(Box::new(crate::builtins::Basename));
        registry.register(Box::new(crate::builtins::Dirname));
        registry.register(Box::new(crate::builtins::Sleep));
        registry.register(Box::new(crate::builtins::Date));
        registry.register(Box::new(crate::builtins::True));
        registry.register(Box::new(crate::builtins::False));
        registry.register(Box::new(crate::builtins::Whoami));
        registry.register(Box::new(crate::builtins::Uname));

        registry
    }

    pub fn register(&mut self, builtin: Box<dyn Builtin>) {
        self.commands.insert(builtin.name().to_string(), builtin);
    }

    pub async fn execute(
        &self,
        name: &str,
        args: Vec<Value>,
        flags: HashMap<String, Value>,
    ) -> RuntimeResult<Value> {
        if let Some(cmd) = self.commands.get(name) {
            cmd.execute(args, flags).await
        } else {
            // 尝试执行外部命令
            crate::builtins::external::execute_external(name, args, flags).await
        }
    }

    pub fn has_command(&self, name: &str) -> bool {
        self.commands.contains_key(name)
    }

    pub fn command_names(&self) -> Vec<&str> {
        self.commands.keys().map(|s| s.as_str()).collect()
    }

    pub fn get_command(&self, name: &str) -> Option<&dyn Builtin> {
        self.commands.get(name).map(|b| b.as_ref())
    }

    pub fn all_commands(&self) -> Vec<(&str, &str)> {
        self.commands
            .values()
            .map(|cmd| (cmd.name(), cmd.description()))
            .collect()
    }
}

impl Default for BuiltinRegistry {
    fn default() -> Self {
        Self::new()
    }
}
