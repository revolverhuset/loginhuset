extern crate chrono;
#[macro_use]
extern crate diesel;

use self::models::{NewSession, NewUser, Session, User};

pub mod models;
pub mod schema;

pub mod utils;

use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;

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
    SqliteConnection::establish(db).expect(&format!("Error connecting to {}", db))
}

pub fn get_user(user_email: &str, db_conn: &SqliteConnection) -> Option<User> {
    use schema::users::dsl::*;
    users
        .filter(email.like(user_email))
        .first::<User>(&*db_conn)
        .optional()
        .expect("Failed to find users table")
}

pub fn delete_session(session: Session, db_conn: &SqliteConnection) {
    use schema::sessions::dsl::*;

    diesel::delete(sessions.filter(token.eq(session.token)))
        .execute(db_conn)
        .expect("DB error");
}

pub fn get_user_for_session(session: &str, db_conn: &SqliteConnection) -> Option<(Session, User)> {
    use schema::sessions::dsl::*;
    use schema::{sessions, users};

    sessions::table
        .inner_join(users::table)
        .filter(token.eq(session))
        .first::<(Session, User)>(db_conn)
        .optional()
        .expect("Failed to load data from DB.")
}
