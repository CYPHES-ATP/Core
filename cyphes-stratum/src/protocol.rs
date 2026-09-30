//! Beam's stratum protocol, as BeamHash III GPU miners speak it.
//!
//! Newline-delimited JSON-RPC 2.0 over TCP, following Beam's own server and
//! client (`beam/pow/stratum.{h,cpp}`, `stratum_server.cpp`,
//! `miner_client.cpp`). Every message carries a string `id` and a `method`:
//!
//! - miner → bridge: `login` (`api_key`), `solution` (`nonce`: 8 bytes hex,
//!   `output`: the 104-byte solution hex), both with the job's `id`;
//! - bridge → miner: `job` (`input`: the 32-byte PoW input hex, `difficulty`:
//!   Beam's packed difficulty, `height`) and `result` (`code`,
//!   `description`, and on login `nonceprefix`, `forkheight`, `forkheight2`).

use serde_json::{json, Map, Value};

/// `STRATUM_RESULTS` in Beam's `stratum.h`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Code {
    Success = 0,
    Accepted = 1,
    Rejected = 2,
    Expired = 3,
    MessageCorrupted = -32000,
    UnknownMethod = -32001,
    EmptyId = -32002,
    LoginFailed = -32003,
}

impl Code {
    /// The `description` Beam sends with each code.
    pub fn description(self) -> &'static str {
        match self {
            Code::Success => "Success",
            Code::Accepted => "accepted",
            Code::Rejected => "rejected",
            Code::Expired => "expired",
            Code::MessageCorrupted => "Message corrupted",
            Code::UnknownMethod => "Unknown method",
            Code::EmptyId => "ID is empty",
            Code::LoginFailed => "Login failed",
        }
    }
}

/// A message from a miner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Login {
        id: String,
        api_key: String,
    },
    Solution {
        id: String,
        nonce: String,
        output: String,
    },
    /// A well-formed message with a method the bridge does not handle; Beam's
    /// server ignores these.
    Other {
        id: String,
        method: String,
    },
}

/// Parses one line from a miner. Errors are the codes Beam's parser returns
/// (`parse_base`), after which Beam drops the connection.
pub fn parse(line: &str) -> Result<Request, Code> {
    let value: Value = serde_json::from_str(line).map_err(|_| Code::MessageCorrupted)?;
    let object = value.as_object().ok_or(Code::MessageCorrupted)?;
    let string = |key: &str| -> Result<String, Code> {
        object
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or(Code::MessageCorrupted)
    };

    let id = string("id")?;
    if id.is_empty() {
        return Err(Code::EmptyId);
    }
    let method = string("method")?;
    match method.as_str() {
        "login" => Ok(Request::Login {
            id,
            api_key: string("api_key")?,
        }),
        "solution" => Ok(Request::Solution {
            id,
            nonce: string("nonce")?,
            output: string("output")?,
        }),
        "job" | "result" | "cancel" => Ok(Request::Other { id, method }),
        _ => Err(Code::UnknownMethod),
    }
}

fn message(id: &str, method: &str, fields: Map<String, Value>) -> String {
    let mut object = Map::new();
    object.insert("jsonrpc".into(), json!("2.0"));
    object.insert("id".into(), json!(id));
    object.insert("method".into(), json!(method));
    object.extend(fields);
    let mut line = Value::Object(object).to_string();
    line.push('\n');
    line
}

fn result_fields(code: Code) -> Map<String, Value> {
    let mut fields = Map::new();
    fields.insert("code".into(), json!(code as i32));
    fields.insert("description".into(), json!(code.description()));
    fields
}

/// A `result` answering the message with `id`.
pub fn result(id: &str, code: Code) -> String {
    message(id, "result", result_fields(code))
}

/// The `result` for an accepted solution that was also a block.
pub fn block_found(id: &str, block_hash: &str) -> String {
    let mut fields = result_fields(Code::Accepted);
    fields.insert("blockhash".into(), json!(block_hash));
    message(id, "result", fields)
}

/// The `result` for a login.
///
/// Miners choose BeamHash I, II or III by comparing the job height with
/// `forkheight` and `forkheight2`, and assume Beam mainnet's fork heights
/// when these are missing. CYPHES uses BeamHash III from genesis, so both
/// are 0.
pub fn login_result(id: &str, code: Code, nonce_prefix: &str) -> String {
    let mut fields = result_fields(code);
    if !nonce_prefix.is_empty() {
        fields.insert("nonceprefix".into(), json!(nonce_prefix));
    }
    fields.insert("forkheight".into(), json!(0));
    fields.insert("forkheight2".into(), json!(0));
    message(id, "result", fields)
}

/// A `job`: find `nonce` and `output` for `input` meeting `difficulty`.
pub fn job(id: &str, input: &[u8; 32], difficulty: u32, height: u32) -> String {
    let mut fields = Map::new();
    fields.insert("input".into(), json!(hex::encode(input)));
    fields.insert("difficulty".into(), json!(difficulty));
    fields.insert("height".into(), json!(height));
    message(id, "job", fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_what_beam_miners_send() {
        // As built by Beam's `append_json_msg(Login)` and `(Solution)`.
        assert_eq!(
            parse(r#"{"api_key":"rig1","id":"login","jsonrpc":"2.0","method":"login"}"#),
            Ok(Request::Login {
                id: "login".into(),
                api_key: "rig1".into()
            })
        );
        let output = "ab".repeat(104);
        let line = format!(
            r#"{{"id":"7","jsonrpc":"2.0","method":"solution","nonce":"0011223344556677","output":"{output}"}}"#
        );
        assert_eq!(
            parse(&line),
            Ok(Request::Solution {
                id: "7".into(),
                nonce: "0011223344556677".into(),
                output
            })
        );
    }

    #[test]
    fn rejects_what_beam_rejects() {
        assert_eq!(parse("not json"), Err(Code::MessageCorrupted));
        assert_eq!(
            parse(r#"{"id":1,"method":"login"}"#),
            Err(Code::MessageCorrupted)
        );
        assert_eq!(
            parse(r#"{"id":"","method":"login","api_key":"x"}"#),
            Err(Code::EmptyId)
        );
        assert_eq!(
            parse(r#"{"id":"1","method":"mine"}"#),
            Err(Code::UnknownMethod)
        );
        assert_eq!(
            parse(r#"{"id":"1","method":"solution","nonce":"00"}"#),
            Err(Code::MessageCorrupted)
        );
        assert_eq!(
            parse(r#"{"id":"1","method":"cancel"}"#),
            Ok(Request::Other {
                id: "1".into(),
                method: "cancel".into()
            })
        );
    }

    #[test]
    fn writes_what_beam_miners_parse() {
        let parse_line = |line: String| -> Value {
            assert!(line.ends_with('\n') && !line[..line.len() - 1].contains('\n'));
            serde_json::from_str(&line).expect("json")
        };

        let job = parse_line(job("12", &[0xab; 32], 0x0100_0000, 42));
        assert_eq!(job["jsonrpc"], "2.0");
        assert_eq!(job["id"], "12");
        assert_eq!(job["method"], "job");
        assert_eq!(job["input"], "ab".repeat(32));
        assert_eq!(job["difficulty"], 0x0100_0000);
        assert_eq!(job["height"], 42);

        let login = parse_line(login_result("login", Code::Success, "a1"));
        assert_eq!(login["method"], "result");
        assert_eq!(login["code"], 0);
        assert_eq!(login["description"], "Success");
        assert_eq!(login["nonceprefix"], "a1");
        assert_eq!(login["forkheight"], 0);
        assert_eq!(login["forkheight2"], 0);
        assert!(parse_line(login_result("login", Code::Success, ""))
            .get("nonceprefix")
            .is_none());

        let found = parse_line(block_found("12", "00ff"));
        assert_eq!(
            (found["code"].clone(), found["blockhash"].clone()),
            (json!(1), json!("00ff"))
        );
        let expired = parse_line(result("12", Code::Expired));
        assert_eq!(
            (expired["code"].clone(), expired["description"].clone()),
            (json!(3), json!("expired"))
        );
    }
}
