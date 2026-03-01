use crate::feed::FeedItem;
use crate::feed_source::FeedSource;

crate::database!(pub(crate) Store {
    feeds: FeedSource,
    posts: FeedItem,
});

pub(crate) type Transaction<'a> = StoreTransaction<'a>;
