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
    assets: Vec<Asset>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct Asset {
    symbol: String,
    quantity: u32,
    average_cost: f64,
    total_value: f64,
}

struct AppState {
    web3: web3::Web3<web3::transports::Http>,
    contract_address: Address,
    account: H160,
    secret: String,
    users: HashMap<String, UserState>,
    orders: HashMap<String, Order>,
}

struct UserState {
    user_id: String,
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
    let username = user.username.clone(); // Ensure we clone the username
    let mut state = data.lock().unwrap();
    state.users.insert(username.clone(), UserState {
        user_id: user_id.clone(),
        orders: vec![],
        transactions: vec![],
        portfolio: Portfolio { assets: vec![] },
    });
    HttpResponse::Ok().json(json!({
        "user_id": user_id,
        "username": username,
        "password": hashed_password,
    }))
}

async fn login_user(data: web::Data<Mutex<AppState>>, user: web::Json<LoginUser>) -> impl Responder {
    println!("Logging in user: {:?}", user);

    let state = data.lock().unwrap();
    if let Some(_user_state) = state.users.get(&user.username) {
        let hashed_password = match hash(&user.password, 4) {
            Ok(h) => h,
            Err(_) => return HttpResponse::InternalServerError().body("Failed to hash password"),
        };
        if verify(&user.password, &hashed_password).unwrap() {
            let my_claims = Claims { sub: user.username.clone(), exp: 10000000000 };
            let token = match encode(&Header::default(), &my_claims, &EncodingKey::from_secret(state.secret.as_ref())) {
                Ok(t) => t,
                Err(_) => return HttpResponse::InternalServerError().body("Failed to generate token"),
            };
            return HttpResponse::Ok().json(token);
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
            let new_order = Order {
                order_id: order_id.clone(),
                user_id: state.users.get(&username).unwrap().user_id.clone(),
                symbol: order.symbol.clone(),
                quantity: order.quantity,
                price: order.price,
                order_type: "buy".to_string(),
            };

            // Separate mutable borrows
            {
                let user_state = state.users.get_mut(&username).unwrap();
                user_state.orders.push(new_order.clone());
                user_state.transactions.push(Transaction {
                    order_id: order_id.clone(),
                    transaction_id: format!("{:?}", tx_id),
                });
            }

            state.orders.insert(order_id.clone(), new_order.clone());
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
            let new_order = Order {
                order_id: order_id.clone(),
                user_id: state.users.get(&username).unwrap().user_id.clone(),
                symbol: order.symbol.clone(),
                quantity: order.quantity,
                price: order.price,
                order_type: "sell".to_string(),
            };

            // Separate mutable borrows
            {
                let user_state = state.users.get_mut(&username).unwrap();
                user_state.orders.push(new_order.clone());
                user_state.transactions.push(Transaction {
                    order_id: order_id.clone(),
                    transaction_id: format!("{:?}", tx_id),
                });
            }

            state.orders.insert(order_id.clone(), new_order.clone());
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
    let state = data.lock().unwrap();
    let orders: Vec<&Order> = state.orders.values().collect();
    HttpResponse::Ok().json(orders)
}

async fn get_order_by_id(data: web::Data<Mutex<AppState>>, order_id: web::Path<String>) -> impl Responder {
    let state = data.lock().unwrap();
    if let Some(order) = state.orders.get(order_id.as_str()) {
        HttpResponse::Ok().json(order)
    } else {
        HttpResponse::NotFound().body("Order not found")
    }
}

async fn get_user_orders(req: HttpRequest, data: web::Data<Mutex<AppState>>) -> Result<HttpResponse, Error> {
    let token_data = validate_token(&req, &data.lock().unwrap().secret)?;
    let username = token_data.claims.sub;

    let state = data.lock().unwrap();
    let user_orders = &state.users.get(&username).unwrap().orders;
    Ok(HttpResponse::Ok().json(user_orders))
}

async fn get_user_transactions(req: HttpRequest, data: web::Data<Mutex<AppState>>) -> Result<HttpResponse, Error> {
    let token_data = validate_token(&req, &data.lock().unwrap().secret)?;
    let username = token_data.claims.sub;

    let state = data.lock().unwrap();
    let user_transactions = &state.users.get(&username).unwrap().transactions;
    Ok(HttpResponse::Ok().json(user_transactions))
}

async fn get_user_portfolio(req: HttpRequest, data: web::Data<Mutex<AppState>>, user_id: web::Path<String>) -> Result<HttpResponse, Error> {
    let token_data = validate_token(&req, &data.lock().unwrap().secret)?;
    let username = token_data.claims.sub;

    let state = data.lock().unwrap();
    if let Some(user_state) = state.users.get(&username) {
        if user_state.user_id == user_id.as_str() {
            return Ok(HttpResponse::Ok().json(&user_state.portfolio));
        }
    }
    Ok(HttpResponse::Unauthorized().body("Invalid user ID"))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let transport = Http::new("http://localhost:8545").unwrap();
    let web3 = web3::Web3::new(transport);
    let contract_address = "0x58D2D2989ca205e3a518Cf950591E50730FBe002".parse().unwrap();
    let account: H160 = "0xCa8161eab569Fe05EC3cA61E207aC0fB73C1Bebb".parse().unwrap();
    let secret = "my_secret_key".to_string(); // Use a strong, random secret in production

    let state = web::Data::new(Mutex::new(AppState { 
        web3, 
        contract_address, 
        account, 
        secret,
        users: HashMap::new(), // Initialize user state
        orders: HashMap::new(), // Initialize order state
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
