use actix_web::{web, App, HttpServer, Responder, HttpResponse, HttpRequest, Error};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::collections::HashMap;
use web3::contract::{Contract, Options};
use web3::transports::Http;
use web3::types::{Address, H160, U256};
use log::{info, error};
use serde_json::Value;
use serde_json::json;
use jsonwebtoken::{encode, decode, Header, Validation, EncodingKey, DecodingKey, TokenData};
use bcrypt::{hash, verify};
use uuid::Uuid;

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
struct Order {
    order_id: String,
    user_id: String,
    symbol: String,
    quantity: u32,
    price: u32,
    order_type: String, // Add order_type field
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct OrderRequest {
    symbol: String,
    quantity: u32,
    price: u32,
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
struct Asset {
    symbol: String,
    shares: u32,
    market_value: f64,
    average_cost: f64,
    portfolio_diversity: f64,
}

struct AppState {
    web3: web3::Web3<web3::transports::Http>,
    contract_address: Address,
    account: H160,
    secret: String,
    redis_client: redis::Client,
    // users: HashMap<String, UserState>,
    // orders: HashMap<String, Order>,
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

async fn register_user(data: web::Data<Mutex<AppState>>, user: web::Json<RegisterUser>) -> impl Responder {
    println!("Registering user: {:?}", user);

    let hashed_password = match hash(&user.password, 4) {
        Ok(h) => h,
        Err(_) => return HttpResponse::InternalServerError().body("Failed to hash password"),
    };
    let user_id = Uuid::new_v4().to_string();
    let portfolio_id = Uuid::new_v4().to_string();
    let username = user.username.clone();

    let mut con = data.lock().unwrap().redis_client.get_async_connection().await.unwrap();
    let user_state = UserState {
        user_id: user_id.clone(),
        username: username.clone(),
        password: hashed_password.clone(),
        orders: vec![],
        transactions: vec![],
        portfolio: Portfolio { 
            portfolio_id: portfolio_id.clone(), 
            total_money: 0.0, 
            assets: HashMap::new() 
        },
    };

    let user_state_json = serde_json::to_string(&user_state).unwrap();
    let _: () = con.set(username.clone(), user_state_json).await.unwrap();

    HttpResponse::Ok().json(json!({
        "user_id": user_id,
        "username": username,
        "password": hashed_password,
        "portfolio_id": portfolio_id
    }))
}

async fn login_user(data: web::Data<Mutex<AppState>>, user: web::Json<LoginUser>) -> impl Responder {
    println!("Logging in user: {:?}", user);

    let mut con = data.lock().unwrap().redis_client.get_async_connection().await.unwrap();
    let user_state_json: Option<String> = con.get(&user.username).await.unwrap();

    if let Some(user_state_json) = user_state_json {
        let user_state: UserState = serde_json::from_str(&user_state_json).unwrap();
        if verify(&user.password, &user_state.password).unwrap() {
            let my_claims = Claims { sub: user.username.clone(), exp: 10000000000 };
            let token = match encode(&Header::default(), &my_claims, &EncodingKey::from_secret(data.lock().unwrap().secret.as_ref())) {
                Ok(t) => t,
                Err(_) => return HttpResponse::InternalServerError().body("Failed to generate token"),
            };
            return HttpResponse::Ok().json(json!({
                "token": token,
                "user_id": user_state.user_id
            }));
        }
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

async fn place_buy_order(req: HttpRequest, data: web::Data<Mutex<AppState>>, order: web::Json<OrderRequest>) -> Result<HttpResponse, Error> {
    let token_data = validate_token(&req, &data.lock().unwrap().secret)?;
    let username = token_data.claims.sub;

    let mut state = data.lock().unwrap();
    
    let contract_abi: Value = serde_json::from_slice(include_bytes!("../build/contracts/OrderBook.json")).unwrap();
    let abi = contract_abi.get("abi").unwrap();
    
    let contract = Contract::from_json(state.web3.eth(), state.contract_address, abi.to_string().as_bytes()).unwrap();

    info!("Placing buy order: {:?}", order);
    
    let options = Options {
        gas: Some(3_000_000.into()),
        ..Default::default()
    };

    let result = contract.call(
        "placeBuyOrder",
        (order.symbol.clone(), U256::from(order.quantity), U256::from(order.price)),
        state.account,
        options
    ).await;

    match result {
        Ok(tx_id) => {
            info!("Buy order placed successfully");
            let order_id = Uuid::new_v4().to_string();
            let mut con = state.redis_client.get_async_connection().await.unwrap();
            let user_state_json: String = con.get(&username).await.unwrap();
            let mut user_state: UserState = serde_json::from_str(&user_state_json).unwrap();

            let new_order = Order {
                order_id: order_id.clone(),
                user_id: user_state.user_id.clone(),
                symbol: order.symbol.clone(),
                quantity: order.quantity,
                price: order.price,
                order_type: "buy".to_string(),
            };

            // Update user's portfolio
            user_state.orders.push(new_order.clone());
            user_state.transactions.push(Transaction {
                order_id: order_id.clone(),
                transaction_id: format!("{:?}", tx_id),
            });

            let asset = user_state.portfolio.assets.entry(order.symbol.clone()).or_insert(Asset {
                symbol: order.symbol.clone(),
                shares: 0,
                market_value: 0.0,
                average_cost: 0.0,
                portfolio_diversity: 0.0,
            });

            let total_cost = asset.shares as f64 * asset.average_cost;
            let new_total_cost = total_cost + order.quantity as f64 * order.price as f64;
            asset.shares += order.quantity;
            asset.average_cost = new_total_cost / asset.shares as f64;
            asset.market_value = asset.shares as f64 * order.price as f64;

            user_state.portfolio.total_money += order.quantity as f64 * order.price as f64;

            // Update portfolio diversity for each asset
            for asset in user_state.portfolio.assets.values_mut() {
                asset.portfolio_diversity = asset.market_value / user_state.portfolio.total_money;
            }

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

async fn place_sell_order(req: HttpRequest, data: web::Data<Mutex<AppState>>, order: web::Json<OrderRequest>) -> Result<HttpResponse, Error> {
    let token_data = validate_token(&req, &data.lock().unwrap().secret)?;
    let username = token_data.claims.sub;

    let mut state = data.lock().unwrap();
    
    let contract_abi: Value = serde_json::from_slice(include_bytes!("../build/contracts/OrderBook.json")).unwrap();
    let abi = contract_abi.get("abi").unwrap();
    
    let contract = Contract::from_json(state.web3.eth(), state.contract_address, abi.to_string().as_bytes()).unwrap();

    info!("Placing sell order: {:?}", order);
    
    let options = Options {
        gas: Some(3_000_000.into()),
        ..Default::default()
    };

    let result = contract.call(
        "placeSellOrder",
        (order.symbol.clone(), U256::from(order.quantity), U256::from(order.price)),
        state.account,
        options
    ).await;

    match result {
        Ok(tx_id) => {
            info!("Sell order placed successfully");
            let order_id = Uuid::new_v4().to_string();
            let mut con = state.redis_client.get_async_connection().await.unwrap();
            let user_state_json: String = con.get(&username).await.unwrap();
            let mut user_state: UserState = serde_json::from_str(&user_state_json).unwrap();

            let new_order = Order {
                order_id: order_id.clone(),
                user_id: user_state.user_id.clone(),
                symbol: order.symbol.clone(),
                quantity: order.quantity,
                price: order.price,
                order_type: "sell".to_string(),
            };

            // Update user's portfolio
            user_state.orders.push(new_order.clone());
            user_state.transactions.push(Transaction {
                order_id: order_id.clone(),
                transaction_id: format!("{:?}", tx_id),
            });

            if let Some(mut asset) = user_state.portfolio.assets.get_mut(&order.symbol).cloned() {
                if asset.shares >= order.quantity {
                    asset.shares -= order.quantity;
                    asset.market_value = asset.shares as f64 * order.price as f64;

                    user_state.portfolio.total_money -= order.quantity as f64 * order.price as f64;

                    // Update portfolio diversity for each asset
                    for asset in user_state.portfolio.assets.values_mut() {
                        asset.portfolio_diversity = asset.market_value / user_state.portfolio.total_money;
                    }

                    // Remove asset if no shares are left
                    if asset.shares == 0 {
                        user_state.portfolio.assets.remove(&order.symbol);
                    } else {
                        user_state.portfolio.assets.insert(order.symbol.clone(), asset);
                    }
                } else {
                    return Ok(HttpResponse::BadRequest().body("Not enough shares to sell"));
                }
            } else {
                return Ok(HttpResponse::BadRequest().body("Asset not found in portfolio"));
            }

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

async fn get_order_book(data: web::Data<Mutex<AppState>>) -> impl Responder {
    let mut con = data.lock().unwrap().redis_client.get_async_connection().await.unwrap();
    let order_keys: Vec<String> = con.keys("*").await.unwrap();
    let mut orders: Vec<Order> = Vec::new();

    for key in order_keys {
        if let Ok(order_json) = con.get::<String, String>(key).await {
            if let Ok(order) = serde_json::from_str::<Order>(&order_json) {
                orders.push(order);
            }
        }
    }

    HttpResponse::Ok().json(orders)
}

async fn get_order_by_id(data: web::Data<Mutex<AppState>>, order_id: web::Path<String>) -> impl Responder {
    let mut con = data.lock().unwrap().redis_client.get_async_connection().await.unwrap();
    if let Ok(order_json) = con.get::<String, String>(order_id.as_str().to_string()).await {
        if let Ok(order) = serde_json::from_str::<Order>(&order_json) {
            return HttpResponse::Ok().json(order);
        }
    }

    HttpResponse::NotFound().body("Order not found")
}

async fn get_user_orders(req: HttpRequest, data: web::Data<Mutex<AppState>>) -> Result<HttpResponse, Error> {
    let token_data = validate_token(&req, &data.lock().unwrap().secret)?;
    let username = token_data.claims.sub;

    let mut con = data.lock().unwrap().redis_client.get_async_connection().await.unwrap();
    let user_state_json: String = con.get(&username).await.unwrap();
    let user_state: UserState = serde_json::from_str(&user_state_json).unwrap();

    Ok(HttpResponse::Ok().json(user_state.orders))
}

async fn get_user_transactions(req: HttpRequest, data: web::Data<Mutex<AppState>>) -> Result<HttpResponse, Error> {
    let token_data = validate_token(&req, &data.lock().unwrap().secret)?;
    let username = token_data.claims.sub;

    let mut con = data.lock().unwrap().redis_client.get_async_connection().await.unwrap();
    let user_state_json: String = con.get(&username).await.unwrap();
    let user_state: UserState = serde_json::from_str(&user_state_json).unwrap();

    Ok(HttpResponse::Ok().json(user_state.transactions))
}

async fn get_user_portfolio(req: HttpRequest, data: web::Data<Mutex<AppState>>, user_id: web::Path<String>) -> Result<HttpResponse, Error> {
    let token_data = validate_token(&req, &data.lock().unwrap().secret)?;
    let username = token_data.claims.sub;

    let mut con = data.lock().unwrap().redis_client.get_async_connection().await.unwrap();
    let user_state_json: String = con.get(&username).await.unwrap();
    let user_state: UserState = serde_json::from_str(&user_state_json).unwrap();

    if user_state.user_id == user_id.as_str() {
        return Ok(HttpResponse::Ok().json(&user_state.portfolio));
    }

    Ok(HttpResponse::Unauthorized().body("Invalid user ID"))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let transport = Http::new("http://localhost:8545").unwrap();
    let web3 = web3::Web3::new(transport);
    let contract_address = "0x08eFcC84831174A37C4c210CBFB8bb909226DceB".parse().unwrap();
    let account: H160 = "0xcd11210A972C4ee76A1340c242F46fB74F75c95b".parse().unwrap();
    let secret = "my_secret_key".to_string(); // Use a strong, random secret in production

    let redis_client = redis::Client::open("redis://127.0.0.1/").expect("Invalid Redis URL");

    let state = web::Data::new(Mutex::new(AppState { 
        web3, 
        contract_address, 
        account, 
        secret,
        redis_client,
        // users: HashMap::new(), // Initialize user state
        // orders: HashMap::new(), // Initialize order state
    }));

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .route("/register", web::post().to(register_user))
            .route("/login", web::post().to(login_user))
            .route("/buy", web::post().to(place_buy_order))
            .route("/sell", web::post().to(place_sell_order))
            .route("/orders", web::get().to(get_user_orders))
            .route("/transactions", web::get().to(get_user_transactions))
            .route("/portfolio/{user_id}", web::get().to(get_user_portfolio))
            .route("/order_book", web::get().to(get_order_book))
            .route("/order/{order_id}", web::get().to(get_order_by_id))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await 
}
