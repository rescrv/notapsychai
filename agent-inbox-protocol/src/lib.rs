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
    pub fn with_shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
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
