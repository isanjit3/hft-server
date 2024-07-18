use serde::{Deserialize, Serialize};
use diesel::prelude::*;
use diesel::Queryable;
use diesel::Insertable;
use crate::schema::{users, orders};
use diesel::sqlite::Sqlite;

#[derive(Queryable, Serialize, Deserialize, Debug, Identifiable, Selectable)]
#[diesel(table_name = users)]
pub struct User {
    pub id: i32,  // Primary key, not optional for querying
    pub user_id: String,
    pub username: String,
    pub password: String,
    pub portfolio_id: String,
}

#[derive(Insertable, Serialize, Deserialize, Debug)]
#[diesel(table_name = users)]
pub struct NewUser {
    pub user_id: String,
    pub username: String,
    pub password: String,
    pub portfolio_id: String,
}

#[derive(Queryable, Serialize, Deserialize, Debug, Clone, Identifiable, Selectable)]
#[diesel(table_name = orders)]
#[diesel(check_for_backend(Sqlite))]
pub struct Order {
    pub id: i32,  // Primary key, not optional for querying
    pub order_id: String,
    pub user_id: i32,
    pub symbol: String,
    pub quantity: i32,
    pub price: i32,
    pub order_type: String,
}

#[derive(Insertable, Serialize, Deserialize, Debug, Clone)]
#[diesel(table_name = orders)]
pub struct NewOrder {
    pub order_id: String,
    pub user_id: i32,
    pub symbol: String,
    pub quantity: i32,
    pub price: i32,
    pub order_type: String,
}