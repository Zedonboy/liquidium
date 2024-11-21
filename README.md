# Liquidium - ICP-Bitcoin Bridge for Rune-Backed Loans

## Overview
Liquidium is a decentralized lending protocol built on the Internet Computer Protocol (ICP) that enables BTC-denominated loans using Runes as collateral. The protocol facilitates peer-to-peer lending by connecting lenders providing BTC liquidity with borrowers offering Rune tokens as collateral.

## Architecture

### Core Components

1. **Loan Management System**
   - Handles creation and management of loan offers
   - Manages collateral deposits
   - Processes loan disbursements and repayments
   - Monitors loan expiry and handles liquidations

2. **Bitcoin Integration**
   - Utilizes ICP's Bitcoin integration for transaction processing
   - Manages BTC addresses and UTXO tracking
   - Handles P2PKH transaction signing and verification

3. **Rune Token Integration**
   - Processes Rune token transfers
   - Verifies Rune ownership and amounts
   - Manages Rune-based collateral

### Key Data Structures

1. **LoanOffer**
   - Contains loan terms, collateral requirements, and LTV ratios
   - Tracks interest rates and loan duration

2. **Bundle**
   - Represents lender's BTC liquidity
   - Contains transaction information and status

3. **Collateral**
   - Stores Rune token information
   - Tracks ownership and loan associations

4. **LoanTransaction**
   - Manages active loans
   - Tracks loan status, amounts, and expiry

## API Methods

### Lender Methods

#### `create_liquidity_bundle`
```rust
pub async fn create_liquidity_bundle(bundle: Bundle) -> Result<(String, String), String>
```
- Creates a new liquidity bundle from lender's BTC
- Returns bundle ID and transaction ID
- Validates sufficient funds
- Creates dedicated BTC address for bundle

#### `create_loan_offer`
```rust
pub async fn create_loan_offer(offer: LoanOffer) -> Result<(), String>
```
- Creates a loan offer using existing liquidity bundle
- Sets terms like collateral requirements, LTV, and duration
- Links offer to specific liquidity bundle

### Borrower Methods

#### `create_collateral`
```rust
pub async fn create_collateral(collateral: Collateral) -> Result<(String, String), String>
```
- Deposits Rune tokens as collateral
- Validates Rune ownership and amounts
- Creates dedicated address for collateral
- Returns collateral ID and transaction ID

#### `get_loan`
```rust
async fn get_loan(request: LoanRequest) -> Result<String, String>
```
- Processes loan request against existing offer
- Validates collateral requirements
- Disburses BTC to borrower
- Creates loan transaction record

#### `pay_loan`
```rust
async fn pay_loan(loan_trx: String) -> Result<(String, String), String>
```
- Processes loan repayment
- Returns collateral upon successful repayment
- Updates loan status

### Utility Methods

#### `deposit_address`
```rust
pub async fn deposit_address() -> String
```
- Generates unique BTC deposit address for user

#### `btc_balance`
```rust
async fn btc_balance(address: String) -> u64
```
- Retrieves BTC balance for given address

### Query Methods

#### `get_loan_offers`
```rust
pub async fn get_loan_offers(lender: Option<Principal>) -> Vec<LoanOffer>
```
- Retrieves available loan offers
- Optional filter by lender

#### `get_colateral`
```rust
fn get_colateral(id: String) -> Option<Collateral>
```
- Retrieves collateral information by ID

#### `get_bundle`
```rust
fn get_bundle(id: String) -> Option<Bundle>
```
- Retrieves bundle information by ID

#### `get_loan_transaction`
```rust
fn get_loan_transaction(id: String) -> Option<LoanTransaction>
```
- Retrieves loan transaction details by ID

## Security Features

1. **Authentication**
   - Uses `is_not_anonymous` guard for protected methods
   - Validates ownership of assets and transactions

2. **Transaction Safety**
   - Implements proper BTC transaction signing
   - Validates UTXO ownership
   - Handles transaction fees appropriately

3. **Loan Safety**
   - Automatic liquidation on expiry
   - LTV ratio enforcement
   - Collateral verification

## Storage Management

The system uses thread-local storage with RefCell for managing state:
```rust
thread_local! {
    static LENDER_OFFER: RefCell<HashMap<Principal, HashSet<String>>>
    static LOAN_OFFER: RefCell<HashMap<String, LoanOffer>>
    static BUNDLES: RefCell<HashMap<String, Bundle>>
    static COLLATERALS: RefCell<HashMap<String, Collateral>>
    // ... additional storage
}
```

## Development Setup

1. Ensure you have the ICP SDK installed
2. Configure Bitcoin testnet/regtest environment
3. Set up required environment variables:
   - `RPC_USER`
   - `RPC_PASS`
   - `BTC_RPC_URL`

## Testing

The project includes integration tests using the PocketIC framework. Run tests using:
```bash
cargo test
```

## Dependencies

- `bitcoin` - Bitcoin network integration
- `candid` - ICP interface definition
- `ic_cdk` - Internet Computer development kit
- `ordinals` - Rune token handling
- Additional utility crates for cryptography and encoding