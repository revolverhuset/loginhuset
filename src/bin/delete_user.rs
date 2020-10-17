extern crate diesel;
extern crate dotenv;
extern crate loginhuset;

use self::diesel::prelude::*;
use self::loginhuset::*;
use dotenv::dotenv;
use std::env;
use std::env::args;

fn main() {
    use ::loginhuset::schema::users::dsl::*;
    dotenv().ok();

    let target = args().nth(1).expect("Expected a target to match against");
    let pattern = format!("%{}%", target);

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let connection = establish_connection(&database_url);
    let num_deleted = diesel::delete(users.filter(email.like(pattern)))
        .execute(&connection)
        .expect("Error deleting users");

    println!("Deleted {} users", num_deleted);
}
