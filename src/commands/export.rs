use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::query::Query;
use crate::store::Store;
use crate::tables::FeedSource;

use super::resolve_posts;

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

pub(crate) fn cmd_export(store: &Store, query: &Query) -> anyhow::Result<()> {
    let effective_query;
    let query = if query.is_empty() {
        effective_query = super::show::default_query();
        &effective_query
    } else {
        query
    };
    let resolved = resolve_posts(store, query)?;

    let feeds_by_id: HashMap<String, FeedSource> = store
        .feeds()
        .items()
        .into_iter()
        .map(|f| {
            let id = store.feeds().id_of(&f);
            (id, f)
        })
        .collect();

    let reads: HashMap<String, DateTime<Utc>> = store
        .reads()
        .items()
        .into_iter()
        .map(|r| (r.post_id, r.read_at))
        .collect();

    for item in &resolved.items {
        if let Some(feed) = feeds_by_id.get(&item.feed) {
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
