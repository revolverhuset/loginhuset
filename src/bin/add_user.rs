extern crate loginhuset;
extern crate diesel;


use loginhuset::*;
use loginhuset::models::*;
use ::loginhuset::schema::users::dsl::*;
use diesel::prelude::*;

use std::env::args;

fn main() {
    let args: Vec<String> = args().collect();
    let mail = &(args.get(1).expect("Missing email address argument"))[..];
    let username = &(args.get(2..).expect("Missing name argument").join(" "))[..];

    let connection = establish_connection();

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
