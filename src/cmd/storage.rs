use anyhow::{Result, anyhow, bail};
use bc_components::ARID;
use bc_envelope::Envelope;
use clap::{Args, ValueEnum};
use hubert::{
    KvStore, hybrid::HybridKv, ipfs::IpfsKv, mainline::MainlineDhtKv,
    server::ServerKvClient,
};

use super::is_verbose;

#[derive(Debug, Clone, Copy, ValueEnum)]
#[doc(hidden)]
pub enum StorageBackend {
    /// BitTorrent Mainline DHT (fast, ≤1 KB messages)
    Mainline,
    /// IPFS (large capacity, up to 10 MB messages)
    Ipfs,
    /// Hybrid (automatic: DHT for small, IPFS for large)
    Hybrid,
    /// Hubert HTTP server (centralized coordination)
    Server,
}

/// Common storage selection options shared across commands.
#[derive(Debug, Clone, Args)]
#[doc(hidden)]
pub struct StorageSelector {
    /// Storage backend to use
    #[arg(long, short, value_enum, default_value = "mainline")]
    pub storage: StorageBackend,

    /// Server/IPFS host (for --storage server)
    #[arg(long)]
    pub host: Option<String>,

    /// Port (for --storage server, --storage ipfs, or --storage hybrid)
    #[arg(long)]
    pub port: Option<u16>,
}

#[derive(Debug, Clone)]
pub enum StorageSelection {
    Mainline,
    Ipfs { port: u16 },
    Hybrid { port: u16 },
    Server { host: String, port: u16 },
}

impl StorageSelector {
    pub fn resolve(&self) -> Result<StorageSelection> {
        match self.storage {
            StorageBackend::Mainline => {
                ensure_absent(self.host.as_deref(), "--host", "mainline")?;
                ensure_absent(self.port, "--port", "mainline")?;
                Ok(StorageSelection::Mainline)
            }
            StorageBackend::Ipfs => {
                ensure_absent(self.host.as_deref(), "--host", "ipfs")?;
                let port = self.port.unwrap_or(5001);
                Ok(StorageSelection::Ipfs { port })
            }
            StorageBackend::Hybrid => {
                ensure_absent(self.host.as_deref(), "--host", "hybrid")?;
                let port = self.port.unwrap_or(5001);
                Ok(StorageSelection::Hybrid { port })
            }
            StorageBackend::Server => {
                let host =
                    self.host.clone().unwrap_or_else(|| "127.0.0.1".to_owned());
                let port = self.port.unwrap_or(45678);
                Ok(StorageSelection::Server { host, port })
            }
        }
    }
}

/// Helper that opens the selected Hubert storage backend.
pub enum StorageClient {
    Mainline(MainlineDhtKv),
    Ipfs(IpfsKv),
    Hybrid(HybridKv),
    Server(ServerKvClient),
}

impl StorageClient {
    pub async fn from_selection(selection: StorageSelection) -> Result<Self> {
        match selection {
            StorageSelection::Mainline => {
                Ok(Self::Mainline(MainlineDhtKv::new().await?))
            }
            StorageSelection::Ipfs { port } => {
                let url = format!("http://127.0.0.1:{port}");
                Ok(Self::Ipfs(IpfsKv::new(&url)))
            }
            StorageSelection::Hybrid { port } => {
                let url = format!("http://127.0.0.1:{port}");
                Ok(Self::Hybrid(HybridKv::new(&url).await?))
            }
            StorageSelection::Server { host, port } => {
                let url = format!("http://{host}:{port}");
                Ok(Self::Server(ServerKvClient::new(&url)))
            }
        }
    }

    pub async fn put(
        &self,
        arid: &ARID,
        envelope: &Envelope,
    ) -> Result<String> {
        match self {
            StorageClient::Mainline(store) => {
                store.put(arid, envelope, None, is_verbose()).await
            }
            StorageClient::Ipfs(store) => {
                store.put(arid, envelope, None, is_verbose()).await
            }
            StorageClient::Hybrid(store) => {
                store.put(arid, envelope, None, is_verbose()).await
            }
            StorageClient::Server(store) => {
                store.put(arid, envelope, None, is_verbose()).await
            }
        }
        .map_err(|err| anyhow!(err))
    }

    pub async fn get(
        &self,
        arid: &ARID,
        timeout_seconds: Option<u64>,
    ) -> Result<Option<Envelope>> {
        match self {
            StorageClient::Mainline(store) => {
                store.get(arid, timeout_seconds, is_verbose()).await
            }
            StorageClient::Ipfs(store) => {
                store.get(arid, timeout_seconds, is_verbose()).await
            }
            StorageClient::Hybrid(store) => {
                store.get(arid, timeout_seconds, is_verbose()).await
            }
            StorageClient::Server(store) => {
                store.get(arid, timeout_seconds, is_verbose()).await
            }
        }
        .map_err(|err| anyhow!(err))
    }
}

pub(crate) fn ensure_absent<T>(
    value: Option<T>,
    flag: &str,
    backend: &str,
) -> Result<()> {
    if value.is_some() {
        bail!("{flag} option is not supported for --storage {backend}");
    }
    Ok(())
}
