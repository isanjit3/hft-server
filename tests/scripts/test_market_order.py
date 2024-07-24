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
def place_buy_order(token, symbol, quantity, price, order_type):
    url = f"{BASE_URL}/buy"
    headers = {
        "Authorization": f"Bearer {token}"
    }
    payload = {
        "symbol": symbol,
        "quantity": quantity,
        "price": price,
        "order_type": order_type
    }
    response = requests.post(url, json=payload, headers=headers)
    return response.json()

# Function to place a sell order
def place_sell_order(token, symbol, quantity, price, order_type):
    url = f"{BASE_URL}/sell"
    headers = {
        "Authorization": f"Bearer {token}"
    }
    payload = {
        "symbol": symbol,
        "quantity": quantity,
        "price": price,
        "order_type": order_type
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

# Delete all data
delete_all_data_url = f"{BASE_URL}/utils/delete/all_data"
response = requests.delete(delete_all_data_url)
print("Deleted all data\n")

# Initialize User A (Alice) with money and no assets
alice = initialize_user("alice", "password123", 10000.0, {})
print("Initialized Alice\n")

# Initialize User B (Bob) with assets and no money
assets_bob = {
    "ABC": {
        "symbol": "ABC",
        "shares": 100,
        "market_value": 5500.0,
        "average_cost": 55.0,
        "portfolio_diversity": 1.0
    }
}
bob = initialize_user("bob", "password123", 0.0, assets_bob)
print("Initialized Bob\n")

# Initialize User C (Charlie) with assets and no money
assets_charlie = {
    "ABC": {
        "symbol": "ABC",
        "shares": 100,
        "market_value": 5000.0,
        "average_cost": 50.0,
        "portfolio_diversity": 1.0
    }
}
charlie = initialize_user("charlie", "password123", 0.0, assets_charlie)
print("Initialized Charlie\n")

# Login Alice
login_alice = login_user("alice", "password123")
token_alice = login_alice.get("token")
user_id_alice = login_alice.get("user_id")
print("Alice Logged in\n")

# Login Bob
login_bob = login_user("bob", "password123")
token_bob = login_bob.get("token")
user_id_bob = login_bob.get("user_id")
print("Bob Logged in\n")

# Login Charlie
login_charlie = login_user("charlie", "password123")
token_charlie = login_charlie.get("token")
user_id_charlie = login_charlie.get("user_id")
print("Charlie Logged in\n")

# Display Alice's Portfolio before transaction
portfolio_alice_before = get_user_portfolio(token_alice, user_id_alice)
print("Alice's Portfolio Before Transaction:\n", json.dumps(portfolio_alice_before, indent=4), "\n")

# Display Bob's Portfolio before transaction
portfolio_bob_before = get_user_portfolio(token_bob, user_id_bob)
print("Bob's Portfolio Before Transaction:\n", json.dumps(portfolio_bob_before, indent=4), "\n")

# Display Charlie's Portfolio before transaction
portfolio_charlie_before = get_user_portfolio(token_charlie, user_id_charlie)
print("Charlie's Portfolio Before Transaction:\n", json.dumps(portfolio_charlie_before, indent=4), "\n")

# Alice places a market buy order for ABC
buy_order_alice = place_buy_order(token_alice, "ABC", 100, 0, "Market")
print("Alice's Market Buy Order:\n", json.dumps(buy_order_alice, indent=4), "\n")

# Bob places a sell order for ABC at $55
sell_order_bob = place_sell_order(token_bob, "ABC", 100, 55, "Limit")
print("Bob's Limit Sell Order:\n", json.dumps(sell_order_bob, indent=4), "\n")

# Charlie places a sell order for ABC at $50
sell_order_charlie = place_sell_order(token_charlie, "ABC", 100, 50, "Limit")
print("Charlie's Limit Sell Order:\n", json.dumps(sell_order_charlie, indent=4), "\n")

# Display Alice's Portfolio after transaction
portfolio_alice_after = get_user_portfolio(token_alice, user_id_alice)
print("Alice's Portfolio After Transaction:\n", json.dumps(portfolio_alice_after, indent=4), "\n")

# Display Bob's Portfolio after transaction
portfolio_bob_after = get_user_portfolio(token_bob, user_id_bob)
print("Bob's Portfolio After Transaction:\n", json.dumps(portfolio_bob_after, indent=4), "\n")

# Display Charlie's Portfolio after transaction
portfolio_charlie_after = get_user_portfolio(token_charlie, user_id_charlie)
print("Charlie's Portfolio After Transaction:\n", json.dumps(portfolio_charlie_after, indent=4), "\n")

# Check if the test performs as expected

def check_expected_behavior(portfolio_before, portfolio_after, expected_assets, expected_money):
    actual_assets = portfolio_after.get("assets", {})
    actual_money = portfolio_after.get("total_money", 0.0)

    for symbol, expected_asset in expected_assets.items():
        actual_asset = actual_assets.get(symbol)
        if actual_asset is None or actual_asset["shares"] != expected_asset["shares"] or actual_asset["market_value"] != expected_asset["market_value"]:
            print(f"Test Failed for {symbol}: Expected {expected_asset}, but got {actual_asset}")
            return False

    if actual_money != expected_money:
        print(f"Test Failed for total money: Expected {expected_money}, but got {actual_money}")
        return False

    print("Test Passed")
    return True

# Expected outcomes
expected_alice_assets = {
    "ABC": {
        "shares": 100,
        "market_value": 5500.0,
    }
}
expected_alice_money = 4500.0

expected_bob_assets = {}
expected_bob_money = 5500.0

expected_charlie_assets = {
    "ABC": {
        "shares": 100,
        "market_value": 5000.0,
    }
}
expected_charlie_money = 0.0

print("Checking Alice's portfolio after transaction:")
check_expected_behavior(portfolio_alice_before, portfolio_alice_after, expected_alice_assets, expected_alice_money)

print("Checking Bob's portfolio after transaction:")
check_expected_behavior(portfolio_bob_before, portfolio_bob_after, expected_bob_assets, expected_bob_money)

print("Checking Charlie's portfolio after transaction:")
check_expected_behavior(portfolio_charlie_before, portfolio_charlie_after, expected_charlie_assets, expected_charlie_money)