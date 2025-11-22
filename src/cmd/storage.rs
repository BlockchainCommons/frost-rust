use anyhow::{Result, bail};
use clap::{Args, ValueEnum};

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

fn ensure_absent<T>(value: Option<T>, flag: &str, backend: &str) -> Result<()> {
    if value.is_some() {
        bail!("{flag} option is not supported for --storage {backend}");
    }
    Ok(())
}
