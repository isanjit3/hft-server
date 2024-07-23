import requests
import json

BASE_URL = "http://127.0.0.1:8080"

# Function to initialize a user
def initialize_user(username, password, total_money, assets):
    url = f"{BASE_URL}/utils/post/initialize_user"
    payload = {
        "username": username,
        "password": password,
        "total_money": total_money,
        "assets": assets
    }
    response = requests.post(url, json=payload)
    return response.json()

# Function to login a user and get the token
def login_user(username, password):
    url = f"{BASE_URL}/login"
    payload = {
        "username": username,
        "password": password
    }
    response = requests.post(url, json=payload)
    return response.json()

# Function to place a buy order
def place_buy_order(token, symbol, quantity, price):
    url = f"{BASE_URL}/buy"
    headers = {
        "Authorization": f"Bearer {token}"
    }
    payload = {
        "symbol": symbol,
        "quantity": quantity,
        "price": price
    }
    response = requests.post(url, json=payload, headers=headers)
    return response.json()

# Function to place a sell order
def place_sell_order(token, symbol, quantity, price):
    url = f"{BASE_URL}/sell"
    headers = {
        "Authorization": f"Bearer {token}"
    }
    payload = {
        "symbol": symbol,
        "quantity": quantity,
        "price": price
    }
    response = requests.post(url, json=payload, headers=headers)
    return response.json()

# Function to get user portfolio
def get_user_portfolio(token, user_id):
    url = f"{BASE_URL}/portfolio/user/{user_id}"
    headers = {
        "Authorization": f"Bearer {token}"
    }
    response = requests.get(url, headers=headers)
    return response.json()

# Initialize User 1 with money and no assets
user1 = initialize_user("user1", "password123", 10000.0, {})
print("Initialized User 1:", user1)

# Initialize User 2 with assets and no money
assets_user2 = {
    "AAPL": {
        "symbol": "AAPL",
        "shares": 10,
        "market_value": 1500.0,
        "average_cost": 150.0,
        "portfolio_diversity": 1.0
    }
}
user2 = initialize_user("user2", "password123", 0.0, assets_user2)
print("Initialized User 2:", user2)

# Login User 1
login_user1 = login_user("user1", "password123")
token_user1 = login_user1.get("token")
user_id1 = login_user1.get("user_id")
print("User 1 Login Token:", token_user1)

# Login User 2
login_user2 = login_user("user2", "password123")
token_user2 = login_user2.get("token")
user_id2 = login_user2.get("user_id")
print("User 2 Login Token:", token_user2)

# Display User 1 Portfolio before transaction
portfolio_user1_before = get_user_portfolio(token_user1, user_id1)
print("User 1 Portfolio Before Transaction:", json.dumps(portfolio_user1_before, indent=4))

# Display User 2 Portfolio before transaction
portfolio_user2_before = get_user_portfolio(token_user2, user_id2)
print("User 2 Portfolio Before Transaction:", json.dumps(portfolio_user2_before, indent=4))

# User 1 places a buy order for AAPL
buy_order = place_buy_order(token_user1, "AAPL", 10, 150)
print("User 1 Buy Order:", buy_order)

# User 2 places a sell order for AAPL
sell_order = place_sell_order(token_user2, "AAPL", 10, 150)
print("User 2 Sell Order:", sell_order)

# Display User 1 Portfolio after transaction
portfolio_user1_after = get_user_portfolio(token_user1, user_id1)
print("User 1 Portfolio After Transaction:", json.dumps(portfolio_user1_after, indent=4))

# Display User 2 Portfolio after transaction
portfolio_user2_after = get_user_portfolio(token_user2, user_id2)
print("User 2 Portfolio After Transaction:", json.dumps(portfolio_user2_after, indent=4))