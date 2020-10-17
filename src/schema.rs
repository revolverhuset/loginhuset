// Generated with diesel print-schema > src/schema.rs
table! {
    sessions (id) {
        id -> Integer,
        user_id -> Integer,
        token -> Text,
        created -> Timestamp,
    }
}

table! {
    users (id) {
        id -> Integer,
        email -> Text,
        name -> Text,
    }
}

joinable!(sessions -> users (user_id));

allow_tables_to_appear_in_same_query!(
    sessions,
    users,
);
