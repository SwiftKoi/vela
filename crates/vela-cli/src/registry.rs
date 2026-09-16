//! The subcommand registry.
//!
//! Adding a command is a registration plus one new file — never an edit to the
//! dispatcher (`CONVENTIONS.md §4.7`). Plugins register here too, which is why the
//! table is a `Vec` behind a struct rather than a fixed array.

use crate::command::Command;
use crate::commands;

/// The set of commands a `vela` invocation can dispatch to.
pub struct Registry {
    commands: Vec<Box<dyn Command>>,
}

impl Registry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    /// Adds a command.
    pub fn register(&mut self, command: Box<dyn Command>) {
        self.commands.push(command);
    }

    /// Finds a command by name.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&dyn Command> {
        self.commands
            .iter()
            .find(|c| c.name() == name)
            .map(AsRef::as_ref)
    }

    /// Every registered command, in registration order.
    pub fn iter(&self) -> impl Iterator<Item = &dyn Command> {
        self.commands.iter().map(AsRef::as_ref)
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

/// The built-in command set.
#[must_use]
pub fn builtin() -> Registry {
    let mut registry = Registry::new();
    registry.register(Box::new(commands::NewProject::at_current_dir()));
    registry.register(Box::new(commands::Analyze::at_current_dir()));
    registry.register(Box::new(commands::Build::at_current_dir()));
    registry.register(Box::new(commands::Check::at_current_dir()));
    registry.register(Box::new(commands::Debug::at_current_dir()));
    registry.register(Box::new(commands::Doc::new()));
    registry.register(Box::new(commands::Format::at_current_dir()));
    registry.register(Box::new(commands::Lsp::at_current_dir()));
    registry.register(Box::new(commands::Migrate::at_current_dir()));
    registry.register(Box::new(commands::Patch::at_current_dir()));
    registry.register(Box::new(commands::Run::at_current_dir()));
    registry.register(Box::new(commands::Test::at_current_dir()));
    registry
}
