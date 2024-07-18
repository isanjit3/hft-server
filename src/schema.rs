// @generated automatically by Diesel CLI.
use diesel::{joinable, allow_tables_to_appear_in_same_query};

diesel::table! {
    users (id) {
        id -> Integer,
        user_id -> Text,
        username -> Text,
        password -> Text,
        portfolio_id -> Text,
    }
}

diesel::table! {
    orders (id) {
        id -> Integer,
        order_id -> Text,
        user_id -> Integer,
        symbol -> Text,
        quantity -> Integer,
        price -> Integer,
        order_type -> Text,
    }
}

joinable!(orders -> users (user_id));
allow_tables_to_appear_in_same_query!(users, orders);
