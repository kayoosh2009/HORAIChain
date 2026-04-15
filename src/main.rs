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
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct WalletRecord {
    pub address: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct BalanceRecord {
    pub address: String,
    pub token: String,
    pub amount: f64,
}

pub struct SupabaseClient {
    url: String,
    key: String,
    client: reqwest::Client,
}

// --- Blockchain Functions ---

impl SupabaseClient {
    pub fn new() -> Self {
        let url = env::var("SUPABASE_URL").expect("SUPABASE_URL must be set");
        let key = env::var("SUPABASE_KEY").expect("SUPABASE_KEY must be set");
        Self {
            url,
            key,
            client: reqwest::Client::new(),
        }
    }

    // Создать кошелёк в БД + начислить 100 HORAI приветственных токенов
    pub async fn create_wallet_record(&self, address: &str) -> Result<(), String> {
        // 1. Записываем кошелёк
        let res = self.client
            .post(format!("{}/rest/v1/wallets", self.url))
            .header("apikey", &self.key)
            .header("Authorization", format!("Bearer {}", self.key))
            .header("Content-Type", "application/json")
            .json(&WalletRecord { address: address.to_string() })
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !res.status().is_success() {
            return Err(format!("Failed to create wallet: {}", res.status()));
        }

        // 2. Начисляем 100 HORAI стартовых токенов
        let res2 = self.client
            .post(format!("{}/rest/v1/balances", self.url))
            .header("apikey", &self.key)
            .header("Authorization", format!("Bearer {}", self.key))
            .header("Content-Type", "application/json")
            .json(&BalanceRecord {
                address: address.to_string(),
                token: "HORAI".to_string(),
                amount: 100.0,
            })
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !res2.status().is_success() {
            return Err(format!("Failed to create balance: {}", res2.status()));
        }

        Ok(())
    }
    // Перевод токенов между кошельками (3% комиссия владельцу)
    pub async fn transfer(&self, from: &str, to: &str, amount: f64) -> Result<(), String> {
        let fee = (amount * 0.03 * 100.0).round() / 100.0;
        let net_amount = (amount * 100.0).round() / 100.0 - fee;
        let owner = env::var("OWNER_ADDRESS").expect("OWNER_ADDRESS must be set");

        // 1. Проверяем баланс отправителя
        let balances = self.get_balance(from).await?;
        let balance = balances.iter()
            .find(|b| b.token == "HORAI")
            .map(|b| b.amount)
            .unwrap_or(0.0);

        if balance < amount {
            return Err(format!("Insufficient balance: have {}, need {}", balance, amount));
        }

        // 2. Списываем у отправителя
        self.client
            .patch(format!("{}/rest/v1/balances?address=eq.{}&token=eq.HORAI", self.url, from))
            .header("apikey", &self.key)
            .header("Authorization", format!("Bearer {}", self.key))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({ "amount": balance - amount }))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        // 3. Начисляем получателю
        self.update_or_create_balance(to, net_amount).await?;

        // 4. Начисляем комиссию владельцу
        self.update_or_create_balance(&owner, fee).await?;

        // 5. Записываем транзакцию в БД
        self.client
            .post(format!("{}/rest/v1/transactions", self.url))
            .header("apikey", &self.key)
            .header("Authorization", format!("Bearer {}", self.key))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "from_address": from,
                "to_address": to,
                "token": "HORAI",
                "amount": amount
            }))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    // Вспомогательная: пополнить баланс или создать если нет
    pub async fn update_or_create_balance(&self, address: &str, add_amount: f64) -> Result<(), String> {
        let balances = self.get_balance(address).await?;
        
        if let Some(bal) = balances.iter().find(|b| b.token == "HORAI") {
            // Обновляем существующий
            self.client
                .patch(format!("{}/rest/v1/balances?address=eq.{}&token=eq.HORAI", self.url, address))
                .header("apikey", &self.key)
                .header("Authorization", format!("Bearer {}", self.key))
                .header("Content-Type", "application/json")
                .json(&serde_json::json!({ "amount": bal.amount + add_amount }))
                .send()
                .await
                .map_err(|e| e.to_string())?;
        } else {
            // Создаём новую запись
            self.client
                .post(format!("{}/rest/v1/balances", self.url))
                .header("apikey", &self.key)
                .header("Authorization", format!("Bearer {}", self.key))
                .header("Content-Type", "application/json")
                .json(&BalanceRecord {
                    address: address.to_string(),
                    token: "HORAI".to_string(),
                    amount: add_amount,
                })
                .send()
                .await
                .map_err(|e| e.to_string())?;
        }

        Ok(())
    }
    
    // Получить баланс кошелька
    pub async fn get_balance(&self, address: &str) -> Result<Vec<BalanceRecord>, String> {
        let res = self.client
            .get(format!("{}/rest/v1/balances?address=eq.{}", self.url, address))
            .header("apikey", &self.key)
            .header("Authorization", format!("Bearer {}", self.key))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let balances: Vec<BalanceRecord> = res.json().await.map_err(|e| e.to_string())?;
        Ok(balances)
    }
}

async fn send_explorer_notification(message: &str) {
    let bot_token = env::var("TELEGRAM_BOT_TOKEN").unwrap_or_default();
    let chat_id = env::var("TELEGRAM_CHAT_ID").unwrap_or_default();
    
    let url = format!("https://api.telegram.org/bot{}/sendMessage", bot_token);
    
    let client = reqwest::Client::new();
    let params = [("chat_id", chat_id), ("text", message.to_string()), ("parse_mode", "Markdown".to_string())];

    // Отправляем уведомление, не блокируя основной поток (ошибки просто логируем)
    let _ = client.post(url).form(&params).send().await.map_err(|e| {
        println!("Failed to send TG notification: {}", e);
    });
}

pub async fn create_wallet() -> WalletInfo {
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

    // Уведомление в TG-обозреватель
    let message = format!(
        "🆕 *New wallet registered!*\n\n`{}`\n\n🎁 Send a welcome gift to the new network member!",
        address
    );
    send_explorer_notification(&message).await;

    WalletInfo {
        mnemonic: mnemonic_phrase,
        address,
    }
}

pub fn import_wallet(phrase: &str) -> Result<WalletInfo, String> {
    // 1. Пытаемся распарсить фразу
    let mnemonic = Mnemonic::parse_in_normalized(Language::English, phrase)
        .map_err(|_| "Invalid mnemonic phrase. Please check your 12 words.".to_string())?;

    // 2. Генерируем тот же самый Seed (как и при создании)
    let seed = mnemonic.to_seed(""); 

    // 3. Повторяем процесс хеширования для получения того же адреса
    let mut hasher = Keccak::v256();
    let mut output = [0u8; 32];
    hasher.update(&seed);
    hasher.finalize(&mut output);

    let address = format!("horai_{}", hex::encode(&output[0..20]));

    Ok(WalletInfo {
        mnemonic: phrase.to_string(),
        address,
    })
}

// --- Code running ---
#[tokio::main]
async fn main() {
    dotenv().ok();
    
    println!("--- HORAIChain Backend Starting ---");

    let _chat_id = env::var("TELEGRAM_CHAT_ID").expect("TELEGRAM_CHAT_ID must be set");
    println!("Explorer Channel ID loaded.");

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