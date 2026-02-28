use std::path::Path;

use crate::feed::FeedItem;
use crate::feed_source::FeedSource;

pub(crate) struct Store {
    feeds: synctato::Table<FeedSource>,
    posts: synctato::Table<FeedItem>,
}

pub(crate) struct Transaction<'a> {
    pub feeds: &'a mut synctato::Table<FeedSource>,
    pub posts: &'a mut synctato::Table<FeedItem>,
}

impl Store {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let feeds = synctato::Table::<FeedSource>::load(path)?;
        let posts = synctato::Table::<FeedItem>::load(path)?;
        Ok(Self { feeds, posts })
    }

    pub fn feeds(&self) -> &synctato::Table<FeedSource> {
        &self.feeds
    }

    pub fn posts(&self) -> &synctato::Table<FeedItem> {
        &self.posts
    }

    pub fn transaction<F, T>(&mut self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&mut Transaction) -> anyhow::Result<T>,
    {
        let result = {
            let mut tx = Transaction {
                feeds: &mut self.feeds,
                posts: &mut self.posts,
            };
            f(&mut tx)?
        };
        self.feeds.save()?;
        self.posts.save()?;
        Ok(result)
    }
}
