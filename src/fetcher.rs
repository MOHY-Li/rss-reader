use feedparser_rs::{ParsedFeed, parse};
use reqwest::Client;
use std::error::Error;
use std::time::Duration;

use crate::config::Args;

pub fn build_client(args: &Args) -> Result<Client, reqwest::Error> {
    Client::builder()
        .user_agent(args.user_agent.clone())
        .timeout(Duration::from_secs(args.timeout_secs))
        .build()
}

pub async fn fetch_and_parse(
    client: &Client,
    url: &str,
) -> Result<ParsedFeed, Box<dyn Error + Send + Sync>> {
    let response = client.get(url).send().await?.error_for_status()?;
    let bytes = response.bytes().await?;
    let feed = parse(bytes.as_ref()).map_err(|err| -> Box<dyn Error + Send + Sync> {
        Box::new(err)
    })?;
    Ok(feed)
}
