pub mod atom;
pub mod rss;

#[derive(Debug, PartialEq)]
pub struct FeedItem {
    pub title: String,
    pub date: String,
}
