use feedparser_rs::{ParsedFeed, parse};
use reqwest::Client;
use scraper::{Html, Selector};
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

pub async fn fetch_article_html(
    client: &Client,
    url: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let response = client.get(url).send().await?.error_for_status()?;
    let body = response.text().await?;
    Ok(extract_article_html(&body))
}

fn extract_article_html(html: &str) -> String {
    let document = Html::parse_document(html);
    let selectors = [
        "article",
        "main",
        "div.post-content",
        "div.article-content",
        "div.entry-content",
        "div.post-body",
        "div#content",
    ];

    for selector in selectors {
        if let Ok(selector) = Selector::parse(selector) {
            if let Some(element) = document.select(&selector).next() {
                return element.html();
            }
        }
    }

    html.to_string()
}
