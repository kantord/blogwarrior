use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::data::BlogData;
use crate::data::schema::FeedSource;
use crate::query::Query;
use crate::query::resolve::resolve_posts;

#[derive(Serialize)]
struct ExportItem<'a> {
    title: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    date: Option<&'a DateTime<Utc>>,
    feed: &'a FeedSource,
    link: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    read_at: Option<&'a DateTime<Utc>>,
}

pub(crate) fn cmd_export(store: &BlogData, query: &Query) -> anyhow::Result<()> {
    let resolved = resolve_posts(store, query)?;

    let feeds_by_id: HashMap<String, &FeedSource> = store
        .feeds()
        .iter()
        .map(|(id, feed)| (id.to_string(), feed))
        .collect();

    let reads: HashMap<String, DateTime<Utc>> = store
        .reads()
        .iter()
        .map(|(_, r)| (r.post_id.clone(), r.read_at))
        .collect();

    for item in &resolved.items {
        if let Some(&feed) = feeds_by_id.get(&item.feed) {
            let export = ExportItem {
                title: &item.title,
                date: item.date.as_ref(),
                feed,
                link: &item.link,
                read_at: reads.get(&item.raw_id),
            };
            println!("{}", serde_json::to_string(&export)?);
        }
    }
    Ok(())
}
