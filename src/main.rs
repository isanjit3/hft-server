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
use dotenv::dotenv;
use std::env;

mod schema;
mod models;

// Diesel imports
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel::RunQueryDsl;

// Model imports
use models::{User, NewUser, Order, NewOrder};

// Schema imports
use schema::users::dsl::users;
use schema::orders::dsl::orders;
use schema::users::dsl::{username as db_username};

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

struct AppState {
    web3: web3::Web3<web3::transports::Http>,
    contract_address: Address,
    account: H160,
    secret: String,
    db_pool: web::Data<Mutex<SqliteConnection>>,
    users: HashMap<String, UserState>,  // Include users field
    orders: HashMap<String, Order>,  // Include orders field
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct UserState {
    user_id: String,
    orders: Vec<Order>,
    transactions: Vec<Transaction>,
    portfolio: Portfolio,
}

/*
#[derive(Deserialize, Serialize, Debug, Clone)]
struct Order {
    order_id: String,
    user_id: String,
    symbol: String,
    quantity: u32,
    price: u32,
    order_type: String, // Add order_type field
}*/

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

pub fn establish_connection() -> SqliteConnection {
    dotenv().ok();
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    SqliteConnection::establish(&database_url)
        .expect(&format!("Error connecting to {}", database_url))
}

async fn register_user(data: web::Data<Mutex<AppState>>, user: web::Json<RegisterUser>) -> Result<HttpResponse, actix_web::Error> {
    println!("Registering user: {:?}", user);

    let hashed_password = match hash(&user.password, 4) {
        Ok(h) => h,
        Err(_) => return Ok(HttpResponse::InternalServerError().body("Failed to hash password")),
    };
    let user_id = Uuid::new_v4().to_string();
    let portfolio_id = Uuid::new_v4().to_string();
    let username = user.username.clone();

    let state = data.lock().unwrap();
    let mut conn = state.db_pool.lock().unwrap();

    let new_user = NewUser {
        user_id: user_id.clone(),
        username: user.username.clone(),
        password: hashed_password.clone(),
        portfolio_id: portfolio_id.clone(),
    };

    diesel::insert_into(users)
        .values(&new_user)
        .execute(&mut *conn)
        .map_err(|e| {
            error!("Error saving new user: {:?}", e);
            actix_web::error::ErrorInternalServerError("Error saving new user")
        })?;

    Ok(HttpResponse::Ok().json(json!({
        "user_id": user_id,
        "username": username,
        "password": hashed_password,
        "portfolio_id": portfolio_id
    })))
}

async fn login_user(data: web::Data<Mutex<AppState>>, user: web::Json<LoginUser>) -> impl Responder {
    println!("Logging in user: {:?}", user);

    let state = data.lock().unwrap();
    let mut conn = state.db_pool.lock().unwrap();
    
    use schema::users::dsl::{users, username as db_username};

    let user_in_db = users
        .filter(db_username.eq(&user.username))
        .select((schema::users::id, schema::users::user_id, schema::users::username, schema::users::password, schema::users::portfolio_id))
        .first::<(i32, String, String, String, String)>(&mut *conn);

    if let Ok((id, user_id, username, password, portfolio_id)) = user_in_db {
        if verify(&user.password, &password).unwrap() {
            let my_claims = Claims { sub: user.username.clone(), exp: 10000000000 };
            let token = match encode(&Header::default(), &my_claims, &EncodingKey::from_secret(state.secret.as_ref())) {
                Ok(t) => t,
                Err(_) => return HttpResponse::InternalServerError().body("Failed to generate token"),
            };
            return HttpResponse::Ok().json(json!({
                "token": token,
                "user_id": user_id
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

async fn place_buy_order(req: HttpRequest, data: web::Data<Mutex<AppState>>, order_req: web::Json<OrderRequest>) -> Result<HttpResponse, Error> {
    let token_data = validate_token(&req, &data.lock().unwrap().secret)?;
    let username = token_data.claims.sub;

    let state = data.lock().unwrap();
    let mut conn = state.db_pool.lock().unwrap();
    
    let contract_abi: Value = serde_json::from_slice(include_bytes!("../build/contracts/OrderBook.json")).unwrap();
    let abi = contract_abi.get("abi").unwrap();
    
    let contract = Contract::from_json(state.web3.eth(), state.contract_address, abi.to_string().as_bytes()).unwrap();

    info!("Placing buy order: {:?}", order_req);
    
    let options = Options {
        gas: Some(3_000_000.into()),
        ..Default::default()
    };

    let result = contract.call(
        "placeBuyOrder",
        (order_req.symbol.clone(), U256::from(order_req.quantity), U256::from(order_req.price)),
        state.account,
        options
    ).await;

    match result {
        Ok(tx_id) => {
            info!("Buy order placed successfully");
            let (user_id, user_id_str): (i32, String) = users.filter(db_username.eq(&username))
                .select((schema::users::id, schema::users::user_id))
                .first(&mut *conn)
                .unwrap();
            
            let order_id = Uuid::new_v4().to_string();
            let new_order = NewOrder {
                order_id: order_id.clone(),
                user_id,
                symbol: order_req.symbol.clone(),
                quantity: order_req.quantity as i32,
                price: order_req.price as i32,
                order_type: "buy".to_string(),
            };

            diesel::insert_into(orders)
                .values(&new_order)
                .execute(&mut *conn)
                .expect("Error saving new order");

            Ok(HttpResponse::Ok().json(json!({
                "order_id": new_order.order_id,
                "user_id": user_id_str,
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

async fn place_sell_order(req: HttpRequest, data: web::Data<Mutex<AppState>>, order_req: web::Json<OrderRequest>) -> Result<HttpResponse, Error> {
    let token_data = validate_token(&req, &data.lock().unwrap().secret)?;
    let username = token_data.claims.sub;

    let state = data.lock().unwrap();
    let mut conn = state.db_pool.lock().unwrap();
    
    let contract_abi: Value = serde_json::from_slice(include_bytes!("../build/contracts/OrderBook.json")).unwrap();
    let abi = contract_abi.get("abi").unwrap();
    
    let contract = Contract::from_json(state.web3.eth(), state.contract_address, abi.to_string().as_bytes()).unwrap();

    info!("Placing sell order: {:?}", order_req);
    
    let options = Options {
        gas: Some(3_000_000.into()),
        ..Default::default()
    };

    let result = contract.call(
        "placeSellOrder",
        (order_req.symbol.clone(), U256::from(order_req.quantity), U256::from(order_req.price)),
        state.account,
        options
    ).await;

    match result {
        Ok(tx_id) => {
            info!("Sell order placed successfully");
            let (user_id, user_id_str): (i32, String) = users.filter(db_username.eq(&username))
                .select((schema::users::id, schema::users::user_id))
                .first(&mut *conn)
                .unwrap();
            
            let order_id = Uuid::new_v4().to_string();
            let new_order = NewOrder {
                order_id: order_id.clone(),
                user_id,
                symbol: order_req.symbol.clone(),
                quantity: order_req.quantity as i32,
                price: order_req.price as i32,
                order_type: "sell".to_string(),
            };

            diesel::insert_into(orders)
                .values(&new_order)
                .execute(&mut *conn)
                .expect("Error saving new order");

            Ok(HttpResponse::Ok().json(json!({
                "order_id": new_order.order_id,
                "user_id": user_id_str,
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
    let mut conn = state.db_pool.lock().unwrap();

    let order_list = orders
        .select(Order::as_select())
        .load::<Order>(&mut *conn)
        .expect("Error loading orders");

    HttpResponse::Ok().json(order_list)
}

async fn get_order_by_id(data: web::Data<Mutex<AppState>>, order_id: web::Path<String>) -> impl Responder {
    let state = data.lock().unwrap();
    let mut conn = state.db_pool.lock().unwrap();

    use schema::orders::dsl::{orders, order_id as db_order_id};

    match orders.filter(db_order_id.eq(order_id.as_str()))
        .first::<Order>(&mut *conn)
    {
        Ok(order) => HttpResponse::Ok().json(order),
        Err(_) => HttpResponse::NotFound().body("Order not found"),
    }
}


async fn get_user_orders(req: HttpRequest, data: web::Data<Mutex<AppState>>, user_id: web::Path<String>) -> Result<HttpResponse, Error> {
    let token_data = validate_token(&req, &data.lock().unwrap().secret)?;
    let username = token_data.claims.sub;

    let state = data.lock().unwrap();
    let mut conn = state.db_pool.lock().unwrap();

    // Ensure the authenticated user matches the requested user_id
    let user_in_db = users
        .filter(db_username.eq(&username))
        .select(schema::users::user_id)
        .first::<String>(&mut *conn)
        .map_err(|_| actix_web::error::ErrorUnauthorized("Unauthorized user"))?;

    if &user_in_db != user_id.as_str() {
        return Ok(HttpResponse::Unauthorized().body("Unauthorized user"));
    }

    let user_id_int: i32 = users
        .filter(db_username.eq(&username))
        .select(schema::users::id)
        .first(&mut *conn)
        .map_err(|e| {
            error!("Error loading user: {:?}", e);
            actix_web::error::ErrorInternalServerError("Error loading user")
        })?;

    let user_orders = orders
        .filter(schema::orders::user_id.eq(user_id_int))
        .select(Order::as_select())
        .load::<Order>(&mut *conn)
        .map_err(|e| {
            error!("Error loading orders: {:?}", e);
            actix_web::error::ErrorInternalServerError("Error loading orders")
        })?;

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
    let contract_address = "0xb986ecb378A372f45971797FC51bBc1F730412b8".parse().unwrap();
    let account: H160 = "0x19105E925981210540A70F15116eF59a595a6f2b".parse().unwrap();
    let secret = "my_secret_key".to_string(); // Use a strong, random secret in production

    let db_conn = establish_connection();
    let state = web::Data::new(Mutex::new(AppState { 
        web3, 
        contract_address, 
        account, 
        secret,
        db_pool: web::Data::new(Mutex::new(db_conn)),
        users: HashMap::new(),  // Initialize user state
        orders: HashMap::new(),  // Initialize order state
    }));

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .route("/register", web::post().to(register_user))
            .route("/login", web::post().to(login_user))
            .route("/buy", web::post().to(place_buy_order))
            .route("/sell", web::post().to(place_sell_order))
            .route("/orders/{user_id}", web::get().to(get_user_orders))
            .route("/transactions", web::get().to(get_user_transactions))
            .route("/portfolio/{user_id}", web::get().to(get_user_portfolio))
            .route("/order_book", web::get().to(get_order_book))
            .route("/order/{order_id}", web::get().to(get_order_by_id))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await    
}
