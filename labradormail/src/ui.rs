//! UI facade for running the inbox TUI with custom configuration.

use std::io::Result;

use crate::global::dialog_default_bindings;
use crate::global::generic_default_bindings;
use crate::global::Bindings;
use crate::inbox;
use crate::root_window::RootWindowConfig;
use crate::Mailbox;

pub struct Ui {
    config: RootWindowConfig,
    dialog_bindings: Bindings,
    global_bindings: Bindings,
}

impl Ui {
    /// Creates a new UI builder with the given root window configuration.
    pub fn new(config: RootWindowConfig) -> Self {
        Self {
            config,
            dialog_bindings: dialog_default_bindings(),
            global_bindings: generic_default_bindings(),
        }
    }

    /// Overrides the default dialog and global bindings.
    pub fn with_bindings(mut self, dialog_bindings: Bindings, global_bindings: Bindings) -> Self {
        self.dialog_bindings = dialog_bindings;
        self.global_bindings = global_bindings;
        self
    }

    /// Runs the inbox UI with sample mailbox data.
    pub async fn run(self) -> Result<()> {
        inbox::run_with_options(self.config, self.dialog_bindings, self.global_bindings).await
    }

    /// Runs the inbox UI with the provided mailboxes.
    pub async fn run_with_mailboxes(self, mailboxes: Vec<Mailbox>) -> Result<()> {
        inbox::run_with_mailboxes_and_options(
            mailboxes,
            self.config,
            self.dialog_bindings,
            self.global_bindings,
        )
        .await
    }
}
