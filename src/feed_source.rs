use serde::{Deserialize, Serialize};

use crate::table::TableRow;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedSource {
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub site_url: String,
    #[serde(default)]
    pub description: String,
}

impl TableRow for FeedSource {
    fn key(&self) -> String {
        self.url.clone()
    }
}
