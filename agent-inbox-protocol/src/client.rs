use crate::{ActionRequest, ActionResponse, MailboxProvider, QueryParameters, QueryResult};

#[derive(Clone, Debug)]
pub struct Client {
    base_url: String,
    http: reqwest::Client,
}

impl Client {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http: reqwest::Client::new(),
        }
    }

    /// Sets a custom reqwest::Client for HTTP requests.
    pub fn with_client(mut self, http: reqwest::Client) -> Self {
        self.http = http;
        self
    }

    fn query_url(&self) -> String {
        format!("{}/query", self.base_url.trim_end_matches('/'))
    }

    fn action_url(&self) -> String {
        format!("{}/action", self.base_url.trim_end_matches('/'))
    }
}

#[async_trait::async_trait]
impl MailboxProvider for Client {
    type Error = reqwest::Error;

    async fn query(&mut self, query: QueryParameters) -> Result<QueryResult, Self::Error> {
        let response = self
            .http
            .post(self.query_url())
            .json(&query)
            .send()
            .await?
            .error_for_status()?;
        let result = response.json::<QueryResult>().await?;
        Ok(result)
    }

    async fn action(&mut self, request: ActionRequest) -> Result<ActionResponse, Self::Error> {
        let response = self
            .http
            .post(self.action_url())
            .json(&request)
            .send()
            .await?
            .error_for_status()?;
        response.json::<ActionResponse>().await
    }
}
