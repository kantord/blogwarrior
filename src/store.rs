use crate::feed::FeedItem;
use crate::feed_source::FeedSource;
use crate::read_mark::ReadMark;

crate::database!(pub(crate) Store {
    feeds: FeedSource,
    posts: FeedItem,
    reads: ReadMark,
});

pub(crate) type Transaction<'a> = StoreTransaction<'a>;
