use actix_web::{web, App, HttpServer, Responder, HttpResponse, HttpRequest, Error};
use actix_web::middleware::Logger;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use web3::contract::{Contract, Options};
use web3::types::{Log, FilterBuilder, H160, Address, U256};
use log::{info, error};
use serde_json::Value;
use serde_json::json;
use jsonwebtoken::{encode, decode, Header, Validation, EncodingKey, DecodingKey, TokenData};
use bcrypt::{hash, verify};
use uuid::Uuid;
use tokio::sync::Mutex as AsyncMutex;
use ethabi;
use futures::stream::StreamExt;
use dotenv::dotenv;
use std::env;

// Redis imports
use redis::AsyncCommands;

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct RegisterUser {
    username: String,
    password: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct LoginUser {
    username: String,
    password: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct User {
    user_id: String,
    username: String,
    password: String,
}
#[derive(Deserialize, Serialize, Debug, Clone)]
enum OrderType {
    Limit,
    Market,
    Stop,
}

impl OrderType {
    fn as_str(&self) -> &'static str {
        match self {
            OrderType::Limit => "limit",
            OrderType::Market => "market",
            OrderType::Stop => "stop",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "limit" => Some(OrderType::Limit),
            "market" => Some(OrderType::Market),
            "stop" => Some(OrderType::Stop),
            _ => None,
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct Order {
    order_id: String,
    user_id: String,
    symbol: String,
    quantity: u32,
    price: u32,
    order_type: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct OrderRequest {
    symbol: String,
    quantity: u32,
    price: u32,
    order_type: OrderType,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct Transaction {
    order_id: String,
    transaction_id: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct Portfolio {
    portfolio_id: String,
    total_money: f64,
    assets: HashMap<String, Asset>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct InitializeUserRequest {
    username: String,
    password: String,
    total_money: f64,
    assets: HashMap<String, Asset>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct Asset {
    symbol: String,
    shares: u32,
    market_value: f64,
    average_cost: f64,
    portfolio_diversity: f64,
}

struct AppState {
    web3: web3::Web3<web3::transports::WebSocket>,
    contract_address: Address,
    account: H160,
    secret: String,
    redis_client: redis::Client,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct UserState {
    user_id: String,
    username: String,
    password: String,
    orders: Vec<Order>,
    transactions: Vec<Transaction>,
    portfolio: Portfolio,
}

#[derive(Debug, Serialize, Deserialize)]
struct OrderMatchedEvent {
    buy_order_id: U256,
    sell_order_id: U256,
    symbol: String,
    quantity: U256,
    price: U256,
    buyer: Address,
    buyer_user_id: String,
    buyer_order_id: String,
    seller: Address,
    seller_user_id: String,
    seller_order_id: String,
}

fn parse_log(log: Log) -> Result<OrderMatchedEvent, web3::Error> {
    let event_data = ethabi::decode(
        &[ethabi::ParamType::Uint(256), ethabi::ParamType::Uint(256), ethabi::ParamType::String, ethabi::ParamType::Uint(256), ethabi::ParamType::Uint(256), ethabi::ParamType::Address, ethabi::ParamType::String, ethabi::ParamType::String, ethabi::ParamType::Address, ethabi::ParamType::String, ethabi::ParamType::String],
        &log.data.0,
    ).map_err(|e| web3::Error::from(format!("Failed to decode event data: {}", e)))?;

    Ok(OrderMatchedEvent {
        buy_order_id: event_data[0].clone().into_uint().unwrap(),
        sell_order_id: event_data[1].clone().into_uint().unwrap(),
        symbol: event_data[2].clone().to_string(),
        quantity: event_data[3].clone().into_uint().unwrap(),
        price: event_data[4].clone().into_uint().unwrap(),
        buyer: event_data[5].clone().into_address().unwrap(),
        buyer_user_id: event_data[6].clone().to_string(),
        buyer_order_id: event_data[7].clone().to_string(),
        seller: event_data[8].clone().into_address().unwrap(),
        seller_user_id: event_data[9].clone().to_string(),
        seller_order_id: event_data[10].clone().to_string(),
    })
}

async fn handle_event(data: web::Data<AsyncMutex<AppState>>, event: OrderMatchedEvent) {
    println!("Order matched event received: {:?}", event);

    let state = data.lock().await;
    let mut con = state.redis_client.get_multiplexed_async_connection().await.unwrap();

    // Update order book
    let buy_order_key = format!("buy_order:{}", event.buy_order_id);
    let sell_order_key = format!("sell_order:{}", event.sell_order_id);

    // Remove matched orders from order book
    let _: () = con.del(buy_order_key).await.unwrap();
    let _: () = con.del(sell_order_key).await.unwrap();

    // Add matched order to order history
    let matched_order = json!({
        "buy_order_id": event.buy_order_id,
        "sell_order_id": event.sell_order_id,
        "symbol": event.symbol,
        "quantity": event.quantity.as_u64(),
        "price": event.price.as_u64(),
        "buyer": event.buyer,
        "buyer_user_id": event.buyer_user_id,
        "buyer_order_id": event.buyer_order_id,
        "seller": event.seller,
        "seller_user_id": event.seller_user_id,
        "seller_order_id": event.seller_order_id
    });

    let _: () = con.rpush("order_history", serde_json::to_string(&matched_order).unwrap()).await.unwrap();

    // Update buyer's portfolio
    if let Ok(buyer_state_json) = con.get::<String, String>(event.buyer_user_id.clone()).await {
        println!("Updating buyer's portfolio for buyer: {:?}", event.buyer);

        let mut buyer_state: UserState = serde_json::from_str(&buyer_state_json).unwrap();
        let asset = buyer_state.portfolio.assets.entry(event.symbol.clone()).or_insert(Asset {
            symbol: event.symbol.clone(),
            shares: 0,
            market_value: 0.0,
            average_cost: 0.0,
            portfolio_diversity: 0.0,
        });

        let total_cost = asset.shares as f64 * asset.average_cost;
        let new_total_cost = total_cost + event.quantity.as_u64() as f64 * event.price.as_u64() as f64;
        asset.shares += event.quantity.as_u64() as u32;
        asset.average_cost = new_total_cost / asset.shares as f64;
        asset.market_value = asset.shares as f64 * event.price.as_u64() as f64;

        // Print event quantity and price as u64
        println!("Event quantity as u64: {:?}", event.quantity.as_u64());
        println!("Event price as u64: {:?}", event.price);
        println!("Buyer money {:?}", buyer_state.portfolio.total_money);

        buyer_state.portfolio.total_money -= event.quantity.as_u64() as f64 * event.price.as_u64() as f64;

        for asset in buyer_state.portfolio.assets.values_mut() {
            asset.portfolio_diversity = asset.market_value / buyer_state.portfolio.total_money;
        }

        let updated_buyer_state_json = serde_json::to_string(&buyer_state).unwrap();
        let _: () = con.set(event.buyer_user_id.clone(), updated_buyer_state_json).await.unwrap();

        println!("Updated buyer's portfolio: {:?}", buyer_state.portfolio);
    }

    // Update seller's portfolio
    if let Ok(seller_state_json) = con.get::<String, String>(event.seller_user_id.clone()).await {
        println!("Updating seller's portfolio for seller: {:?}", event.seller);

        let mut seller_state: UserState = serde_json::from_str(&seller_state_json).unwrap();
        if let Some(mut asset) = seller_state.portfolio.assets.get_mut(&event.symbol).cloned() {
            if asset.shares >= event.quantity.as_u64() as u32 {
                asset.shares -= event.quantity.as_u64() as u32;
                asset.market_value = asset.shares as f64 * event.price.as_u64() as f64;

                // Print event quantity and price as u64
                println!("Event quantity as u64: {:?}", event.quantity.as_u64());
                println!("Event price as u64: {:?}", event.price);
                println!("Seller money {:?}", seller_state.portfolio.total_money);

                seller_state.portfolio.total_money += event.quantity.as_u64() as f64 * event.price.as_u64() as f64;

                for asset in seller_state.portfolio.assets.values_mut() {
                    asset.portfolio_diversity = asset.market_value / seller_state.portfolio.total_money;
                }

                if asset.shares == 0 {
                    seller_state.portfolio.assets.remove(&event.symbol);
                } else {
                    seller_state.portfolio.assets.insert(event.symbol.clone(), asset);
                }

                let updated_seller_state_json = serde_json::to_string(&seller_state).unwrap();
                let _: () = con.set(event.seller_user_id.clone(), updated_seller_state_json).await.unwrap();

                println!("Updated seller's portfolio: {:?}", seller_state.portfolio);
            }
        }
    }

    println!("Order matched and portfolios updated: buyer = {:?}, seller = {:?}", event.buyer, event.seller);
}

async fn listen_for_events(data: web::Data<AsyncMutex<AppState>>) {
    let transport = web3::transports::WebSocket::new("ws://localhost:8545").await.unwrap();
    let web3 = web3::Web3::new(transport);

    let contract_address: H160 = env::var("CONTRACT_ADDRESS").expect("CONTRACT_ADDRESS not set in .env file").parse().expect("Invalid contract address");
    let filter = FilterBuilder::default()
        .address(vec![contract_address])
        .build();

    let mut event_stream = web3.eth_subscribe().subscribe_logs(filter).await.unwrap();

    while let Some(log) = event_stream.next().await {
        match log {
            Ok(log) => {
                if let Ok(event) = parse_log(log) {
                    handle_event(data.clone(), event).await;
                }
            }
            Err(e) => {
                eprintln!("Error receiving log: {:?}", e);
            }
        }
    }
}

async fn register_user(data: web::Data<AsyncMutex<AppState>>, user: web::Json<RegisterUser>) -> impl Responder {
    println!("Registering user: {:?}", user);

    let hashed_password = match hash(&user.password, 4) {
        Ok(h) => h,
        Err(_) => return HttpResponse::InternalServerError().body("Failed to hash password"),
    };
    let user_id = Uuid::new_v4().to_string();
    let portfolio_id = Uuid::new_v4().to_string();
    let username = user.username.clone();
    let state = data.lock().await;

    let mut con = state.redis_client.get_multiplexed_async_connection().await.unwrap();
    let user_state = UserState {
        user_id: user_id.clone(),
        username: username.clone(),
        password: hashed_password.clone(),
        orders: vec![],
        transactions: vec![],
        portfolio: Portfolio {
            portfolio_id: portfolio_id.clone(),
            total_money: 0.0,
            assets: HashMap::new(),
        },
    };

    let user_state_json = serde_json::to_string(&user_state).unwrap();
    let _: () = con.set(&username, user_state_json).await.unwrap();

    println!("User successfully registered and saved to Redis with username: {}", username);

    HttpResponse::Ok().json(json!({
        "user_id": user_id,
        "username": username,
        "password": hashed_password,
        "portfolio_id": portfolio_id
    }))
}

async fn login_user(data: web::Data<AsyncMutex<AppState>>, user: web::Json<LoginUser>) -> impl Responder {
    println!("Logging in user: {:?}", user);

    let state = data.lock().await;
    let mut con = state.redis_client.get_multiplexed_async_connection().await.unwrap();
    let user_state_json: Option<String> = con.get(&user.username).await.unwrap();

    if let Some(user_state_json) = user_state_json {
        let user_state: UserState = serde_json::from_str(&user_state_json).unwrap();
        println!("Stored user data: {:?}", user_state);
        println!("Provided password: {:?}", user.password);

        if verify(&user.password, &user_state.password).unwrap() {
            let my_claims = Claims { sub: user.username.clone(), exp: 10000000000 };
            let token = match encode(&Header::default(), &my_claims, &EncodingKey::from_secret(state.secret.as_ref())) {
                Ok(t) => t,
                Err(_) => return HttpResponse::InternalServerError().body("Failed to generate token"),
            };
            return HttpResponse::Ok().json(json!({
                "token": token,
                "user_id": user_state.user_id
            }));
        } else {
            println!("Password verification failed");
        }
    } else {
        println!("User not found in Redis");
    }
    HttpResponse::Unauthorized().body("Invalid username or password")
}

fn validate_token(req: &HttpRequest, secret: &str) -> Result<TokenData<Claims>, Error> {
    if let Some(auth_header) = req.headers().get("Authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            let token = auth_str.trim_start_matches("Bearer ").to_string();
            return decode::<Claims>(&token, &DecodingKey::from_secret(secret.as_ref()), &Validation::default())
                .map_err(|_| actix_web::error::ErrorUnauthorized("Invalid token"));
        }
    }
    Err(actix_web::error::ErrorUnauthorized("Missing or invalid Authorization header"))
}

async fn place_buy_order(req: HttpRequest, data: web::Data<AsyncMutex<AppState>>, order: web::Json<OrderRequest>) -> Result<HttpResponse, Error> {
    let token_data = validate_token(&req, &data.lock().await.secret)?;
    let username = token_data.claims.sub;

    println!("Placing buy order for user: {}, order: {:?}", username, order);

    let state = data.lock().await;

    let contract_abi: Value = serde_json::from_slice(include_bytes!("../build/contracts/OrderBook.json")).unwrap();
    let abi = contract_abi.get("abi").unwrap();

    let contract = Contract::from_json(state.web3.eth(), state.contract_address, abi.to_string().as_bytes()).unwrap();

    let order_id = Uuid::new_v4().to_string();
    let user_id = username.clone(); // Assuming username is unique and used as user_id

    info!("Placing buy order: {:?}", order);

    // Log the parameters
    println!("Symbol: {}", order.symbol);
    println!("Quantity: {}", order.quantity);
    println!("Price: {}", order.price);
    println!("User ID: {}", user_id);
    println!("Order ID: {}", order_id);
    println!("Order Type: {:?}", order.order_type);

    let options = Options {
        gas: Some(3_000_000.into()),
        ..Default::default()
    };

    let order_type = match order.order_type {
        OrderType::Limit => U256::from(0),
        OrderType::Market => U256::from(1),
        OrderType::Stop => U256::from(2),
    };

    let result = contract.call(
        "placeBuyOrder",
        (
            order.symbol.clone(),
            U256::from(order.quantity),
            U256::from(order.price),
            user_id.clone(),
            order_id.clone(),
            order_type,
        ),
        state.account,
        options,
    ).await;

    match result {
        Ok(tx_id) => {
            info!("Buy order placed successfully: tx_id = {:?}", tx_id);
            let mut con = state.redis_client.get_multiplexed_async_connection().await.unwrap();
            let user_state_json: String = con.get(&username).await.unwrap();
            let mut user_state: UserState = serde_json::from_str(&user_state_json).unwrap();

            let new_order = Order {
                order_id: order_id.clone(),
                user_id: user_state.user_id.clone(),
                symbol: order.symbol.clone(),
                quantity: order.quantity,
                price: order.price,
                order_type: order.order_type.as_str().to_string(),
            };

            // Update user's portfolio
            user_state.orders.push(new_order.clone());
            user_state.transactions.push(Transaction {
                order_id: order_id.clone(),
                transaction_id: format!("{:?}", tx_id),
            });

            // Store the order in Redis
            let order_json = serde_json::to_string(&new_order).unwrap();
            let _: () = con.set(order_id.clone(), order_json).await.unwrap();

            // Update user state in Redis
            let user_state_json = serde_json::to_string(&user_state).unwrap();
            let _: () = con.set(&username, user_state_json).await.unwrap();

            Ok(HttpResponse::Ok().json(json!({
                "order_id": new_order.order_id,
                "user_id": new_order.user_id,
                "symbol": new_order.symbol,
                "quantity": new_order.quantity,
                "price": new_order.price,
                "order_type": new_order.order_type
            })))
        },
        Err(e) => {
            error!("Error placing buy order: {:?}", e);
            Ok(HttpResponse::InternalServerError().body("Error placing buy order"))
        },
    }
}

async fn place_sell_order(req: HttpRequest, data: web::Data<AsyncMutex<AppState>>, order: web::Json<OrderRequest>) -> Result<HttpResponse, Error> {
    let token_data = validate_token(&req, &data.lock().await.secret)?;
    let username = token_data.claims.sub;

    println!("Placing sell order for user: {}, order: {:?}", username, order);

    let state = data.lock().await;

    let contract_abi: Value = serde_json::from_slice(include_bytes!("../build/contracts/OrderBook.json")).unwrap();
    let abi = contract_abi.get("abi").unwrap();

    let contract = Contract::from_json(state.web3.eth(), state.contract_address, abi.to_string().as_bytes()).unwrap();

    let order_id = Uuid::new_v4().to_string();
    let user_id = username.clone(); // Assuming username is unique and used as user_id

    info!("Placing sell order: {:?}", order);

    // Log the parameters
    println!("Symbol: {}", order.symbol);
    println!("Quantity: {}", order.quantity);
    println!("Price: {}", order.price);
    println!("User ID: {}", user_id);
    println!("Order ID: {}", order_id);
    println!("Order Type: {:?}", order.order_type);

    let options = Options {
        gas: Some(3_000_000.into()),
        ..Default::default()
    };

    let order_type = match order.order_type {
        OrderType::Limit => U256::from(0),
        OrderType::Market => U256::from(1),
        OrderType::Stop => U256::from(2),
    };

    let result = contract.call(
        "placeSellOrder",
        (
            order.symbol.clone(),
            U256::from(order.quantity),
            U256::from(order.price),
            user_id.clone(),
            order_id.clone(),
            order_type,
        ),
        state.account,
        options,
    ).await;

    match result {
        Ok(tx_id) => {
            info!("Sell order placed successfully: tx_id = {:?}", tx_id);
            let mut con = state.redis_client.get_multiplexed_async_connection().await.unwrap();
            let user_state_json: String = con.get(&username).await.unwrap();
            let mut user_state: UserState = serde_json::from_str(&user_state_json).unwrap();

            let new_order = Order {
                order_id: order_id.clone(),
                user_id: user_state.user_id.clone(),
                symbol: order.symbol.clone(),
                quantity: order.quantity,
                price: order.price,
                order_type: order.order_type.as_str().to_string(),
            };

            // Update user's portfolio
            user_state.orders.push(new_order.clone());
            user_state.transactions.push(Transaction {
                order_id: order_id.clone(),
                transaction_id: format!("{:?}", tx_id),
            });

            // Store the order in Redis
            let order_json = serde_json::to_string(&new_order).unwrap();
            let _: () = con.set(order_id.clone(), order_json).await.unwrap();

            // Update user state in Redis
            let user_state_json = serde_json::to_string(&user_state).unwrap();
            let _: () = con.set(&username, user_state_json).await.unwrap();

            Ok(HttpResponse::Ok().json(json!({
                "order_id": new_order.order_id,
                "user_id": new_order.user_id,
                "symbol": new_order.symbol,
                "quantity": new_order.quantity,
                "price": new_order.price,
                "order_type": new_order.order_type
            })))
        },
        Err(e) => {
            error!("Error placing sell order: {:?}", e);
            Ok(HttpResponse::InternalServerError().body("Error placing sell order"))
        },
    }
}

async fn get_user_orders(req: HttpRequest, data: web::Data<AsyncMutex<AppState>>, user_id: web::Path<String>) -> Result<HttpResponse, Error> {
    let token_data = validate_token(&req, &data.lock().await.secret)?;
    let username = token_data.claims.sub;

    println!("Fetching orders for user: {}", user_id);

    let mut con = data.lock().await.redis_client.get_multiplexed_async_connection().await.unwrap();
    let user_state_json: String = con.get(&username).await.unwrap();
    let user_state: UserState = serde_json::from_str(&user_state_json).unwrap();

    if user_state.user_id == user_id.as_str() {
        return Ok(HttpResponse::Ok().json(&user_state.orders));
    }

    Ok(HttpResponse::Unauthorized().body("Invalid user ID"))
}

async fn get_order_by_id(req: HttpRequest, data: web::Data<AsyncMutex<AppState>>, order_id: web::Path<String>) -> Result<HttpResponse, Error> {
    let _token_data = validate_token(&req, &data.lock().await.secret)?;

    println!("Fetching order by ID: {}", order_id);

    let mut con = data.lock().await.redis_client.get_multiplexed_async_connection().await.unwrap();
    if let Ok(order_json) = con.get::<String, String>(order_id.to_string()).await {
        if let Ok(order) = serde_json::from_str::<Order>(&order_json) {
            return Ok(HttpResponse::Ok().json(order));
        }
    }

    Ok(HttpResponse::NotFound().body("Order not found"))
}

async fn get_user_portfolio(req: HttpRequest, data: web::Data<AsyncMutex<AppState>>, user_id: web::Path<String>) -> Result<HttpResponse, Error> {
    let token_data = validate_token(&req, &data.lock().await.secret)?;
    let username = token_data.claims.sub;

    println!("Fetching portfolio for user: {}", user_id);

    let mut con = data.lock().await.redis_client.get_multiplexed_async_connection().await.unwrap();
    let user_state_json: String = con.get(&username).await.unwrap();
    let user_state: UserState = serde_json::from_str(&user_state_json).unwrap();

    if user_state.user_id == user_id.as_str() {
        return Ok(HttpResponse::Ok().json(&user_state.portfolio));
    }

    Ok(HttpResponse::Unauthorized().body("Invalid user ID"))
}

async fn get_portfolio_by_id(req: HttpRequest, data: web::Data<AsyncMutex<AppState>>, portfolio_id: web::Path<String>) -> Result<HttpResponse, Error> {
    let _token_data = validate_token(&req, &data.lock().await.secret)?;

    println!("Fetching portfolio by ID: {}", portfolio_id);

    let mut con = data.lock().await.redis_client.get_multiplexed_async_connection().await.unwrap();
    let keys: Vec<String> = con.keys("*").await.unwrap();
    
    for key in keys {
        if let Ok(user_state_json) = con.get::<String, String>(key.clone()).await {
            if let Ok(user_state) = serde_json::from_str::<UserState>(&user_state_json) {
                if user_state.portfolio.portfolio_id == portfolio_id.as_str() {
                    return Ok(HttpResponse::Ok().json(user_state.portfolio));
                }
            }
        }
    }

    Ok(HttpResponse::NotFound().body("Portfolio not found"))
}

async fn get_order_book(req: HttpRequest, data: web::Data<AsyncMutex<AppState>>) -> Result<HttpResponse, Error> {
    let _token_data = validate_token(&req, &data.lock().await.secret)?;

    println!("Fetching order book");

    let mut con = data.lock().await.redis_client.get_multiplexed_async_connection().await.unwrap();
    let order_keys: Vec<String> = con.keys("*").await.unwrap();
    let mut orders: Vec<Order> = Vec::new();

    for key in order_keys {
        if let Ok(order_json) = con.get::<String, String>(key).await {
            if let Ok(order) = serde_json::from_str::<Order>(&order_json) {
                orders.push(order);
            }
        }
    }

    Ok(HttpResponse::Ok().json(orders))
}

async fn get_user_transactions(req: HttpRequest, data: web::Data<AsyncMutex<AppState>>) -> Result<HttpResponse, Error> {
    let token_data = validate_token(&req, &data.lock().await.secret)?;
    let username = token_data.claims.sub;

    println!("Fetching all transactions from the blockchain");

    let mut con = data.lock().await.redis_client.get_multiplexed_async_connection().await.unwrap();
    let user_state_json: String = con.get(&username).await.unwrap();
    let user_state: UserState = serde_json::from_str(&user_state_json).unwrap();

    Ok(HttpResponse::Ok().json(user_state.transactions))
}

async fn get_all_users(data: web::Data<AsyncMutex<AppState>>) -> Result<HttpResponse, Error> {
    let mut con = data.lock().await.redis_client.get_multiplexed_async_connection().await.unwrap();
    let keys: Vec<String> = con.keys("*").await.unwrap();

    let mut users = Vec::new();
    for key in keys {
        if let Ok(user_json) = con.get::<String, String>(key).await {
            if let Ok(user) = serde_json::from_str::<UserState>(&user_json) {
                users.push(user);
            }
        }
    }

    println!("Getting {} users", users.len());

    Ok(HttpResponse::Ok().json(json!({
        "number_of_users": users.len(),
        "users": users,
    })))
}

async fn get_all_portfolios(data: web::Data<AsyncMutex<AppState>>) -> Result<HttpResponse, Error> {
    let mut con = data.lock().await.redis_client.get_multiplexed_async_connection().await.unwrap();
    let keys: Vec<String> = con.keys("*").await.unwrap();

    let mut portfolios = Vec::new();
    for key in keys {
        if let Ok(user_state_json) = con.get::<String, String>(key).await {
            if let Ok(user_state) = serde_json::from_str::<UserState>(&user_state_json) {
                portfolios.push(user_state.portfolio);
            }
        }
    }

    println!("Getting {} portfolios", portfolios.len());

    Ok(HttpResponse::Ok().json(json!({
        "number_of_portfolios": portfolios.len(),
        "portfolios": portfolios,
    })))
}

async fn initialize_user(data: web::Data<AsyncMutex<AppState>>, user: web::Json<InitializeUserRequest>) -> impl Responder {
    println!("Initializing user: {:?}", user);

    let hashed_password = match hash(&user.password, 4) {
        Ok(h) => h,
        Err(_) => return HttpResponse::InternalServerError().body("Failed to hash password"),
    };
    let user_id = Uuid::new_v4().to_string();
    let portfolio_id = Uuid::new_v4().to_string();
    let username = user.username.clone();
    let state = data.lock().await;

    let mut con = state.redis_client.get_multiplexed_async_connection().await.unwrap();
    let user_state = UserState {
        user_id: user_id.clone(),
        username: username.clone(),
        password: hashed_password.clone(),
        orders: vec![],
        transactions: vec![],
        portfolio: Portfolio {
            portfolio_id: portfolio_id.clone(),
            total_money: user.total_money,
            assets: user.assets.clone(),
        },
    };

    let user_state_json = serde_json::to_string(&user_state).unwrap();
    let _: () = con.set(&username, user_state_json).await.unwrap();

    println!("User successfully initialized and saved to Redis with username: {}", username);

    HttpResponse::Ok().json(user_state)
}

async fn delete_all_data(data: web::Data<AsyncMutex<AppState>>) -> Result<HttpResponse, Error> {
    println!("Deleting all data");

    let mut con = data.lock().await.redis_client.get_multiplexed_async_connection().await.unwrap();
    let _: () = redis::cmd("FLUSHALL").query_async(&mut con).await.unwrap();
    
    Ok(HttpResponse::Ok().body("All data deleted"))
}

async fn delete_all_users(data: web::Data<AsyncMutex<AppState>>) -> Result<HttpResponse, Error> {
    let mut con = data.lock().await.redis_client.get_multiplexed_async_connection().await.unwrap();
    let keys: Vec<String> = con.keys("user:*").await.unwrap();
    if !keys.is_empty() {
        let _: () = con.del(keys.clone()).await.unwrap();
    }
    
    println!("Deleting {} users", keys.len());

    Ok(HttpResponse::Ok().body("All users deleted"))
}

async fn delete_all_orders(data: web::Data<AsyncMutex<AppState>>) -> Result<HttpResponse, Error> {
    let mut con = data.lock().await.redis_client.get_multiplexed_async_connection().await.unwrap();
    let keys: Vec<String> = con.keys("order:*").await.unwrap();
    if !keys.is_empty() {
        let _: () = con.del(keys.clone()).await.unwrap();
    }

    println!("Deleting {} orders", keys.len());
    
    Ok(HttpResponse::Ok().body("All orders deleted"))
}

async fn delete_all_portfolios(data: web::Data<AsyncMutex<AppState>>) -> Result<HttpResponse, Error> {
    let mut con = data.lock().await.redis_client.get_multiplexed_async_connection().await.unwrap();
    let keys: Vec<String> = con.keys("portfolio:*").await.unwrap();
    if !keys.is_empty() {
        let _: () = con.del(keys.clone()).await.unwrap();
    }
    
    println!("Deleting {} portfolios", keys.len());

    Ok(HttpResponse::Ok().body("All portfolios deleted"))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

    let ws_url = env::var("WS_URL").expect("WS_URL not set in .env file");
    let contract_address: H160 = env::var("CONTRACT_ADDRESS").expect("CONTRACT_ADDRESS not set in .env file").parse().expect("Invalid contract address");
    let account: H160 = env::var("ACCOUNT_ADDRESS").expect("ACCOUNT_ADDRESS not set in .env file").parse().expect("Invalid account address");
    let secret = env::var("SECRET_KEY").expect("SECRET_KEY not set in .env file");
    let redis_url = env::var("REDIS_CLIENT_URL").expect("REDIS_CLIENT_URL not set in .env file");

    let transport = web3::transports::WebSocket::new(&ws_url).await.unwrap();
    let web3 = web3::Web3::new(transport);
    let redis_client = redis::Client::open(redis_url).expect("Invalid Redis URL");

    let state = web::Data::new(AsyncMutex::new(AppState { 
        web3: web3.clone(), 
        contract_address, 
        account, 
        secret,
        redis_client,
    }));

    let listen_data = state.clone();
    tokio::spawn(async move {
        listen_for_events(listen_data).await;
    });

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .wrap(Logger::default())

            // Application routes
            .route("/register", web::post().to(register_user))
            .route("/login", web::post().to(login_user))
            .route("/buy", web::post().to(place_buy_order))
            .route("/sell", web::post().to(place_sell_order))
            .route("/order/user/{user_id}", web::get().to(get_user_orders))
            .route("/order/id/{order_id}", web::get().to(get_order_by_id))
            .route("/portfolio/user/{user_id}", web::get().to(get_user_portfolio))
            .route("/portfolio/id/{portfolio_id}", web::get().to(get_portfolio_by_id))
            .route("/transactions", web::get().to(get_user_transactions))

            // Utility routes
            .route("utils/get/users", web::get().to(get_all_users))
            .route("utils/get/orders", web::get().to(get_order_book))
            .route("utils/get/portfolios", web::get().to(get_all_portfolios))
            .route("utils/post/initialize_user", web::post().to(initialize_user))
            .route("utils/delete/all_data", web::delete().to(delete_all_data))
            .route("utils/delete/users", web::delete().to(delete_all_users))
            .route("utils/delete/orders", web::delete().to(delete_all_orders))
            .route("utils/delete/portfolios", web::delete().to(delete_all_portfolios))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}