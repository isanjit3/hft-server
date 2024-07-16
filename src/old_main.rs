use actix_web::{web, App, HttpServer, Responder, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

#[derive(Deserialize, Serialize, Debug, Clone)]
struct Order {
    symbol: String,
    quantity: u32,
    price: f64,
}

#[derive(Serialize, Debug)]
struct OrderBook {
    buy_orders: Vec<Order>,
    sell_orders: Vec<Order>,
}

impl OrderBook {
    fn new() -> Self {
        OrderBook {
            buy_orders: Vec::new(),
            sell_orders: Vec::new(),
        }
    }

    fn sort_orders(&mut self) {
        self.buy_orders.sort_by(|a, b| b.price.partial_cmp(&a.price).unwrap());
        self.sell_orders.sort_by(|a, b| a.price.partial_cmp(&b.price).unwrap());
    }

    fn add_buy_order(&mut self, order: Order) {
        let mut remaining_quantity = order.quantity;
        let mut i = 0;
        while i < self.sell_orders.len() {
            if remaining_quantity == 0 {
                break;
            }
            if self.sell_orders[i].symbol == order.symbol && self.sell_orders[i].price <= order.price {
                if self.sell_orders[i].quantity > remaining_quantity {
                    self.sell_orders[i].quantity -= remaining_quantity;
                    remaining_quantity = 0;
                } else {
                    remaining_quantity -= self.sell_orders[i].quantity;
                    self.sell_orders.remove(i);
                    continue;
                }
            }
            i += 1;
        }
        if remaining_quantity > 0 {
            self.buy_orders.push(Order {
                quantity: remaining_quantity,
                ..order
            });
        }
        self.sort_orders();
    }

    fn add_sell_order(&mut self, order: Order) {
        let mut remaining_quantity = order.quantity;
        let mut i = 0;
        while i < self.buy_orders.len() {
            if remaining_quantity == 0 {
                break;
            }
            if self.buy_orders[i].symbol == order.symbol && self.buy_orders[i].price >= order.price {
                if self.buy_orders[i].quantity > remaining_quantity {
                    self.buy_orders[i].quantity -= remaining_quantity;
                    remaining_quantity = 0;
                } else {
                    remaining_quantity -= self.buy_orders[i].quantity;
                    self.buy_orders.remove(i);
                    continue;
                }
            }
            i += 1;
        }
        if remaining_quantity > 0 {
            self.sell_orders.push(Order {
                quantity: remaining_quantity,
                ..order
            });
        }
        self.sort_orders();
    }
}

async fn buy(order: web::Json<Order>, order_book: web::Data<Mutex<OrderBook>>) -> impl Responder {
    let mut book = order_book.lock().unwrap();
    book.add_buy_order(order.into_inner());
    HttpResponse::Ok().json(&*book)
}

async fn sell(order: web::Json<Order>, order_book: web::Data<Mutex<OrderBook>>) -> impl Responder {
    let mut book = order_book.lock().unwrap();
    book.add_sell_order(order.into_inner());
    HttpResponse::Ok().json(&*book)
}

async fn index() -> impl Responder {
    HttpResponse::Ok().body("Welcome to the Trading Server!")
}

async fn get_buy_orders(order_book: web::Data<Mutex<OrderBook>>) -> impl Responder {
    let book = order_book.lock().unwrap();
    HttpResponse::Ok().json(&book.buy_orders)
}

async fn get_sell_orders(order_book: web::Data<Mutex<OrderBook>>) -> impl Responder {
    let book = order_book.lock().unwrap();
    HttpResponse::Ok().json(&book.sell_orders)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let order_book = web::Data::new(Mutex::new(OrderBook::new()));

    HttpServer::new(move || {
        App::new()
            .app_data(order_book.clone())
            .route("/", web::get().to(index))
            .route("/buy", web::post().to(buy))
            .route("/sell", web::post().to(sell))
            .route("/buy_orders", web::get().to(get_buy_orders))
            .route("/sell_orders", web::get().to(get_sell_orders))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
