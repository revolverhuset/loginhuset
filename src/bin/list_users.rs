extern crate diesel;
extern crate dotenv;
extern crate loginhuset;

use ::loginhuset::schema::users::dsl::*;
use diesel::prelude::*;
use dotenv::dotenv;
use loginhuset::models::*;
use loginhuset::*;
use std::env;

fn main() {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let connection = establish_connection(&database_url);

    let results = users
        .load::<User>(&connection)
        .expect("Error loading users");

    println!(
        "{} user{}",
        results.len(),
        if results.len() == 1 { "" } else { "s" }
    );
    for user in results {
        println!("\t{}: {}", user.email, user.name);
    }
}
