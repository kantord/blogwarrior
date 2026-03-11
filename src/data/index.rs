use crate::data::schema::FeedSource;
use crate::shorthand::compute_shorthands;

pub(crate) struct FeedEntry {
    pub feed: FeedSource,
    pub id: String,
    pub shorthand: String,
}

pub(crate) struct FeedIndex {
    pub entries: Vec<FeedEntry>,
}

impl FeedIndex {
    fn find_by_shorthand(&self, shorthand: &str) -> Option<&FeedEntry> {
        self.entries.iter().find(|e| e.shorthand == shorthand)
    }

    pub(crate) fn id_for_shorthand(&self, shorthand: &str) -> Option<&str> {
        self.find_by_shorthand(shorthand).map(|e| e.id.as_str())
    }

    pub(crate) fn url_for_shorthand(&self, shorthand: &str) -> Option<&str> {
        self.find_by_shorthand(shorthand)
            .map(|e| e.feed.url.as_str())
    }
}

pub(crate) fn feed_index(table: &synctato::Table<FeedSource>) -> FeedIndex {
    let mut pairs: Vec<(String, FeedSource)> = table
        .iter()
        .map(|(id, feed)| (id.to_string(), feed.clone()))
        .collect();
    pairs.sort_by(|(_, a), (_, b)| a.url.cmp(&b.url));
    let ids: Vec<String> = pairs.iter().map(|(id, _)| id.clone()).collect();
    let shorthands = compute_shorthands(&ids);
    let entries = pairs
        .into_iter()
        .zip(shorthands)
        .map(|((id, feed), shorthand)| FeedEntry {
            feed,
            id,
            shorthand,
        })
        .collect();
    FeedIndex { entries }
}

pub(crate) fn resolve_shorthand(
    feeds_table: &synctato::Table<FeedSource>,
    shorthand: &str,
) -> Option<String> {
    feed_index(feeds_table)
        .url_for_shorthand(shorthand)
        .map(|s| s.to_string())
}
