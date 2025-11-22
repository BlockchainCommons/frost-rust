use anyhow::{Result, bail};
use clap::Parser;
use mainline::Testnet;
use reqwest::Client;
use serde_json::Value;
use tokio::{
    runtime::Runtime,
    time::{Duration, timeout},
};

use crate::cmd::storage::{StorageSelection, StorageSelector};

#[derive(Debug, Parser)]
#[doc(hidden)]
pub struct CommandArgs {
    #[command(flatten)]
    storage: StorageSelector,
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        let selection = self.storage.resolve()?;
        let runtime = Runtime::new()?;
        runtime.block_on(async move { run_check(selection).await })
    }
}

async fn run_check(selection: StorageSelection) -> Result<()> {
    match selection {
        StorageSelection::Mainline => check_mainline().await,
        StorageSelection::Ipfs { port } => check_ipfs(port).await,
        StorageSelection::Hybrid { port } => {
            check_mainline().await?;
            check_ipfs(port).await?;
            println!("✓ Hybrid storage is available (DHT + IPFS)");
            Ok(())
        }
        StorageSelection::Server { host, port } => check_server(&host, port).await,
    }
}

async fn check_mainline() -> Result<()> {
    // Try to connect to mainline DHT using testnet
    match Testnet::new_async(5).await {
        Ok(_) => {
            println!("✓ Mainline DHT is available");
            Ok(())
        }
        Err(e) => {
            bail!("✗ Mainline DHT is not available: {}", e)
        }
    }
}

async fn check_ipfs(port: u16) -> Result<()> {
    let client = Client::new();
    let url = format!("http://127.0.0.1:{}/api/v0/version", port);
    match client
        .post(&url)
        .timeout(Duration::from_secs(2))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                println!("✓ IPFS is available at 127.0.0.1:{}", port);
                Ok(())
            } else {
                bail!("✗ IPFS daemon returned error: {}", response.status())
            }
        }
        Err(e) => {
            bail!("✗ IPFS is not available at 127.0.0.1:{}: {}", port, e)
        }
    }
}

async fn check_server(host: &str, port: u16) -> Result<()> {
    let url = format!("http://{}:{}/health", host, port);
    let client = Client::new();

    // Try to connect to health endpoint with 2-second timeout
    match timeout(Duration::from_secs(2), client.get(&url).send()).await {
        Ok(Ok(response)) => {
            if response.status().is_success() {
                // Try to parse the JSON response
                if let Ok(text) = response.text().await {
                    if let Ok(json) = serde_json::from_str::<Value>(&text) {
                        if json.get("server").and_then(|v| v.as_str())
                            == Some("hubert")
                        {
                            let version = json
                                .get("version")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown");
                            println!(
                                "✓ Hubert server is available at {}:{} (version {})",
                                host, port, version
                            );
                            Ok(())
                        } else {
                            bail!(
                                "✗ Server at {}:{} is not a Hubert server",
                                host, port
                            );
                        }
                    } else {
                        bail!(
                            "✗ Server at {}:{} returned invalid health response",
                            host, port
                        );
                    }
                } else {
                    bail!(
                        "✗ Server at {}:{} returned invalid health response",
                        host, port
                    );
                }
            } else {
                bail!(
                    "✗ Server at {}:{} is not available (status: {})",
                    host, port, response.status()
                );
            }
        }
        Ok(Err(e)) => {
            bail!("✗ Server is not available at {}:{}: {}", host, port, e)
        }
        Err(_) => {
            bail!(
                "✗ Server is not available at {}:{}: connection timeout",
                host, port
            )
        }
    }
}
