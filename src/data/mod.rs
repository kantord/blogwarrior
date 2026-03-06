pub mod index;
pub mod schema;

use schema::BlogDataSchema;
use synctato::Store;

pub(crate) type BlogData = Store<BlogDataSchema>;
pub(crate) type Transaction<'a> = schema::BlogDataSchemaTransaction<'a>;
