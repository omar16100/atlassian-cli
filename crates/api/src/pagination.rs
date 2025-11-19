use crate::error::Result;
use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PagedResponse<T> {
    pub values: Vec<T>,
    #[serde(rename = "startAt")]
    pub start_at: Option<u32>,
    #[serde(rename = "maxResults")]
    pub max_results: Option<u32>,
    pub total: Option<u32>,
    #[serde(rename = "isLast")]
    pub is_last: Option<bool>,
}

impl<T> PagedResponse<T> {
    pub fn has_next(&self) -> bool {
        if let Some(is_last) = self.is_last {
            return !is_last;
        }

        if let (Some(start), Some(max), Some(total)) = (self.start_at, self.max_results, self.total)
        {
            return start + max < total;
        }

        false
    }

    pub fn next_start(&self) -> Option<u32> {
        if !self.has_next() {
            return None;
        }

        match (self.start_at, self.max_results) {
            (Some(start), Some(max)) => Some(start + max),
            _ => None,
        }
    }
}

#[async_trait]
pub trait Paginator<T>: Sync {
    async fn fetch_page(&self, start_at: u32, max_results: u32) -> Result<PagedResponse<T>>;

    async fn fetch_all(&self, max_results: u32) -> Result<Vec<T>>
    where
        T: Send,
    {
        let mut all_items = Vec::new();
        let mut start_at = 0;

        loop {
            debug!(start_at, max_results, "Fetching page");
            let page = self.fetch_page(start_at, max_results).await?;
            let item_count = page.values.len();
            let has_next = page.has_next();
            let next_start = page.next_start();

            all_items.extend(page.values);

            if !has_next || item_count == 0 {
                debug!(total_items = all_items.len(), "Finished pagination");
                break;
            }

            start_at = next_start.unwrap_or(start_at + max_results);
        }

        Ok(all_items)
    }

    fn stream<'a>(
        &'a self,
        max_results: u32,
    ) -> Pin<Box<dyn Stream<Item = Result<Vec<T>>> + Send + 'a>>
    where
        T: Send + 'a,
    {
        Box::pin(async_stream::stream! {
            let mut start_at = 0;

            loop {
                debug!(start_at, max_results, "Fetching page in stream");
                let page = self.fetch_page(start_at, max_results).await;

                match page {
                    Ok(page) => {
                        let item_count = page.values.len();
                        let has_next = page.has_next();
                        let next_start = page.next_start();

                        yield Ok(page.values);

                        if !has_next || item_count == 0 {
                            break;
                        }

                        start_at = next_start.unwrap_or(start_at + max_results);
                    }
                    Err(err) => {
                        yield Err(err);
                        break;
                    }
                }
            }
        })
    }
}

pub async fn collect_pages<T, P: Paginator<T>>(
    paginator: &P,
    max_results: u32,
    limit: Option<usize>,
) -> Result<Vec<T>>
where
    T: Send,
{
    let mut stream = paginator.stream(max_results);
    let mut all_items = Vec::new();

    while let Some(result) = stream.next().await {
        let items = result?;
        all_items.extend(items);

        if let Some(limit) = limit {
            if all_items.len() >= limit {
                all_items.truncate(limit);
                break;
            }
        }
    }

    Ok(all_items)
}
