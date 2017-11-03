extern crate loginhuset;
extern crate diesel;

use self::diesel::prelude::*;
use self::loginhuset::*;
use std::env::args;

fn main() {
    use ::loginhuset::schema::users::dsl::*;

    let target = args().nth(1).expect("Expected a target to match against");
    let pattern = format!("%{}%", target);

    let connection = establish_connection();
    let num_deleted = diesel::delete(users.filter(email.like(pattern)))
        .execute(&connection)
        .expect("Error deleting users");

    println!("Deleted {} users", num_deleted);
}
