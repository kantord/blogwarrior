use std::path::Path;

use crate::feed::FeedItem;
use crate::feed_source::FeedSource;

pub(crate) struct Store {
    pub feeds: synctato::Table<FeedSource>,
    pub posts: synctato::Table<FeedItem>,
}

impl Store {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let feeds = synctato::Table::<FeedSource>::load(path)?;
        let posts = synctato::Table::<FeedItem>::load(path)?;
        Ok(Self { feeds, posts })
    }

    pub fn save(&self) -> anyhow::Result<()> {
        self.feeds.save()?;
        self.posts.save()?;
        Ok(())
    }
}
