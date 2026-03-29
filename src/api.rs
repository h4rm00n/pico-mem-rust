use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::{LlmConfig, EmbeddingConfig};

pub struct ApiClient {
    client: Client,
    llm_config: LlmConfig,
    embedding_config: EmbeddingConfig,
}

#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: String,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessageContent,
}

#[derive(Deserialize)]
struct ChatMessageContent {
    content: String,
}

impl ApiClient {
    pub fn new(llm_config: LlmConfig, embedding_config: EmbeddingConfig) -> Self {
        Self {
            client: Client::new(),
            llm_config,
            embedding_config,
        }
    }

    pub async fn get_embedding(&self, text: &str) -> Result<Vec<f32>> {
        let request = EmbeddingRequest {
            model: self.embedding_config.model.clone(),
            input: text.to_string(),
        };

        let response = self
            .client
            .post(format!("{}/embeddings", self.embedding_config.base_url))
            .header("Authorization", format!("Bearer {}", self.embedding_config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await?;

        response.error_for_status_ref()?;
        
        let embedding_response: EmbeddingResponse = response.json().await?;
        
        embedding_response
            .data
            .first()
            .map(|d| d.embedding.clone())
            .ok_or_else(|| anyhow::anyhow!("No embedding returned"))
    }

    pub async fn summarize(&self, text_block: &str) -> Result<String> {
        let prompt = self.llm_config.summarize_prompt.replace("{text_block}", text_block);

        let request = ChatRequest {
            model: self.llm_config.model.clone(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt,
            }],
            temperature: 0.1,
            max_tokens: 300,
        };

        let response = self
            .client
            .post(format!("{}/chat/completions", self.llm_config.base_url))
            .header("Authorization", format!("Bearer {}", self.llm_config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .timeout(std::time::Duration::from_secs(180))
            .send()
            .await?;

        response.error_for_status_ref()?;
        
        let chat_response: ChatResponse = response.json().await?;
        
        chat_response
            .choices
            .first()
            .map(|c| c.message.content.trim().to_string())
            .ok_or_else(|| anyhow::anyhow!("No response returned"))
    }
}