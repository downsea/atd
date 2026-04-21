use atd_types::AtdError;
#[cfg(test)]
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::UnixStream;
use tokio::sync::Mutex;

use crate::endpoint::Endpoint;
use crate::protocol::{Request, Response};
use crate::wire::{read_frame, write_frame};

/// Async ATD client.
///
/// Each request/response pair is serialized under an internal mutex so the
/// client is safe to clone across tasks by wrapping in `Arc<AtdClient>`.
pub struct AtdClient {
    inner: Mutex<Pipe>,
}

enum Pipe {
    Unix {
        read: tokio::net::unix::OwnedReadHalf,
        write: tokio::net::unix::OwnedWriteHalf,
    },
    /// Used only by in-crate tests.
    #[cfg(test)]
    Duplex {
        read: Box<dyn AsyncRead + Send + Unpin>,
        write: Box<dyn AsyncWrite + Send + Unpin>,
    },
}

impl AtdClient {
    pub async fn connect(endpoint: Endpoint) -> Result<Self, AtdError> {
        match endpoint {
            Endpoint::UnixSocket(path) => {
                let stream = UnixStream::connect(&path).await?;
                let (read, write) = stream.into_split();
                let client = AtdClient {
                    inner: Mutex::new(Pipe::Unix { read, write }),
                };
                client.ping().await?;
                Ok(client)
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn from_duplex<R, W>(read: R, write: W) -> Self
    where
        R: AsyncRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        AtdClient {
            inner: Mutex::new(Pipe::Duplex {
                read: Box::new(read),
                write: Box::new(write),
            }),
        }
    }

    pub async fn ping(&self) -> Result<(), AtdError> {
        match self.request(&Request::Ping).await? {
            Response::Pong => Ok(()),
            other => Err(AtdError::ProtocolError {
                expected: "pong".into(),
                got: format!("{other:?}"),
            }),
        }
    }

    pub(crate) async fn request(&self, req: &Request) -> Result<Response, AtdError> {
        let mut guard = self.inner.lock().await;
        match &mut *guard {
            Pipe::Unix { read, write } => {
                write_frame(write, req).await?;
                let resp: Response = read_frame(read).await?;
                Ok(resp)
            }
            #[cfg(test)]
            Pipe::Duplex { read, write } => {
                write_frame(write, req).await?;
                let resp: Response = read_frame(read).await?;
                Ok(resp)
            }
        }
    }

    pub async fn discover(
        &self,
        query: Option<&str>,
        filter: crate::options::DiscoverFilter,
    ) -> Result<Vec<atd_types::ToolSummary>, AtdError> {
        let resp = self.request(&Request::ToolList).await?;
        let raw = match resp {
            Response::ToolListResponse { tools } => tools,
            Response::Error { message, .. } => {
                return Err(AtdError::ProtocolError {
                    expected: "tool_list".into(),
                    got: format!("error: {message}"),
                });
            }
            other => {
                return Err(AtdError::ProtocolError {
                    expected: "tool_list".into(),
                    got: format!("{other:?}"),
                });
            }
        };

        let arr = raw.as_array().ok_or_else(|| AtdError::ProtocolError {
            expected: "array of tool summaries".into(),
            got: format!("{raw}"),
        })?;

        let mut out: Vec<atd_types::ToolSummary> = Vec::with_capacity(arr.len());
        for v in arr {
            match serde_json::from_value::<atd_types::ToolSummary>(v.clone()) {
                Ok(s) => out.push(s),
                Err(_) => {
                    // Tolerate entries that are full ToolDefinitions by projecting down.
                    if let Ok(def) =
                        serde_json::from_value::<atd_types::ToolDefinition>(v.clone())
                    {
                        out.push(atd_types::ToolSummary::from(&def));
                    }
                }
            }
        }

        if let Some(q) = query {
            let q_lower = q.to_lowercase();
            out.retain(|s| {
                s.name.to_lowercase().contains(&q_lower)
                    || s.description.to_lowercase().contains(&q_lower)
                    || s.id.to_lowercase().contains(&q_lower)
            });
        }
        if let Some(d) = filter.domain.as_deref() {
            out.retain(|s| s.domain == d);
        }
        if let Some(v) = filter.visibility {
            out.retain(|s| s.visibility == v);
        }
        if let Some(t) = filter.tier {
            out.retain(|s| s.tier == t);
        }
        if let Some(n) = filter.limit {
            out.truncate(n);
        }

        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    /// Spawn a task that acts as a one-shot server: reads exactly one request
    /// from the server-side of a duplex pipe, maps it to a scripted response.
    async fn spin_server<F>(server_end: tokio::io::DuplexStream, mut handler: F)
    where
        F: FnMut(Request) -> Response + Send + 'static,
    {
        let (mut read, mut write) = tokio::io::split(server_end);
        tokio::spawn(async move {
            while let Ok(req) = read_frame::<_, Request>(&mut read).await {
                let resp = handler(req);
                if write_frame(&mut write, &resp).await.is_err() {
                    break;
                }
            }
        });
    }

    #[tokio::test]
    async fn ping_returns_ok_when_server_sends_pong() {
        let (client_end, server_end) = duplex(4096);
        spin_server(server_end, |req| match req {
            Request::Ping => Response::Pong,
            _ => Response::Error {
                message: "unexpected".into(),
                code: None,
                retryable: None,
                details: None,
            },
        })
        .await;

        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        client.ping().await.unwrap();
    }

    #[tokio::test]
    async fn ping_errors_when_server_sends_wrong_response() {
        let (client_end, server_end) = duplex(4096);
        spin_server(server_end, |_| Response::HelloResponse {
            version: "x".into(),
            capabilities: vec![],
        })
        .await;

        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let err = client.ping().await.unwrap_err();
        assert!(matches!(err, AtdError::ProtocolError { .. }));
    }

    #[tokio::test]
    async fn discover_projects_tool_definitions_to_summaries() {
        let (client_end, server_end) = duplex(16_384);
        spin_server(server_end, |req| match req {
            Request::ToolList => Response::ToolListResponse {
                tools: serde_json::json!([
                    {
                        "id": "anos:fs.read",
                        "name": "Read",
                        "description": "read a file",
                        "version": "0.1.0",
                        "capability": {
                            "domain": "fs",
                            "actions": ["read"],
                            "tags": ["filesystem"],
                            "intent_examples": []
                        },
                        "input_schema": {},
                        "output_schema": {},
                        "bindings": [{"protocol": "Cli", "config": {}}],
                        "safety": {"level": "Read", "dry_run": false, "side_effects": [], "data_sensitivity": null},
                        "resources": {"timeout_ms": 1000, "max_concurrent": 1, "rate_limit_per_min": null, "estimated_tokens": null},
                        "trust": {"publisher": "anos", "trust_level": "L2Tested", "signature": null},
                        "visibility": "read"
                    }
                ]),
            },
            _ => unreachable!(),
        })
        .await;

        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let summaries = client
            .discover(None, crate::options::DiscoverFilter::default())
            .await
            .unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, "anos:fs.read");
        assert_eq!(summaries[0].domain, "fs");
    }

    #[tokio::test]
    async fn discover_applies_query_and_limit_client_side() {
        let (client_end, server_end) = duplex(16_384);
        spin_server(server_end, |_| Response::ToolListResponse {
            tools: serde_json::json!([
                {"id": "anos:fs.read", "name": "Read", "description": "read a file", "domain": "fs", "tags": []},
                {"id": "anos:fs.write", "name": "Write", "description": "write a file", "domain": "fs", "tags": []},
                {"id": "anos:web.fetch", "name": "Fetch", "description": "download a url", "domain": "web", "tags": []}
            ]),
        })
        .await;

        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);

        let only_fs = client
            .discover(
                Some("fs"),
                crate::options::DiscoverFilter {
                    limit: Some(1),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(only_fs.len(), 1);
        assert!(only_fs[0].id.starts_with("anos:fs"));
    }
}
