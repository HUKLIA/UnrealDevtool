use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Local LLM servers this app knows how to talk to. Both are OpenAI-adjacent
/// but use different endpoints/wire formats, hence the two branches
/// throughout this module rather than one shared client.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LlmProvider {
    Ollama,
    LmStudio,
}

impl LlmProvider {
    fn base_url(self) -> &'static str {
        match self {
            LlmProvider::Ollama   => "http://localhost:11434",
            LlmProvider::LmStudio => "http://localhost:1234",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            LlmProvider::Ollama   => "Ollama",
            LlmProvider::LmStudio => "LM Studio",
        }
    }
}

#[derive(Clone, Serialize)]
pub struct ChatMessage {
    pub role:    String, // "system" | "user" | "assistant"
    pub content: String,
}

/// Short-timeout agent for reachability/model-listing probes — this must
/// fail fast (not hang the caller) since "nothing is listening on this
/// port" is the expected, common case when a server isn't running.
fn probe_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(400))
        .timeout(Duration::from_secs(2))
        .build()
}

/// Detects which local LLM servers are reachable right now and what models
/// each has available. Meant to run on a background thread — even a
/// localhost network call shouldn't happen on the UI thread.
pub fn detect_providers() -> Vec<(LlmProvider, Vec<String>)> {
    let mut found = Vec::new();
    if let Some(models) = list_models(LlmProvider::Ollama)   { found.push((LlmProvider::Ollama, models)); }
    if let Some(models) = list_models(LlmProvider::LmStudio) { found.push((LlmProvider::LmStudio, models)); }
    found
}

fn list_models(provider: LlmProvider) -> Option<Vec<String>> {
    match provider {
        LlmProvider::Ollama => {
            let resp = probe_agent().get(&format!("{}/api/tags", provider.base_url())).call().ok()?;
            let val: serde_json::Value = resp.into_json().ok()?;
            Some(val.get("models")?.as_array()?.iter()
                .filter_map(|m| m.get("name")?.as_str().map(str::to_string))
                .collect())
        }
        LlmProvider::LmStudio => {
            let resp = probe_agent().get(&format!("{}/v1/models", provider.base_url())).call().ok()?;
            let val: serde_json::Value = resp.into_json().ok()?;
            Some(val.get("data")?.as_array()?.iter()
                .filter_map(|m| m.get("id")?.as_str().map(str::to_string))
                .collect())
        }
    }
}

#[derive(Deserialize)]
struct OllamaMsg { content: String }
#[derive(Deserialize)]
struct OllamaChunk { message: Option<OllamaMsg>, done: bool }

#[derive(Deserialize)]
struct OpenAiDelta { content: Option<String> }
#[derive(Deserialize)]
struct OpenAiChoice { delta: OpenAiDelta }
#[derive(Deserialize)]
struct OpenAiChunk { choices: Vec<OpenAiChoice> }

/// Streams a chat completion, calling `on_token` for each incremental piece
/// of text as it arrives. Blocking — run on a background thread. Checks
/// `cancel` between chunks so a user-initiated stop takes effect without
/// waiting for the rest of the response.
pub fn stream_chat(
    provider: LlmProvider,
    model: &str,
    messages: &[ChatMessage],
    cancel: &AtomicBool,
    mut on_token: impl FnMut(&str),
) -> Result<(), String> {
    // No overall response timeout — generation can legitimately run long;
    // only the initial connect needs a bound.
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(800))
        .build();
    let body = serde_json::json!({ "model": model, "messages": messages, "stream": true });

    match provider {
        LlmProvider::Ollama => {
            let resp = agent.post(&format!("{}/api/chat", provider.base_url()))
                .send_json(body)
                .map_err(|e| format!("Ollama request failed: {e}"))?;
            for line in BufReader::new(resp.into_reader()).lines() {
                if cancel.load(Ordering::Relaxed) { return Ok(()); }
                let line = line.map_err(|e| e.to_string())?;
                if line.trim().is_empty() { continue; }
                let Ok(chunk) = serde_json::from_str::<OllamaChunk>(&line) else { continue };
                if let Some(msg) = chunk.message { on_token(&msg.content); }
                if chunk.done { break; }
            }
        }
        LlmProvider::LmStudio => {
            let resp = agent.post(&format!("{}/v1/chat/completions", provider.base_url()))
                .send_json(body)
                .map_err(|e| format!("LM Studio request failed: {e}"))?;
            for line in BufReader::new(resp.into_reader()).lines() {
                if cancel.load(Ordering::Relaxed) { return Ok(()); }
                let line = line.map_err(|e| e.to_string())?;
                let Some(data) = line.strip_prefix("data: ") else { continue };
                if data == "[DONE]" { break; }
                let Ok(chunk) = serde_json::from_str::<OpenAiChunk>(data) else { continue };
                if let Some(content) = chunk.choices.first().and_then(|c| c.delta.content.as_deref()) {
                    on_token(content);
                }
            }
        }
    }
    Ok(())
}
