use std::path::{Path, PathBuf};

use bridgingio_desktop_host::bridge::BridgeTransport;

#[cfg(unix)]
use std::io::{BufRead, BufReader, Write};
#[cfg(unix)]
use std::os::unix::net::UnixStream;

#[derive(Clone, Debug)]
pub enum HostBridgeTransport {
    #[cfg(unix)]
    UnixSocket {
        socket_path: PathBuf,
    },
    Unsupported {
        reason: String,
    },
}

impl HostBridgeTransport {
    pub fn from_runtime_root(runtime_root: &Path, unsupported_reason: Option<String>) -> Self {
        if let Some(reason) = unsupported_reason {
            return Self::Unsupported { reason };
        }

        #[cfg(unix)]
        {
            return Self::UnixSocket {
                socket_path: runtime_root.join("state/control-plane.sock"),
            };
        }

        #[cfg(not(unix))]
        {
            Self::Unsupported {
                reason:
                    "local control-plane transport is deferred on non-unix host in current baseline"
                        .to_string(),
            }
        }
    }
}

impl BridgeTransport for HostBridgeTransport {
    fn request_response(&self, request_line: &str) -> Result<String, String> {
        match self {
            #[cfg(unix)]
            HostBridgeTransport::UnixSocket { socket_path } => {
                let mut stream = UnixStream::connect(socket_path)
                    .map_err(|err| format!("connect control-plane socket failed: {err}"))?;
                stream
                    .write_all(format!("{request_line}\n").as_bytes())
                    .map_err(|err| format!("write control-plane request failed: {err}"))?;
                let mut response_line = String::new();
                {
                    let mut reader = BufReader::new(&mut stream);
                    reader
                        .read_line(&mut response_line)
                        .map_err(|err| format!("read control-plane response failed: {err}"))?;
                }
                Ok(response_line.trim().to_string())
            }
            HostBridgeTransport::Unsupported { reason } => Err(reason.clone()),
        }
    }
}
