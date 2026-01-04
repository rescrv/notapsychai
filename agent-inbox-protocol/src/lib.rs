use chrono::{DateTime, Utc};

#[cfg(feature = "client")]
mod client;
#[cfg(feature = "client")]
pub use client::Client;
#[cfg(feature = "server")]
mod server;
#[cfg(feature = "server")]
pub use server::router;

macro_rules! typed_string {
    ($name:ident) => {
        impl $name {
            /// Creates a new instance if the input is valid.
            pub fn new(s: impl Into<String>) -> Option<Self> {
                let s = s.into();
                Self::validate(&s)?;
                Some(Self(s))
            }

            /// Returns the inner string as a slice.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

///////////////////////////////////////////// MessageId /////////////////////////////////////////////

#[derive(
    Clone,
    Debug,
    Default,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    serde::Deserialize,
    serde::Serialize,
)]
pub struct MessageId(String);
typed_string!(MessageId);

impl MessageId {
    pub fn validate(_: &str) -> Option<()> {
        Some(())
    }
}

//////////////////////////////////////////// MailboxName ///////////////////////////////////////////

#[derive(
    Clone,
    Debug,
    Default,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    serde::Deserialize,
    serde::Serialize,
)]
pub struct MailboxName(String);
typed_string!(MailboxName);

impl MailboxName {
    pub fn validate(_: &str) -> Option<()> {
        Some(())
    }
}

/////////////////////////////////////////////// From ///////////////////////////////////////////////

#[derive(
    Clone,
    Debug,
    Default,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    serde::Deserialize,
    serde::Serialize,
)]
pub struct From(String);
typed_string!(From);

impl From {
    pub fn validate(_: &str) -> Option<()> {
        Some(())
    }
}

/////////////////////////////////////////////// Body ///////////////////////////////////////////////

#[derive(
    Clone,
    Debug,
    Default,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    serde::Deserialize,
    serde::Serialize,
)]
pub struct Body(String);
typed_string!(Body);

impl Body {
    pub fn validate(_: &str) -> Option<()> {
        Some(())
    }
}

/////////////////////////////////////////////// Verb ///////////////////////////////////////////////

/// A verb representing an action to perform.
#[derive(
    Clone,
    Debug,
    Default,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    serde::Deserialize,
    serde::Serialize,
)]
pub struct Verb(String);
typed_string!(Verb);

impl Verb {
    pub fn validate(_: &str) -> Option<()> {
        Some(())
    }
}

/////////////////////////////////////////////// Action //////////////////////////////////////////////

/// Allowed shortcut keys for actions.
///
/// These keys are chosen to avoid conflicts with standard navigation bindings (j/k/J/K for
/// movement, q for quit, ? for help, / for search, etc.) while providing intuitive shortcuts
/// for common actions.
pub const ALLOWED_SHORTCUTS: &[char] = &[
    'a', // archive
    'b', // back/bounce
    'c', // compose/create
    'd', // done/delete
    'e', // edit
    'f', // forward/flag
    'g', // go
    'h', // (reserved for help in some contexts, but allowed here)
    'i', // inbox/important
    'l', // label
    'o', // open
    'p', // print/pin
    'r', // reply
    's', // star/spam/snooze
    'u', // unread/undo
    'v', // view
    'w', // write
    'x', // delete/archive (common in some UIs)
    'y', // yes/confirm
    'A', // Archive (shifted variant)
    'B', // Bounce (shifted variant)
    'C', // Compose (shifted variant)
    'D', // Defer/Delete (shifted variant)
    'E', // Edit (shifted variant)
    'F', // Forward (shifted variant)
    'G', // Go (shifted variant)
    'I', // Important (shifted variant)
    'O', // Open (shifted variant)
    'P', // Print (shifted variant)
    'R', // Reply-all (shifted variant)
    'S', // Send (shifted variant)
    'U', // Unsubscribe (shifted variant)
    'W', // Write (shifted variant)
    'X', // Delete (shifted variant)
    'Y', // Yes (shifted variant)
];

/// Returns true if the given shortcut is in the allow-list.
pub fn is_allowed_shortcut(shortcut: &str) -> bool {
    if shortcut.len() != 1 {
        return false;
    }
    let ch = shortcut.chars().next().unwrap();
    ALLOWED_SHORTCUTS.contains(&ch)
}

/// An action that can be performed on a message.
#[derive(Clone, Debug, Eq, PartialEq, Hash, serde::Deserialize, serde::Serialize)]
pub struct Action {
    /// The verb identifying this action.
    pub verb: Verb,
    /// Human-readable label for display (e.g., "Mark Done", "Defer to Tomorrow").
    pub label: String,
    /// Optional keyboard shortcut hint (e.g., "d" for done).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
}

impl Action {
    /// Creates a new action.
    pub fn new(verb: Verb, label: impl Into<String>) -> Self {
        Self {
            verb,
            label: label.into(),
            shortcut: None,
        }
    }

    /// Creates a new action with a shortcut.
    ///
    /// The shortcut is validated against the allow-list; invalid shortcuts are ignored.
    pub fn with_shortcut(mut self, shortcut: impl Into<String>) -> Self {
        let s = shortcut.into();
        if is_allowed_shortcut(&s) {
            self.shortcut = Some(s);
        }
        self
    }

    /// Returns a copy of this action with the shortcut sanitized.
    ///
    /// If the shortcut is not in the allow-list, it is removed.
    pub fn sanitized(mut self) -> Self {
        if let Some(ref s) = self.shortcut
            && !is_allowed_shortcut(s)
        {
            self.shortcut = None;
        }
        self
    }
}

////////////////////////////////////////////// Message /////////////////////////////////////////////

#[derive(Clone, Debug, Default, Eq, PartialEq, Hash, serde::Deserialize, serde::Serialize)]
pub struct Message {
    pub msg_id: MessageId,
    pub date: DateTime<Utc>,
    pub from: From,
    pub body: Body,
    pub wrap: bool,
    pub actions: Vec<Action>,
}

////////////////////////////////////////////// Mailbox /////////////////////////////////////////////

#[derive(Clone, Debug, Default, Eq, PartialEq, Hash, serde::Deserialize, serde::Serialize)]
pub struct Mailbox {
    pub name: MailboxName,
    pub messages: Vec<Message>,
}

////////////////////////////////////////// QueryParameters /////////////////////////////////////////

#[derive(Clone, Debug, Default, Eq, PartialEq, Hash, serde::Deserialize, serde::Serialize)]
pub struct QueryParameters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_keywords: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailbox: Option<MailboxName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailboxes: Option<Vec<MailboxName>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<From>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_per_inbox: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_across_inboxes: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<SortOrder>,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, serde::Deserialize, serde::Serialize)]
pub enum SortOrder {
    DateAsc,
    DateDesc,
    Relevance,
}

/////////////////////////////////////////// QueryResults ///////////////////////////////////////////

#[derive(Clone, Debug, Default, Eq, PartialEq, Hash, serde::Deserialize, serde::Serialize)]
pub struct QueryResult {
    mailboxes: Vec<Mailbox>,
}

impl QueryResult {
    /// Creates a new query result from a list of mailboxes.
    pub fn new(mailboxes: Vec<Mailbox>) -> Self {
        Self { mailboxes }
    }

    /// Returns the mailboxes in this query result.
    pub fn mailboxes(&self) -> &[Mailbox] {
        &self.mailboxes
    }

    /// Consumes the query result and returns the mailboxes.
    pub fn into_mailboxes(self) -> Vec<Mailbox> {
        self.mailboxes
    }

    /// Returns a sanitized copy of this query result.
    ///
    /// Removes any action shortcuts that are not in the allow-list.
    pub fn sanitized(self) -> Self {
        let mailboxes = self
            .mailboxes
            .into_iter()
            .map(|mut mailbox| {
                for message in &mut mailbox.messages {
                    message.actions = message
                        .actions
                        .drain(..)
                        .map(|action| action.sanitized())
                        .collect();
                }
                mailbox
            })
            .collect();
        Self { mailboxes }
    }
}

////////////////////////////////////////// MailboxProvider /////////////////////////////////////////

#[async_trait::async_trait]
pub trait MailboxProvider {
    type Error: std::error::Error;
    async fn query(&mut self, query: QueryParameters) -> Result<QueryResult, Self::Error>;
    async fn action(&mut self, request: ActionRequest) -> Result<ActionResponse, Self::Error>;
}

////////////////////////////////////////// ActionRequest ///////////////////////////////////////////

/// Request to perform an action on a message.
#[derive(Clone, Debug, Eq, PartialEq, Hash, serde::Deserialize, serde::Serialize)]
pub struct ActionRequest {
    /// The message ID to act upon.
    pub message_id: MessageId,
    /// The verb identifying the action to perform.
    pub verb: Verb,
}

////////////////////////////////////////// ActionResponse //////////////////////////////////////////

/// Response from an action request.
#[derive(Clone, Debug, Eq, PartialEq, Hash, serde::Deserialize, serde::Serialize)]
pub struct ActionResponse {
    /// Whether the action succeeded.
    pub success: bool,
    /// Optional message describing the result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl ActionResponse {
    /// Creates a successful response.
    pub fn success() -> Self {
        Self {
            success: true,
            message: None,
        }
    }

    /// Creates a successful response with a message.
    pub fn success_with_message(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: Some(message.into()),
        }
    }

    /// Creates a failure response.
    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: Some(message.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_allowed_shortcut_accepts_valid_lowercase() {
        assert!(is_allowed_shortcut("a"));
        assert!(is_allowed_shortcut("d"));
        assert!(is_allowed_shortcut("r"));
        assert!(is_allowed_shortcut("x"));
    }

    #[test]
    fn is_allowed_shortcut_accepts_valid_uppercase() {
        assert!(is_allowed_shortcut("A"));
        assert!(is_allowed_shortcut("D"));
        assert!(is_allowed_shortcut("R"));
        assert!(is_allowed_shortcut("S"));
    }

    #[test]
    fn is_allowed_shortcut_rejects_reserved_keys() {
        // Navigation keys.
        assert!(!is_allowed_shortcut("j"));
        assert!(!is_allowed_shortcut("k"));
        assert!(!is_allowed_shortcut("J"));
        assert!(!is_allowed_shortcut("K"));
        assert!(!is_allowed_shortcut("q"));
        assert!(!is_allowed_shortcut("n"));
        assert!(!is_allowed_shortcut("m"));
        assert!(!is_allowed_shortcut("t"));
        assert!(!is_allowed_shortcut("z"));
        assert!(!is_allowed_shortcut("Z"));
        assert!(!is_allowed_shortcut("H"));
        assert!(!is_allowed_shortcut("L"));
        assert!(!is_allowed_shortcut("M"));
        assert!(!is_allowed_shortcut("V"));
    }

    #[test]
    fn is_allowed_shortcut_rejects_special_characters() {
        assert!(!is_allowed_shortcut("?"));
        assert!(!is_allowed_shortcut("/"));
        assert!(!is_allowed_shortcut(":"));
        assert!(!is_allowed_shortcut(";"));
        assert!(!is_allowed_shortcut("!"));
        assert!(!is_allowed_shortcut("1"));
        assert!(!is_allowed_shortcut("*"));
    }

    #[test]
    fn is_allowed_shortcut_rejects_multi_char_strings() {
        assert!(!is_allowed_shortcut("ab"));
        assert!(!is_allowed_shortcut("dd"));
        assert!(!is_allowed_shortcut(""));
    }

    #[test]
    fn action_with_shortcut_accepts_valid() {
        let action = Action::new(Verb::new("done").unwrap(), "Done").with_shortcut("d");
        assert_eq!(action.shortcut, Some("d".to_string()));
    }

    #[test]
    fn action_with_shortcut_rejects_invalid() {
        let action = Action::new(Verb::new("quit").unwrap(), "Quit").with_shortcut("q");
        assert_eq!(action.shortcut, None);
    }

    #[test]
    fn action_sanitized_removes_invalid_shortcut() {
        let mut action = Action::new(Verb::new("test").unwrap(), "Test");
        action.shortcut = Some("q".to_string()); // Manually set invalid shortcut.
        let sanitized = action.sanitized();
        assert_eq!(sanitized.shortcut, None);
    }

    #[test]
    fn action_sanitized_preserves_valid_shortcut() {
        let mut action = Action::new(Verb::new("done").unwrap(), "Done");
        action.shortcut = Some("d".to_string());
        let sanitized = action.sanitized();
        assert_eq!(sanitized.shortcut, Some("d".to_string()));
    }

    #[test]
    fn query_result_sanitized_scrubs_all_actions() {
        let mut action1 = Action::new(Verb::new("done").unwrap(), "Done");
        action1.shortcut = Some("d".to_string()); // Valid.
        let mut action2 = Action::new(Verb::new("quit").unwrap(), "Quit");
        action2.shortcut = Some("q".to_string()); // Invalid.
        let mut action3 = Action::new(Verb::new("nav").unwrap(), "Navigate");
        action3.shortcut = Some("j".to_string()); // Invalid.

        let message = Message {
            msg_id: MessageId::new("msg-1").unwrap(),
            date: Utc::now(),
            from: From::new("test@example.com").unwrap(),
            body: Body::new("Test").unwrap(),
            wrap: false,
            actions: vec![action1, action2, action3],
        };

        let mailbox = Mailbox {
            name: MailboxName::new("INBOX").unwrap(),
            messages: vec![message],
        };

        let result = QueryResult::new(vec![mailbox]).sanitized();
        let actions = &result.mailboxes()[0].messages[0].actions;

        assert_eq!(actions.len(), 3);
        assert_eq!(actions[0].shortcut, Some("d".to_string()));
        assert_eq!(actions[1].shortcut, None);
        assert_eq!(actions[2].shortcut, None);
    }
}
