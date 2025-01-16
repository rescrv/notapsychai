use rustyline::completion::{Candidate, Completer};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::{Hint, Hinter, HistoryHinter};
use rustyline::{
    Cmd, ConditionalEventHandler, Context, Event, EventContext, Helper, RepeatCount, Validator,
};

//////////////////////////////////////////// CommandHint ///////////////////////////////////////////

#[derive(Clone, Hash, Debug, PartialEq, Eq)]
pub struct CommandHint {
    pub display: String,
    pub complete_up_to: usize,
}

impl CommandHint {
    pub fn new(text: &str, complete_up_to: &str) -> Self {
        assert!(text.starts_with(complete_up_to));
        Self {
            display: text.to_string(),
            complete_up_to: complete_up_to.len(),
        }
    }

    pub fn suffix(&self, strip_chars: usize) -> Self {
        Self {
            display: self.display[strip_chars..].to_owned(),
            complete_up_to: self.complete_up_to.saturating_sub(strip_chars),
        }
    }
}

impl Candidate for CommandHint {
    fn display(&self) -> &str {
        self.display.as_str()
    }

    fn replacement(&self) -> &str {
        self.display.as_str()
    }
}

impl Hint for CommandHint {
    fn display(&self) -> &str {
        &self.display
    }

    fn completion(&self) -> Option<&str> {
        if self.complete_up_to > 0 {
            Some(&self.display[..self.complete_up_to])
        } else {
            None
        }
    }
}

//////////////////////////////////////////// ShellHelper ///////////////////////////////////////////

#[derive(Helper, Validator)]
pub struct ShellHelper {
    pub commands: Vec<CommandHint>,
    #[rustyline(Hinter)]
    pub hinter: HistoryHinter,
    pub hints: Vec<CommandHint>,
}

impl Completer for ShellHelper {
    type Candidate = CommandHint;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _: &Context<'_>,
    ) -> Result<(usize, Vec<CommandHint>), ReadlineError> {
        let candidates = self
            .commands
            .iter()
            .filter_map(|hint| {
                if hint.display.starts_with(&line[..pos]) {
                    Some(hint.suffix(pos))
                } else {
                    None
                }
            })
            .collect();
        Ok((pos, candidates))
    }
}

impl Highlighter for ShellHelper {}

impl Hinter for ShellHelper {
    type Hint = CommandHint;

    fn hint(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Option<CommandHint> {
        if line.is_empty() || pos < line.len() {
            return None;
        }

        if let Some(hint) = self.hinter.hint(line, pos, ctx) {
            return Some(CommandHint::new(&hint, &hint));
        }

        self.hints
            .iter()
            .filter_map(|hint| {
                if hint.display.starts_with(line) {
                    Some(hint.suffix(pos))
                } else {
                    None
                }
            })
            .next()
    }
}

////////////////////////////////////////// TabEventHandler /////////////////////////////////////////

pub struct TabEventHandler;

impl ConditionalEventHandler for TabEventHandler {
    fn handle(&self, _: &Event, n: RepeatCount, _: bool, ctx: &EventContext) -> Option<Cmd> {
        if ctx.line()[..ctx.pos()]
            .chars()
            .next_back()
            .filter(|c| c.is_whitespace())
            .is_some()
        {
            Some(Cmd::SelfInsert(n, '\t'))
        } else {
            None // default complete
        }
    }
}
