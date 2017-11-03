extern crate loginhuset;
extern crate diesel;

use loginhuset::*;
use loginhuset::models::*;
use ::loginhuset::schema::users::dsl::*;
use diesel::prelude::*;

fn main() {
    let connection = establish_connection();

    let results = users
        .load::<User>(&connection)
        .expect("Error loading users");

    println!("{} user{}", results.len(), if results.len()==1 {""} else {"s"});
    for user in results {
        println!("\t{}: {}", user.email, user.name);
    }
}
