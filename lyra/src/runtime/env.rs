use crate::parser::Value;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Environment {
    scopes: Vec<HashMap<String, Value>>,
}

impl Environment {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(value) = scope.get(name) {
                return Some(value.clone());
            }
        }
        None
    }

    pub fn set(&mut self, name: String, value: Value) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, value);
        }
    }

    pub fn define(&mut self, name: String, value: Value) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, value);
        }
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Return scalar Lyra variables in a form suitable for a child process.
    /// Inner scopes overwrite outer ones, matching normal variable lookup.
    pub fn process_variables(&self) -> HashMap<String, String> {
        let mut variables = HashMap::new();

        for scope in &self.scopes {
            for (name, value) in scope {
                let value = match value {
                    Value::String(value) => value.clone(),
                    Value::Number(value) => value.to_string(),
                    Value::Bool(value) => value.to_string(),
                    Value::Null | Value::List(_) | Value::Record(_) | Value::Table { .. } => {
                        continue;
                    }
                };
                variables.insert(name.clone(), value);
            }
        }

        variables
    }
}

impl Default for Environment {
    fn default() -> Self {
        Self::new()
    }
}
