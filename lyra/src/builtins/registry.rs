use crate::parser::Value;
use crate::runtime::RuntimeResult;
use async_trait::async_trait;
use std::collections::HashMap;

#[async_trait]
pub trait Builtin: Send + Sync {
    fn name(&self) -> &str;
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

        // 注册内建命令
        registry.register(Box::new(crate::builtins::Echo));
        registry.register(Box::new(crate::builtins::Ls));
        registry.register(Box::new(crate::builtins::Cd));
        registry.register(Box::new(crate::builtins::Pwd));
        registry.register(Box::new(crate::builtins::Exit));

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
}

impl Default for BuiltinRegistry {
    fn default() -> Self {
        Self::new()
    }
}
