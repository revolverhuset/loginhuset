use chrono;
use super::schema::{users, sessions};

#[derive(Insertable)]
#[table_name="users"]
pub struct NewUser<'a> {
    pub email: &'a str,
    pub name: &'a str,
}

#[derive(Queryable)]
pub struct User {
    pub id: i32,
    pub email: String,
    pub name: String,
}

#[derive(Insertable)]
#[table_name="sessions"]
pub struct NewSession<'a> {
    pub user_id: i32,
    pub token: &'a str,
}

#[derive(Queryable, Identifiable, Associations)]
#[belongs_to(User)]
pub struct Session {
    pub id: i32,
    pub user_id: i32,
    pub token: String,
    pub created: chrono::NaiveDateTime,
}
