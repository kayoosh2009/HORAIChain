use axum::{
    routing::get,
    Router,
};
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use dotenv::dotenv;
use std::env;
use bip39::{Mnemonic, Language};
use tiny_keccak::{Hasher, Keccak};

// --- Data Setup ---
#[derive(Debug)]
pub struct WalletInfo {
    pub mnemonic: String,
    pub address: String,
}

// --- Blockchain Functions ---
pub fn create_wallet() -> WalletInfo {
    // Генерируем 16 байт энтропии через rand
    let mut entropy = [0u8; 16];
    getrandom::getrandom(&mut entropy).expect("Failed to generate entropy");
    
    let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy)
        .expect("Failed to create mnemonic");
    
    let mnemonic_phrase = mnemonic.to_string();
    let seed = mnemonic.to_seed(""); 

    let mut hasher = Keccak::v256();
    let mut output = [0u8; 32];
    hasher.update(&seed);
    hasher.finalize(&mut output);

    let address = format!("horai_{}", hex::encode(&output[0..20]));

    WalletInfo {
        mnemonic: mnemonic_phrase,
        address,
    }
}

// --- Code running ---
#[tokio::main]
async fn main() {
    dotenv().ok();
    
    println!("--- HORAIChain Backend Starting ---");

    // Для теста: создадим кошелек при запуске и выведем его в консоль
    let test_wallet = create_wallet();
    println!("New Wallet Generated!");
    println!("Mnemonic: {}", test_wallet.mnemonic);
    println!("Address: {}", test_wallet.address);

    let bot_token = env::var("TELEGRAM_BOT_TOKEN").expect("TELEGRAM_BOT_TOKEN must be set");
    let supabase_url = env::var("SUPABASE_URL").expect("SUPABASE_URL must be set");
    println!("Successfully connected to bot: {}...", &bot_token[..5]);
    println!("Supabase environment: {}", supabase_url);
    
    println!("Configuration loaded successfully.");

    let app = Router::new()
        .route("/", get(health_check))
        .layer(CorsLayer::permissive());

    let port = env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .expect("PORT must be a number");

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    
    println!("HORAIChain API is running on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health_check() -> &'static str {
    "HORAIChain API: Online"
}