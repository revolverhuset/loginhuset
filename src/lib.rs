extern crate chrono;
#[macro_use] extern crate diesel;

use self::models::{NewUser, NewSession, User};

pub mod schema;
pub mod models;

use diesel::sqlite::SqliteConnection;
use diesel::prelude::*;

pub fn create_session<'a>(conn: &SqliteConnection, user: &'a User, token: &'a str) -> usize {
    use schema::sessions;

    let new_session = NewSession {
        user_id: user.id,
        token: token,
    };

    diesel::insert_into(sessions::table)
        .values(&new_session)
        .execute(conn)
        .expect("Error saving session")
}

pub fn create_user<'a>(conn: &SqliteConnection, email: &'a str, name: &'a str) -> usize {
    use schema::users;

    let new_user = NewUser {
        email: email,
        name: name,
    };

    diesel::insert_into(users::table)
        .values(&new_user)
        .execute(conn)
        .expect("Error saving new user")
}

pub fn establish_connection(db: &str) -> SqliteConnection {
    SqliteConnection::establish(db)
        .expect(&format!("Error connecting to {}", db))
}
