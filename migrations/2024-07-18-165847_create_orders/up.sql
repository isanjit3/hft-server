-- Your SQL goes here
CREATE TABLE orders (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    order_id TEXT NOT NULL,
    user_id INTEGER NOT NULL,
    symbol TEXT NOT NULL,
    quantity INTEGER NOT NULL,
    price INTEGER NOT NULL,
    order_type TEXT NOT NULL,
    FOREIGN KEY(user_id) REFERENCES users(id)
);