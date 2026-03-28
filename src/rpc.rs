use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};

#[derive(Debug, Deserialize)]
pub struct Request {
    #[allow(dead_code)]
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct Response {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Error>,
}

#[derive(Debug, Serialize)]
pub struct Error {
    pub code: i32,
    pub message: String,
}

impl Response {
    pub fn success(id: Option<serde_json::Value>, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<serde_json::Value>, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(Error { code, message }),
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct HelloParams {}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct EventParams {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct BeforeLlmParams {
    #[serde(default)]
    pub messages: Vec<Message>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct AfterLlmParams {
    #[serde(default)]
    pub response: Option<ResponseContent>,
    #[serde(default)]
    pub content: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct ResponseContent {
    #[serde(default)]
    pub content: String,
}

pub fn read_request() -> io::Result<Option<Request>> {
    let stdin = io::stdin();
    let mut line = String::new();
    let mut lock = stdin.lock();

    loop {
        line.clear();
        let bytes_read = lock.read_line(&mut line)?;
        if bytes_read == 0 {
            return Ok(None);
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        match serde_json::from_str::<Request>(trimmed) {
            Ok(req) => return Ok(Some(req)),
            Err(e) => {
                tracing::error!("JSON decode error: {}", e);
                continue;
            }
        }
    }
}

pub fn write_response(response: &Response) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    let json = serde_json::to_string(response)?;
    writeln!(stdout, "{}", json)?;
    stdout.flush()
}
