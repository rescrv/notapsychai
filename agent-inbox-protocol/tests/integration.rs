#[cfg(all(feature = "client", feature = "server"))]
mod integration {
    use agent_inbox_protocol::{
        Body, Client, From, Mailbox, MailboxName, MailboxProvider, Message, QueryParameters,
        QueryResult, router,
    };
    use chrono::Utc;
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    #[derive(Clone)]
    struct DummyProvider {
        expected: QueryParameters,
        response: QueryResult,
    }

    #[async_trait::async_trait]
    impl MailboxProvider for DummyProvider {
        type Error = std::convert::Infallible;

        async fn query(&mut self, query: QueryParameters) -> Result<QueryResult, Self::Error> {
            assert_eq!(query, self.expected);
            Ok(self.response.clone())
        }
    }

    #[tokio::test]
    async fn client_queries_server_over_random_socket() {
        let mailbox = Mailbox {
            name: MailboxName::new("inbox").expect("valid mailbox name"),
            messages: vec![Message {
                date: Utc::now(),
                from: From::new("alice@example.com").expect("valid from"),
                body: Body::new("hello from the integration test").expect("valid body"),
                wrap: false,
            }],
        };
        let expected = QueryResult::new(vec![mailbox]);
        let query = QueryParameters {
            search: Some("hello".to_string()),
            keywords: Some(vec!["integration".to_string()]),
            max_per_inbox: Some(10),
            max_across_inboxes: Some(10),
        };

        let provider = DummyProvider {
            expected: query.clone(),
            response: expected.clone(),
        };
        let app = router(provider);

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind random port");
        let addr = listener.local_addr().expect("local addr");
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let server_handle = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await
                .expect("server exits cleanly");
        });

        tokio::task::yield_now().await;

        let mut client = Client::new(format!("http://{}", addr));
        let response = client.query(query).await.expect("query succeeds");
        assert_eq!(response, expected);

        let _ = shutdown_tx.send(());
        let _ = server_handle.await;
    }
}
