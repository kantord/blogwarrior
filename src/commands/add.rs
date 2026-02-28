use std::path::Path;

use crate::feed_source::FeedSource;

pub(crate) fn cmd_add(store: &Path, url: &str) -> anyhow::Result<()> {
    let mut table = synctato::Table::<FeedSource>::load(store)?;
    table.upsert(FeedSource {
        url: url.to_string(),
        title: String::new(),
        site_url: String::new(),
        description: String::new(),
    });
    table.save()
}
