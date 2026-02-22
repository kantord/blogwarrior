use serde::{Deserialize, Serialize};

use crate::table::TableRow;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedSource {
    pub id: String,
    pub url: String,
}

impl TableRow for FeedSource {
    fn id(&self) -> &str {
        &self.id
    }
    fn set_id(&mut self, id: String) {
        self.id = id;
    }
}
