use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use web3::types::{Address, H160, U256};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct RegisterUser {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct LoginUser {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct User {
    pub user_id: String,
    pub username: String,
    pub password: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub enum OrderType {
    Limit,
    Market,
    Stop,
}

impl OrderType {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderType::Limit => "limit",
            OrderType::Market => "market",
            OrderType::Stop => "stop",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "limit" => Some(OrderType::Limit),
            "market" => Some(OrderType::Market),
            "stop" => Some(OrderType::Stop),
            _ => None,
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Order {
    pub order_id: String,
    pub user_id: String,
    pub symbol: String,
    pub quantity: u32,
    pub price: u32,
    pub order_type: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct OrderRequest {
    pub symbol: String,
    pub quantity: u32,
    pub price: u32,
    pub order_type: OrderType,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Transaction {
    pub order_id: String,
    pub transaction_id: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Portfolio {
    pub portfolio_id: String,
    pub total_money: f64,
    pub assets: HashMap<String, Asset>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct InitializeUserRequest {
    pub username: String,
    pub password: String,
    pub total_money: f64,
    pub assets: HashMap<String, Asset>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Asset {
    pub symbol: String,
    pub shares: u32,
    pub market_value: f64,
    pub average_cost: f64,
    pub portfolio_diversity: f64,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct UserState {
    pub user_id: String,
    pub username: String,
    pub password: String,
    pub orders: Vec<Order>,
    pub transactions: Vec<Transaction>,
    pub portfolio: Portfolio,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OrderMatchedEvent {
    pub buy_order_id: U256,
    pub sell_order_id: U256,
    pub symbol: String,
    pub quantity: U256,
    pub price: U256,
    pub buyer: Address,
    pub buyer_user_id: String,
    pub buyer_order_id: String,
    pub seller: Address,
    pub seller_user_id: String,
    pub seller_order_id: String,
}
