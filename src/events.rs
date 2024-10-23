use web3::types::{Log, Address, U256};
use serde_json::json;
use redis::AsyncCommands;
use log::{info, error};
use ethabi;
use crate::models::OrderMatchedEvent;
use crate::state::AppState;
use crate::models::{UserState, Order, Transaction, Portfolio, Asset};
use serde_json::Value;
use tokio::sync::Mutex as AsyncMutex;

pub fn parse_log(log: Log) -> Result<OrderMatchedEvent, web3::Error> {
    let event_data = ethabi::decode(
        &[
            ethabi::ParamType::Uint(256),
            ethabi::ParamType::Uint(256),
            ethabi::ParamType::String,
            ethabi::ParamType::Uint(256),
            ethabi::ParamType::Uint(256),
            ethabi::ParamType::Address,
            ethabi::ParamType::String,
            ethabi::ParamType::String,
            ethabi::ParamType::Address,
            ethabi::ParamType::String,
            ethabi::ParamType::String,
        ],
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

pub async fn handle_event(data: web::Data<AsyncMutex<AppState>>, event: OrderMatchedEvent) {
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

pub async fn listen_for_events(data: web::Data<AsyncMutex<AppState>>) {
    let transport = web3::transports::WebSocket::new("ws://localhost:8545").await.unwrap();
    let web3 = web3::Web3::new(transport);

    let contract_address: H160 = env::var("CONTRACT_ADDRESS").expect("CONTRACT_ADDRESS not set in .env file").parse().expect("Invalid contract address");
    let filter = ethabi::FilterBuilder::default()
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
