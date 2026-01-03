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

use labradormail::router;
use labradormail::sample_mailbox_data;
use labradormail::Mailbox;
use labradormail::MailboxProvider;
use labradormail::QueryParameters;
use labradormail::QueryResult;

/// A simple mailbox provider that serves static sample data.
struct SampleMailboxProvider {
    mailboxes: Vec<Mailbox>,
}

impl SampleMailboxProvider {
    fn new() -> Self {
        Self {
            mailboxes: sample_mailbox_data(),
        }
    }
}

#[async_trait::async_trait]
impl MailboxProvider for SampleMailboxProvider {
    type Error = Infallible;

    async fn query(&mut self, _query: QueryParameters) -> Result<QueryResult, Self::Error> {
        Ok(QueryResult::new(self.mailboxes.clone()))
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
