extern crate chrono;
#[macro_use] extern crate diesel;
extern crate dotenv;
#[macro_use] extern crate diesel_codegen;

use self::models::{NewUser, NewSession, User};

pub mod schema;
pub mod models;

use diesel::sqlite::SqliteConnection;
use diesel::prelude::*;
use dotenv::dotenv;
use std::env;


pub fn create_session<'a>(conn: &SqliteConnection, user: &'a User, token: &'a str) -> usize {
    use schema::sessions;

    let new_session = NewSession {
        user_id: user.id,
        token: token,
    };

    diesel::insert(&new_session).into(sessions::table)
        .execute(conn)
        .expect("Error saving session")
}

pub fn create_user<'a>(conn: &SqliteConnection, email: &'a str, name: &'a str) -> usize {
    use schema::users;

    let new_user = NewUser {
        email: email,
        name: name,
    };

    diesel::insert(&new_user).into(users::table)
        .execute(conn)
        .expect("Error saving new user")
}

pub fn establish_connection() -> SqliteConnection {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");
    SqliteConnection::establish(&database_url)
        .expect(&format!("Error connecting to {}", database_url))
}
