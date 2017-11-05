extern crate loginhuset;
extern crate diesel;
extern crate dotenv;

use loginhuset::*;
use loginhuset::models::*;
use ::loginhuset::schema::users::dsl::*;
use diesel::prelude::*;
use dotenv::dotenv;
use std::env::args;
use std::env;

fn main() {
    dotenv().ok();
    let args: Vec<String> = args().collect();
    let database_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");
    let mail = &(args.get(1).expect("Missing email address argument"))[..];
    let username = &(args.get(2..).expect("Missing name argument").join(" "))[..];

    let connection = establish_connection(&database_url);

    let ex = users.filter(email.eq(mail))
        .limit(1)
        .load::<User>(&connection)
        .expect("Failed to find user");

    if ex.len() > 0 {
        panic!(format!("User {} already exists!", mail));
    }

    let _ = create_user(&connection, mail, username);
    println!("Saved user {}", mail);
}
