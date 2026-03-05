mod feed_item;
mod feed_source;
mod read_mark;

pub use feed_item::FeedItem;
pub use feed_source::FeedSource;
pub use read_mark::ReadMark;

crate::schema!(pub(crate) BlogDataSchema {
    feeds: FeedSource,
    posts: FeedItem,
    reads: ReadMark,
});

pub(crate) type BlogData = crate::synctato::Connection<BlogDataSchema>;
pub(crate) type Transaction<'a> = BlogDataSchemaTransaction<'a>;
