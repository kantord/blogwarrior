use crate::schema::BlogDataSchema;
use synctato::Store;

pub(crate) type BlogData = Store<BlogDataSchema>;
pub(crate) type Transaction<'a> = crate::schema::BlogDataSchemaTransaction<'a>;
