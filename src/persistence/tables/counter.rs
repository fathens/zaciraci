use diesel::prelude::*;

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::persistence::schema::counter)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Counter {
    pub value: i32,
}
