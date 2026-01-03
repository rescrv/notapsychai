//! Example server that hosts the agent-inbox-protocol on localhost:3000.
//!
//! This server uses the sample mailbox data from labradormail and serves it via the
//! agent-inbox-protocol. Run with:
//!
//! ```sh
//! cargo run --example server
//! ```
//!
//! Then in another terminal:
//!
//! ```sh
//! cargo run --bin labrador
//! ```

use std::convert::Infallible;

use agent_inbox_protocol::Action;
use agent_inbox_protocol::Body;
use agent_inbox_protocol::From;
use agent_inbox_protocol::MailboxName;
use agent_inbox_protocol::Message;
use agent_inbox_protocol::MessageId;
use agent_inbox_protocol::Verb;
use chrono::Utc;
use labradormail::router;
use labradormail::ActionRequest;
use labradormail::ActionResponse;
use labradormail::Mailbox;
use labradormail::MailboxProvider;
use labradormail::QueryParameters;
use labradormail::QueryResult;

/// A simple mailbox provider that serves sample data with actions.
struct SampleMailboxProvider {
    mailboxes: Vec<Mailbox>,
}

impl SampleMailboxProvider {
    fn new() -> Self {
        Self {
            mailboxes: create_sample_mailboxes_with_actions(),
        }
    }
}

/// Creates sample mailboxes with actions attached to messages.
fn create_sample_mailboxes_with_actions() -> Vec<Mailbox> {
    let done_action = Action::new(Verb::new("done").expect("valid verb"), "Mark Done")
        .with_shortcut("d");
    let archive_action = Action::new(Verb::new("archive").expect("valid verb"), "Archive")
        .with_shortcut("a");
    let defer_action = Action::new(Verb::new("defer").expect("valid verb"), "Defer")
        .with_shortcut("D");
    let reply_action = Action::new(Verb::new("reply").expect("valid verb"), "Reply")
        .with_shortcut("r");
    let delete_action = Action::new(Verb::new("delete").expect("valid verb"), "Delete")
        .with_shortcut("x");

    let inbox = Mailbox {
        name: MailboxName::new("INBOX").expect("valid mailbox name"),
        messages: vec![
            Message {
                msg_id: MessageId::new("msg-001").expect("valid message id"),
                date: Utc::now(),
                from: From::new("alice@example.com").expect("valid from"),
                body: Body::new("Hello World - This is a test message with actions").expect("valid body"),
                wrap: false,
                actions: vec![done_action.clone(), archive_action.clone(), reply_action.clone()],
            },
            Message {
                msg_id: MessageId::new("msg-002").expect("valid message id"),
                date: Utc::now(),
                from: From::new("bob@example.com").expect("valid from"),
                body: Body::new("Re: Hello World - Another message with different actions").expect("valid body"),
                wrap: false,
                actions: vec![done_action.clone(), defer_action.clone(), delete_action.clone()],
            },
            Message {
                msg_id: MessageId::new("msg-003").expect("valid message id"),
                date: Utc::now(),
                from: From::new("charlie@example.com").expect("valid from"),
                body: Body::new("Project Update - Important project information").expect("valid body"),
                wrap: false,
                actions: vec![archive_action.clone(), reply_action.clone()],
            },
        ],
    };

    let sent = Mailbox {
        name: MailboxName::new("Sent").expect("valid mailbox name"),
        messages: vec![
            Message {
                msg_id: MessageId::new("msg-004").expect("valid message id"),
                date: Utc::now(),
                from: From::new("me@example.com").expect("valid from"),
                body: Body::new("Re: Hello World").expect("valid body"),
                wrap: false,
                actions: vec![archive_action.clone()],
            },
        ],
    };

    let drafts = Mailbox {
        name: MailboxName::new("Drafts").expect("valid mailbox name"),
        messages: vec![Message {
            msg_id: MessageId::new("msg-005").expect("valid message id"),
            date: Utc::now(),
            from: From::new("me@example.com").expect("valid from"),
            body: Body::new("Draft: Proposal").expect("valid body"),
            wrap: false,
            actions: vec![delete_action.clone()],
        }],
    };

    let trash = Mailbox {
        name: MailboxName::new("Trash").expect("valid mailbox name"),
        messages: vec![Message {
            msg_id: MessageId::new("msg-006").expect("valid message id"),
            date: Utc::now(),
            from: From::new("spam@example.com").expect("valid from"),
            body: Body::new("You've won!").expect("valid body"),
            wrap: false,
            actions: vec![delete_action],
        }],
    };

    vec![inbox, sent, drafts, trash]
}

#[async_trait::async_trait]
impl MailboxProvider for SampleMailboxProvider {
    type Error = Infallible;

    async fn query(&mut self, _query: QueryParameters) -> Result<QueryResult, Self::Error> {
        Ok(QueryResult::new(self.mailboxes.clone()))
    }

    async fn action(&mut self, request: ActionRequest) -> Result<ActionResponse, Self::Error> {
        eprintln!("Action request: {:?}", request);
        Ok(ActionResponse::success_with_message(format!(
            "Performed '{}' on message '{}'",
            request.verb.as_str(),
            request.message_id.as_str()
        )))
    }
}

#[tokio::main]
async fn main() {
    let provider = SampleMailboxProvider::new();
    let app = router(provider);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .expect("failed to bind to port 3000");

    println!("Example server listening on http://localhost:3000");
    println!("Press Ctrl+C to stop");

    axum::serve(listener, app).await.expect("server error");
}
