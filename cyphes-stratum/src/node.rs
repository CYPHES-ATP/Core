//! JSON-RPC client for the CYPHES node.

use std::{path::Path, time::Duration};

use serde::de::DeserializeOwned;
use serde_json::{json, Value};

/// An RPC failure: unreachable node, HTTP error, or an RPC error response.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct RpcError(String);

/// The node's JSON-RPC endpoint.
#[derive(Clone)]
pub struct Node {
    url: String,
    auth: Option<(String, String)>,
    http: reqwest::Client,
}

impl Node {
    /// `url` is the node's RPC address, such as `http://127.0.0.1:2975`.
    /// `cookie` is the node's RPC cookie file, when cookie authentication is
    /// on (the default): it holds `__cookie__:<password>`.
    pub fn new(url: &str, cookie: Option<&Path>) -> Result<Node, RpcError> {
        let auth = match cookie {
            Some(path) => {
                let cookie = std::fs::read_to_string(path)
                    .map_err(|e| RpcError(format!("read {}: {e}", path.display())))?;
                let (user, password) = cookie
                    .trim()
                    .split_once(':')
                    .ok_or_else(|| RpcError(format!("{} is not a cookie file", path.display())))?;
                Some((user.to_owned(), password.to_owned()))
            }
            None => None,
        };
        let http = reqwest::Client::builder()
            // Long-polled templates can take minutes to change.
            .timeout(Duration::from_secs(600))
            .build()
            .map_err(|e| RpcError(e.to_string()))?;
        Ok(Node {
            url: url.to_owned(),
            auth,
            http,
        })
    }

    /// Calls `method` and returns its `result`.
    pub async fn call<T: DeserializeOwned>(
        &self,
        method: &str,
        params: Value,
    ) -> Result<T, RpcError> {
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
        let mut request = self.http.post(&self.url).json(&body);
        if let Some((user, password)) = &self.auth {
            request = request.basic_auth(user, Some(password));
        }
        let response: Value = request
            .send()
            .await
            .map_err(|e| RpcError(format!("{method}: {e}")))?
            .json()
            .await
            .map_err(|e| RpcError(format!("{method}: {e}")))?;
        match response.get("error") {
            Some(error) if !error.is_null() => Err(RpcError(format!("{method}: {error}"))),
            _ => serde_json::from_value(response["result"].clone())
                .map_err(|e| RpcError(format!("{method}: unexpected result: {e}"))),
        }
    }
}
