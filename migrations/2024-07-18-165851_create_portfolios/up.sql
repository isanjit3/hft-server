-- Your SQL goes here
CREATE TABLE portfolios (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    portfolio_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    total_money DOUBLE NOT NULL,
    FOREIGN KEY(user_id) REFERENCES users(user_id)
);
