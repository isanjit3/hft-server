-- Your SQL goes here
CREATE TABLE assets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    portfolio_id TEXT NOT NULL,
    symbol TEXT NOT NULL,
    shares INTEGER NOT NULL,
    market_value DOUBLE NOT NULL,
    average_cost DOUBLE NOT NULL,
    portfolio_diversity DOUBLE NOT NULL,
    FOREIGN KEY(portfolio_id) REFERENCES portfolios(portfolio_id)
);
