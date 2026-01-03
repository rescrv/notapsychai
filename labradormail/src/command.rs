//! Command line parser and executor.

use crate::OpCode;

/// Result of parsing a command.
#[derive(Debug, PartialEq, Eq)]
pub enum CommandAction {
    /// Execute an opcode.
    Op(OpCode),
    /// Set a configuration option.
    Set(String, String),
    /// Unknown command.
    Unknown(String),
    /// Empty command.
    Empty,
}

/// Parses a command string into an action.
pub fn parse_command(cmd: &str) -> CommandAction {
    let parts: Vec<&str> = cmd.trim().split_whitespace().collect();
    if parts.is_empty() {
        return CommandAction::Empty;
    }

    match parts[0] {
        "q" | "quit" => CommandAction::Op(OpCode::Quit),
        "h" | "help" | "?" => CommandAction::Op(OpCode::Help),
        "set" => {
            if parts.len() < 2 {
                return CommandAction::Unknown("set requires an argument".to_string());
            }
            // Handle set option=value
            let config_part = parts[1..].join(" ");
            if let Some((key, value)) = config_part.split_once('=') {
                CommandAction::Set(key.trim().to_string(), value.trim().to_string())
            } else {
                // Boolean toggle/set? For now assume bool true if no value?
                // Or just require = for simplicity.
                CommandAction::Set(config_part, "yes".to_string())
            }
        }
        "exec" => {
             if parts.len() < 2 {
                return CommandAction::Unknown("exec requires an opcode name".to_string());
            }
            // TODO: Lookup opcode by name
            CommandAction::Unknown(format!("exec {} not implemented", parts[1]))
        }
        _ => CommandAction::Unknown(parts[0].to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_quit() {
        assert_eq!(parse_command("quit"), CommandAction::Op(OpCode::Quit));
        assert_eq!(parse_command("q"), CommandAction::Op(OpCode::Quit));
    }

    #[test]
    fn parse_set() {
        assert_eq!(
            parse_command("set foo=bar"),
            CommandAction::Set("foo".to_string(), "bar".to_string())
        );
        assert_eq!(
            parse_command("set status_on_top"),
            CommandAction::Set("status_on_top".to_string(), "yes".to_string())
        );
    }
}
