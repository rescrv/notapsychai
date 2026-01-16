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

use agent_inbox_protocol::Body;
use agent_inbox_protocol::From;
use agent_inbox_protocol::MailboxName;
use agent_inbox_protocol::Message;
use agent_inbox_protocol::MessageID;
use chrono::Utc;
use claudius::ToolParam;
use labradormail::router;
use labradormail::Mailbox;
use labradormail::MailboxProvider;
use labradormail::QueryParameters;
use labradormail::QueryResult;

use agent_inbox_protocol::ToolCallRequest;
use agent_inbox_protocol::ToolCallResponse;

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

    fn delete_message(&mut self, message_id: &MessageID) -> bool {
        let target = message_id.as_str();
        let mut removed = false;
        for mailbox in &mut self.mailboxes {
            let before = mailbox.messages.len();
            mailbox
                .messages
                .retain(|message| message.msg_id.as_str() != target);
            removed |= before != mailbox.messages.len();
        }
        removed
    }
}

/// Creates bulk sample mailboxes with 25+ messages across Email, GitHub, RSS, Slack, and Jira.
fn create_bulk_sample_mailboxes_with_actions() -> Vec<Mailbox> {
    let empty_schema = serde_json::json!({"type": "object", "properties": {}});
    let reply_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "subject": {"type": "string", "description": "Reply subject"},
            "body": {"type": "string", "description": "Reply body"},
            "urgent": {"type": "boolean", "description": "Mark as urgent"}
        }
    });
    let defer_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "until": {"type": "string", "description": "ISO date or date-time"},
            "reason": {"type": "string", "description": "Why this is deferred"}
        }
    });
    let delete_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "confirm": {"type": "boolean", "description": "Confirm permanent deletion"},
            "reason": {"type": "string", "description": "Optional deletion reason"}
        }
    });

    let done_action = ToolParam::new("done".to_string(), empty_schema.clone())
        .with_description("Mark the message as done and remove it from the inbox.".to_string());
    let archive_action = ToolParam::new("archive".to_string(), empty_schema.clone())
        .with_description("Archive the message for later reference.".to_string());
    let defer_action = ToolParam::new("defer".to_string(), defer_schema)
        .with_description("Defer the message to be handled at a later time.".to_string());
    let reply_action = ToolParam::new("reply".to_string(), reply_schema)
        .with_description("Reply to the message sender.".to_string());
    let delete_action = ToolParam::new("delete".to_string(), delete_schema)
        .with_description("Permanently delete the message.".to_string());

    let email = Mailbox {
        name: MailboxName::new("Email").expect("valid mailbox name"),
        messages: vec![
            Message {
                msg_id: MessageID::new("email-hammad-9012").expect("valid message id"),
                date: Utc::now(),
                from: From::new("Hammad <hammad@example.com>").expect("valid from"),
                body: Body::new(
                    "Re: Customer latency spikes - urgent\n\n\
                    Hey team,\n\n\
                    I just got off a call with Acme Corp and they're seeing the same latency issues \
                    we discussed yesterday. They measured 500ms+ response times during their batch \
                    processing window. Can we prioritize the connection pool fix? They're threatening \
                    to escalate to their VP.\n\n\
                    Let me know if you need any additional data from their logs.\n\n\
                    Thanks,\nHammad",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![
                    archive_action.clone(),
                    reply_action.clone(),
                    delete_action.clone(),
                ],
            },
            Message {
                msg_id: MessageID::new("email-sarah-1001").expect("valid message id"),
                date: Utc::now(),
                from: From::new("Sarah Chen <sarah@example.com>").expect("valid from"),
                body: Body::new(
                    "Q4 Planning Meeting - Action Items\n\n\
                    Hi all,\n\n\
                    Following up on yesterday's planning session. Here are the key action items:\n\
                    1. Finalize roadmap by Friday\n\
                    2. Budget proposals due next Monday\n\
                    3. Team capacity planning spreadsheet needs updates\n\n\
                    Please confirm your deliverables.\n\n\
                    Best,\nSarah",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![
                    archive_action.clone(),
                    reply_action.clone(),
                    defer_action.clone(),
                ],
            },
            Message {
                msg_id: MessageID::new("email-newsletter-1002").expect("valid message id"),
                date: Utc::now(),
                from: From::new("TechWeekly <newsletter@techweekly.com>").expect("valid from"),
                body: Body::new(
                    "This Week in Tech: AI Breakthroughs and Cloud Trends\n\n\
                    Top stories:\n\
                    - OpenAI announces new model capabilities\n\
                    - AWS re:Invent highlights\n\
                    - Kubernetes 1.29 released with new features\n\n\
                    Click to read more...",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![archive_action.clone(), delete_action.clone()],
            },
            Message {
                msg_id: MessageID::new("email-mike-1003").expect("valid message id"),
                date: Utc::now(),
                from: From::new("Mike Johnson <mike@partner.co>").expect("valid from"),
                body: Body::new(
                    "Partnership Proposal - Integration Opportunity\n\n\
                    Hi,\n\n\
                    We've been following your product and believe there's a strong synergy \
                    for an integration partnership. Our platform serves 50K+ developers and \
                    your API would be a natural fit.\n\n\
                    Would you be open to a 30-minute call next week?\n\n\
                    Regards,\nMike",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![
                    reply_action.clone(),
                    defer_action.clone(),
                    archive_action.clone(),
                ],
            },
            Message {
                msg_id: MessageID::new("email-hr-1004").expect("valid message id"),
                date: Utc::now(),
                from: From::new("HR Team <hr@company.com>").expect("valid from"),
                body: Body::new(
                    "Reminder: Annual Benefits Enrollment Deadline\n\n\
                    This is a reminder that open enrollment closes on Friday.\n\n\
                    Please review and update your benefits selections in the HR portal.\n\
                    Changes include new dental plan options and increased 401k match.\n\n\
                    Questions? Contact benefits@company.com",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![done_action.clone(), defer_action.clone()],
            },
            Message {
                msg_id: MessageID::new("email-aws-1005").expect("valid message id"),
                date: Utc::now(),
                from: From::new("AWS Billing <billing@aws.amazon.com>").expect("valid from"),
                body: Body::new(
                    "Your AWS Bill for November 2024\n\n\
                    Account: 123456789012\n\
                    Total: $4,287.43\n\n\
                    Top services:\n\
                    - EC2: $2,100.00\n\
                    - RDS: $1,200.00\n\
                    - S3: $500.00\n\n\
                    View detailed breakdown in console.",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![archive_action.clone(), done_action.clone()],
            },
        ],
    };

    let github = Mailbox {
        name: MailboxName::new("GitHub").expect("valid mailbox name"),
        messages: vec![
            Message {
                msg_id: MessageID::new("github-issue-1234").expect("valid message id"),
                date: Utc::now(),
                from: From::new("alice@example.com").expect("valid from"),
                body: Body::new(
                    "Issue #1234: Customers seeing latency spikes\n\n\
                    Several enterprise customers reported intermittent latency spikes during peak hours. \
                    P99 latency jumped from 50ms to 800ms between 2pm and 4pm UTC. \
                    Initial investigation suggests database connection pool exhaustion. \
                    Need to review connection limits and consider adding circuit breakers.",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![archive_action.clone(), reply_action.clone()],
            },
            Message {
                msg_id: MessageID::new("github-pr-2001").expect("valid message id"),
                date: Utc::now(),
                from: From::new("dependabot[bot]").expect("valid from"),
                body: Body::new(
                    "PR #2001: Bump lodash from 4.17.20 to 4.17.21\n\n\
                    Bumps lodash from 4.17.20 to 4.17.21.\n\n\
                    Release notes:\n\
                    - Security fix for prototype pollution vulnerability\n\
                    - CVE-2021-23337 addressed\n\n\
                    Changelog and compatibility notes available.",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![done_action.clone(), defer_action.clone()],
            },
            Message {
                msg_id: MessageID::new("github-issue-2002").expect("valid message id"),
                date: Utc::now(),
                from: From::new("frustrated_user").expect("valid from"),
                body: Body::new(
                    "Issue #2002: Documentation is outdated\n\n\
                    The quickstart guide references API v1 endpoints but v2 is the default now.\n\
                    Steps to reproduce:\n\
                    1. Follow getting started guide\n\
                    2. Try the example curl command\n\
                    3. Get 404 error\n\n\
                    This is blocking new user onboarding.",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![reply_action.clone(), archive_action.clone()],
            },
            Message {
                msg_id: MessageID::new("github-pr-2003").expect("valid message id"),
                date: Utc::now(),
                from: From::new("senior_dev").expect("valid from"),
                body: Body::new(
                    "PR #2003: Refactor authentication middleware\n\n\
                    This PR consolidates our auth logic into a single middleware.\n\n\
                    Changes:\n\
                    - Unified JWT validation\n\
                    - Added rate limiting per user\n\
                    - Improved error messages\n\n\
                    Tests passing. Ready for review.",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![
                    reply_action.clone(),
                    done_action.clone(),
                    defer_action.clone(),
                ],
            },
            Message {
                msg_id: MessageID::new("github-issue-2004").expect("valid message id"),
                date: Utc::now(),
                from: From::new("security_researcher").expect("valid from"),
                body: Body::new(
                    "Issue #2004: [Security] Potential SSRF in webhook handler\n\n\
                    I've identified a potential SSRF vulnerability in the webhook validation endpoint.\n\
                    The URL parameter is not properly sanitized before making outbound requests.\n\n\
                    Steps and PoC available via security@example.com.\n\n\
                    Severity: High",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![reply_action.clone(), archive_action.clone()],
            },
            Message {
                msg_id: MessageID::new("github-discussion-2005").expect("valid message id"),
                date: Utc::now(),
                from: From::new("community_member").expect("valid from"),
                body: Body::new(
                    "Discussion #2005: RFC - New plugin architecture\n\n\
                    I'd like to propose a new plugin system for extensibility.\n\n\
                    Key points:\n\
                    - Sandboxed execution environment\n\
                    - TypeScript SDK for plugin authors\n\
                    - Marketplace for sharing plugins\n\n\
                    Looking for feedback from maintainers.",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![reply_action.clone(), defer_action.clone()],
            },
        ],
    };

    let rss = Mailbox {
        name: MailboxName::new("RSS").expect("valid mailbox name"),
        messages: vec![
            Message {
                msg_id: MessageID::new("rss-techblog-5678").expect("valid message id"),
                date: Utc::now(),
                from: From::new("Engineering Blog").expect("valid from"),
                body: Body::new(
                    "How We Debugged a Mysterious Latency Spike\n\n\
                    Our team recently tackled a production incident where API latency spiked \
                    unpredictably. After ruling out database issues, we discovered the root cause \
                    was garbage collection pauses in our Go services. This post walks through our \
                    debugging process and the tuning changes we made to GOGC settings.",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![delete_action.clone()],
            },
            Message {
                msg_id: MessageID::new("rss-hackernews-3001").expect("valid message id"),
                date: Utc::now(),
                from: From::new("Hacker News").expect("valid from"),
                body: Body::new(
                    "Show HN: I built a distributed database in Rust\n\n\
                    After 2 years of weekend work, I'm releasing my distributed KV store.\n\
                    Features: Raft consensus, MVCC, SQL layer.\n\
                    GitHub: github.com/example/rustdb\n\n\
                    Points: 847 | Comments: 234",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![archive_action.clone(), delete_action.clone()],
            },
            Message {
                msg_id: MessageID::new("rss-morning-brew-3002").expect("valid message id"),
                date: Utc::now(),
                from: From::new("Morning Brew").expect("valid from"),
                body: Body::new(
                    "Daily Digest: Markets rally on Fed comments\n\n\
                    - S&P 500 up 1.2% on dovish Fed signals\n\
                    - Tech earnings beat expectations\n\
                    - Crypto markets stable after weekend volatility\n\
                    - Oil prices drop on demand concerns",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![delete_action.clone()],
            },
            Message {
                msg_id: MessageID::new("rss-arxiv-3003").expect("valid message id"),
                date: Utc::now(),
                from: From::new("arXiv CS").expect("valid from"),
                body: Body::new(
                    "New paper: Efficient Attention Mechanisms for Long Context Windows\n\n\
                    Abstract: We present a novel attention mechanism that reduces memory \
                    complexity from O(n^2) to O(n log n) while maintaining model quality. \
                    Experiments show 3x throughput improvement on sequences >100K tokens.\n\n\
                    Authors: Smith et al.",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![archive_action.clone(), defer_action.clone()],
            },
            Message {
                msg_id: MessageID::new("rss-lobsters-3004").expect("valid message id"),
                date: Utc::now(),
                from: From::new("Lobste.rs").expect("valid from"),
                body: Body::new(
                    "Why I'm mass-deleting my old GitHub repos\n\n\
                    A reflection on digital permanence and the hidden costs of maintaining \
                    abandoned projects. Sometimes the best thing you can do for the community \
                    is clearly mark something as unmaintained.\n\n\
                    Score: 127 | Comments: 89",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![delete_action.clone(), archive_action.clone()],
            },
        ],
    };

    let slack = Mailbox {
        name: MailboxName::new("Slack").expect("valid mailbox name"),
        messages: vec![
            Message {
                msg_id: MessageID::new("slack-oncall-4001").expect("valid message id"),
                date: Utc::now(),
                from: From::new("#oncall-alerts").expect("valid from"),
                body: Body::new(
                    "PagerDuty Alert: High error rate on api-prod-west\n\n\
                    Error rate exceeded 5% threshold.\n\
                    Current: 7.2%\n\
                    Service: api-gateway\n\
                    Region: us-west-2\n\n\
                    Runbook: wiki/oncall/api-errors",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![done_action.clone(), defer_action.clone()],
            },
            Message {
                msg_id: MessageID::new("slack-general-4002").expect("valid message id"),
                date: Utc::now(),
                from: From::new("#general").expect("valid from"),
                body: Body::new(
                    "Reminder: All-hands meeting tomorrow at 10am PST\n\n\
                    Agenda:\n\
                    - Q4 results review\n\
                    - 2025 strategy preview\n\
                    - Team awards\n\n\
                    Zoom link in calendar invite.",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![done_action.clone(), archive_action.clone()],
            },
            Message {
                msg_id: MessageID::new("slack-dm-4003").expect("valid message id"),
                date: Utc::now(),
                from: From::new("@janet").expect("valid from"),
                body: Body::new(
                    "Hey, do you have a minute to chat about the migration timeline?\n\n\
                    The stakeholders are pushing for an earlier date and I want to make sure \
                    we're aligned on what's realistic. Maybe we can find 15 min today?",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![reply_action.clone(), defer_action.clone()],
            },
            Message {
                msg_id: MessageID::new("slack-random-4004").expect("valid message id"),
                date: Utc::now(),
                from: From::new("#random").expect("valid from"),
                body: Body::new(
                    "Anyone want to join a lunch group for the new ramen place?\n\n\
                    Planning to go around 12:30. They have great vegetarian options too.\n\
                    React with :ramen: if interested!",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![delete_action.clone()],
            },
        ],
    };

    let jira = Mailbox {
        name: MailboxName::new("Jira").expect("valid mailbox name"),
        messages: vec![
            Message {
                msg_id: MessageID::new("jira-eng-5001").expect("valid message id"),
                date: Utc::now(),
                from: From::new("Jira").expect("valid from"),
                body: Body::new(
                    "ENG-5001: Implement rate limiting for public API\n\n\
                    Priority: High\n\
                    Assignee: You\n\
                    Sprint: 2024-Q4-S3\n\n\
                    Description: Add configurable rate limits per API key.\n\
                    AC: Support 100/1000/10000 req/min tiers.",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![done_action.clone(), defer_action.clone()],
            },
            Message {
                msg_id: MessageID::new("jira-eng-5002").expect("valid message id"),
                date: Utc::now(),
                from: From::new("Jira").expect("valid from"),
                body: Body::new(
                    "ENG-5002: Database migration blocked\n\n\
                    Status changed: In Progress -> Blocked\n\
                    Blocker: Waiting on DBA approval for schema changes\n\n\
                    Comment from @dba_tom: Need to review index strategy first. \
                    Can we meet Thursday?",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![reply_action.clone(), defer_action.clone()],
            },
            Message {
                msg_id: MessageID::new("jira-eng-5003").expect("valid message id"),
                date: Utc::now(),
                from: From::new("Jira").expect("valid from"),
                body: Body::new(
                    "ENG-5003: Customer reported data export bug\n\n\
                    Priority: Critical\n\
                    Reporter: Support Team\n\n\
                    Large CSV exports (>1M rows) are truncating silently.\n\
                    Customer: BigCorp Inc\n\
                    Impact: $500K ARR account",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![done_action.clone(), reply_action.clone()],
            },
            Message {
                msg_id: MessageID::new("jira-eng-5004").expect("valid message id"),
                date: Utc::now(),
                from: From::new("Jira").expect("valid from"),
                body: Body::new(
                    "ENG-5004: Tech debt - Remove deprecated v1 endpoints\n\n\
                    Priority: Low\n\
                    Sprint: Backlog\n\n\
                    v1 API was deprecated 6 months ago. Usage is now <0.1%.\n\
                    Time to clean up and simplify the codebase.",
                )
                .expect("valid body"),
                wrap: true,
                tools: vec![defer_action.clone(), archive_action.clone()],
            },
        ],
    };

    vec![email, github, rss, slack, jira]
}

/// Creates sample mailboxes with actions attached to messages.
fn create_sample_mailboxes_with_actions() -> Vec<Mailbox> {
    let empty_schema = serde_json::json!({"type": "object", "properties": {}});
    let reply_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "subject": {
                "type": "string",
                "description": "Reply subject"
            },
            "body": {
                "type": "string",
                "description": "Reply body"
            },
            "urgent": {
                "type": "boolean",
                "description": "Mark as urgent"
            }
        }
    });
    let defer_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "until": {
                "type": "string",
                "description": "ISO date or date-time"
            },
            "reason": {
                "type": "string",
                "description": "Why this is deferred"
            }
        }
    });
    let delete_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "confirm": {
                "type": "boolean",
                "description": "Confirm permanent deletion"
            },
            "reason": {
                "type": "string",
                "description": "Optional deletion reason"
            }
        }
    });
    let _done_action = ToolParam::new("done".to_string(), empty_schema.clone())
        .with_description("Mark the message as done and remove it from the inbox.".to_string());
    let archive_action = ToolParam::new("archive".to_string(), empty_schema.clone())
        .with_description("Archive the message for later reference.".to_string());
    let _defer_action = ToolParam::new("defer".to_string(), defer_schema)
        .with_description("Defer the message to be handled at a later time.".to_string());
    let reply_action = ToolParam::new("reply".to_string(), reply_schema)
        .with_description("Reply to the message sender.".to_string());
    let delete_action = ToolParam::new("delete".to_string(), delete_schema)
        .with_description("Permanently delete the message.".to_string());

    let github = Mailbox {
        name: MailboxName::new("GitHub").expect("valid mailbox name"),
        messages: vec![Message {
            msg_id: MessageID::new("github-issue-1234").expect("valid message id"),
            date: Utc::now(),
            from: From::new("alice@example.com").expect("valid from"),
            body: Body::new(
                "Issue #1234: Customers seeing latency spikes\n\n\
                Several enterprise customers reported intermittent latency spikes during peak hours. \
                P99 latency jumped from 50ms to 800ms between 2pm and 4pm UTC. \
                Initial investigation suggests database connection pool exhaustion. \
                Need to review connection limits and consider adding circuit breakers.",
            )
            .expect("valid body"),
            wrap: true,
            tools: vec![archive_action.clone(), reply_action.clone()],
        }],
    };

    let rss = Mailbox {
        name: MailboxName::new("RSS").expect("valid mailbox name"),
        messages: vec![Message {
            msg_id: MessageID::new("rss-techblog-5678").expect("valid message id"),
            date: Utc::now(),
            from: From::new("Engineering Blog").expect("valid from"),
            body: Body::new(
                "How We Debugged a Mysterious Latency Spike\n\n\
                Our team recently tackled a production incident where API latency spiked \
                unpredictably. After ruling out database issues, we discovered the root cause \
                was garbage collection pauses in our Go services. This post walks through our \
                debugging process and the tuning changes we made to GOGC settings.",
            )
            .expect("valid body"),
            wrap: true,
            tools: vec![delete_action.clone()],
        }],
    };

    let email = Mailbox {
        name: MailboxName::new("Email").expect("valid mailbox name"),
        messages: vec![Message {
            msg_id: MessageID::new("email-hammad-9012").expect("valid message id"),
            date: Utc::now(),
            from: From::new("Hammad <hammad@example.com>").expect("valid from"),
            body: Body::new(
                "Re: Customer latency spikes - urgent\n\n\
                Hey team,\n\n\
                I just got off a call with Acme Corp and they're seeing the same latency issues \
                we discussed yesterday. They measured 500ms+ response times during their batch \
                processing window. Can we prioritize the connection pool fix? They're threatening \
                to escalate to their VP.\n\n\
                Let me know if you need any additional data from their logs.\n\n\
                Thanks,\nHammad",
            )
            .expect("valid body"),
            wrap: true,
            tools: vec![archive_action, reply_action, delete_action],
        }],
    };

    vec![email, github, rss]
}

#[async_trait::async_trait]
impl MailboxProvider for SampleMailboxProvider {
    type Error = Infallible;

    async fn query(&mut self, query: QueryParameters) -> Result<QueryResult, Self::Error> {
        if query.search.is_some() {
            Ok(QueryResult::new(create_sample_mailboxes_with_actions()))
        } else {
            Ok(QueryResult::new(create_bulk_sample_mailboxes_with_actions()))
        }
    }

    async fn tool_call(
        &mut self,
        request: ToolCallRequest,
    ) -> Result<ToolCallResponse, Self::Error> {
        eprintln!("Tool call request: {:?}", request);
        if request.name == "delete" {
            let removed = self.delete_message(&request.message_id);
            let status = if removed { "Deleted" } else { "No-op" };
            Ok(ToolCallResponse::success_with_message(format!(
                "{} message '{}'",
                status,
                request.message_id.as_str()
            )))
        } else {
            Ok(ToolCallResponse::success_with_message(format!(
                "Performed '{}' on message '{}'",
                request.name,
                request.message_id.as_str()
            )))
        }
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
