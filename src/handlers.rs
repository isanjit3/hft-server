use actix_web::{web, HttpResponse, HttpRequest, Responder, Error};
use bcrypt::{hash, verify};
use jsonwebtoken::{encode, Header, EncodingKey, decode, DecodingKey, Validation, TokenData};
use serde_json::json;
use uuid::Uuid;
use log::{info, error};
use redis::AsyncCommands;
use std::env;
use tokio::sync::Mutex as AsyncMutex;

use crate::models::*;
use crate::state::AppState; // Assuming AppState is defined in state.rs

// Import other necessary dependencies
use web3::contract::{Contract, Options};
use web3::types::{H160, U256, Value};

// Function to validate JWT token
pub fn validate_token(req: &HttpRequest, secret: &str) -> Result<TokenData<Claims>, Error> {
    if let Some(auth_header) = req.headers().get("Authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            let token = auth_str.trim_start_matches("Bearer ").to_string();
            return decode::<Claims>(&token, &DecodingKey::from_secret(secret.as_ref()), &Validation::default())
                .map_err(|_| actix_web::error::ErrorUnauthorized("Invalid token"));
        }
    }
    Err(actix_web::error::ErrorUnauthorized("Missing or invalid Authorization header"))
}

pub async fn register_user(
    data: web::Data<AsyncMutex<AppState>>,
    user: web::Json<RegisterUser>
) -> impl Responder {
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

pub async fn login(
    data: web::Data<AsyncMutex<AppState>>,
    user: web::Json<LoginUser>
) -> impl Responder {
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

pub async fn signout(req: HttpRequest) -> impl Responder {
    // Invalidate the token or clear the client-side storage of the token
    // Since JWT is stateless, just a response indicating the sign-out is enough
    println!("User signed out");
    HttpResponse::Ok().json(json!({
        "message": "Successfully signed out"
    }))
}

// Similarly, move other handler functions here...

pub async fn place_buy_order(
    req: HttpRequest,
    data: web::Data<AsyncMutex<AppState>>,
    order: web::Json<OrderRequest>
) -> Result<HttpResponse, Error> {
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

// Similarly, implement other handler functions like place_sell_order, get_user_orders, etc.
