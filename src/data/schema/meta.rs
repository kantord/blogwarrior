use serde::{Deserialize, Serialize};
use synctato::TableRow;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetaEntry {
    pub key: String,
    pub value: String,
}

impl TableRow for MetaEntry {
    fn key(&self) -> String {
        self.key.clone()
    }
    const TABLE_NAME: &'static str = "meta";
    const SHARD_CHARACTERS: usize = 0;
    const EXPECTED_CAPACITY: usize = 100;
}
