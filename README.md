# 📈 High-Frequency Trading Server

Welcome to the High-Frequency Trading Server! This project is designed to simulate a high-frequency trading environment, providing support for various types of orders, including limit and market orders. The platform leverages Rust for high-performance server-side operations and Solidity for smart contract integration. 

## 🚀 Features

### 🛒 Limit Orders
- Place limit orders to buy or sell shares at a specified price.
- Orders are matched when the buy price is greater than or equal to the sell price.

### 📉 Market Orders
- Place market orders to buy or sell shares at the best available price.
- Market orders prioritize immediate execution over price.

### 🧪 Comprehensive Testing
- Python scripts for testing the functionality of limit and market orders.
- Includes scenarios for both filled and unfilled orders to ensure reliability.

## 🔧 Installation and Setup

1. **Clone the Repository**
    ```bash
    git clone https://github.com/your-repo/hft_trading_server.git
    cd hft_trading_server
    ```

2. **Install Dependencies**
    - For Rust:
        ```bash
        rustup update
        cargo build
        ```
    - For Python:
        ```bash
        pip install -r requirements.txt
        ```

3. **Set Up Environment Variables**
    - Create a `.env` file with the necessary environment variables:
        ```env
        WS_URL=<your_web3_provider_url>
        CONTRACT_ADDRESS=<your_contract_address>
        ACCOUNT=<your_account_address>
        REDIS_CLIENT_URL=<your_redis_url>
        SECRET_KEY=<your_secret_key>
        ```

4. **Compile the Smart Contract**
    - Navigate to the `contracts` directory and compile the Solidity contract:
        ```bash
        solc --optimize --bin --abi OrderBook.sol -o build/
        ```

5. **Deploy the Contract**
    - Use a script or tool like Remix, Truffle, or Hardhat to deploy the compiled contract to your preferred Ethereum network.

## 📝 Usage

### Running the Server
- Start the Rust server:
    ```bash
    cargo run
    ```

### Running Tests
- Execute the Python test scripts:
    ```bash
    python test_limit_orders.py
    python test_market_orders.py
    ```

### API Endpoints
- **Place Buy Order**: `/buy`
- **Place Sell Order**: `/sell`
- **Get Portfolio**: `/portfolio/user/{user_id}`

## 📄 Contract Overview

### Order Types
- **Limit Order**: Executes at a specified price or better.
- **Market Order**: Executes immediately at the best available price.
- **Stop Order**: Can be added later for more complex trading strategies.

### Solidity Contract
- Manages buy and sell orders with different types.
- Emits events for order placements and matches.
- Matches orders based on specified rules for limit and market orders.

## 🛠️ Development

### Server Code
- **Rust**: For high-performance server-side logic.
- **Actix-Web**: Web framework for handling HTTP requests.
- **Redis**: For state management and order book storage.

### Smart Contract
- **Solidity**: For Ethereum-based smart contract logic.
- **Events**: Emit logs for order placements and matches.

## 💡 Next Steps

1. **Implement Stop Orders**: Add functionality for stop orders.
2. **Enhance Matching Logic**: Improve the order matching algorithm.
3. **Real-Time Data**: Integrate with real-time market data for dynamic trading.
4. **User Interface**: Develop a frontend for better user interaction.

## 👥 Contributors
- **Sanjit Thangarasu**: [GitHub](https://github.com/isanjit3) | [Website](https://sanjit.app)

## 📞 Contact
For any questions or suggestions, feel free to open an issue or contact us at [isanjit3@gmail.com](mailto:isanjit3@gmail.com).

---

Made with ⭐️ by Sanjit Thangarasu

