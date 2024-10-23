mod models;
mod handlers;
mod events;
mod state;

use actix_web::{web, App, HttpServer};
use actix_web::middleware::Logger;
use dotenv::dotenv;
use env_logger;
use std::env;
use tokio::sync::Mutex as AsyncMutex;

use state::AppState;
use handlers::*;
use events::listen_for_events;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

    let ws_url = env::var("WS_URL").expect("WS_URL not set in .env file");
    let contract_address: H160 = env::var("CONTRACT_ADDRESS").expect("CONTRACT_ADDRESS not set in .env file")
        .parse().expect("Invalid contract address");
    let account: H160 = env::var("ACCOUNT_ADDRESS").expect("ACCOUNT_ADDRESS not set in .env file")
        .parse().expect("Invalid account address");
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
            .route("/login", web::post().to(login))
            .route("/signout", web::post().to(signout))
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
