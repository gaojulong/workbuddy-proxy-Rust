use std::sync::Arc;
use std::time::Instant;

use axum::http::StatusCode;
use futures_util::StreamExt;
use tokio::sync::Mutex;

use crate::config::Config;
use crate::models::timeout_for;
use crate::token::{build_headers, TokenManager};

/// 构建发给上游的 body（与 Python 版一致：移除 stream 字段，强制 stream=true）
pub fn build_wb_body(mut body: serde_json::Value, model: &str) -> serde_json::Value {
    if let Some(obj) = body.as_object_mut() {
        obj.remove("stream");
        obj.insert("stream".to_string(), serde_json::json!(true));
        obj.insert("model".to_string(), serde_json::json!(model));
    }
    body
}

/// 上游响应错误 → 错误 JSON（与 Python 版一致）
fn error_sse(msg: &str) -> String {
    format!("data: {}\n\n", serde_json::json!({"error": msg}))
}

/// 流式转发（与 Python _stream_response 一致，重试状态机）
pub async fn stream_response(
    state: &Arc<AppStateInner>,
    body: serde_json::Value,
    model: String,
    mut sender: tokio::sync::mpsc::Sender<String>,
) {
    let max_attempts = 2;
    let url = format!("{}/v2/chat/completions", state.config.wb_api_base);
    let timeout = timeout_for(&model, state.config.wb_timeout, state.config.wb_reasoning_timeout);

    for attempt in 1..=max_attempts {
        // 1. 获取 token（锁内）
        let access_token = {
            let mut tm = state.token.lock().await;
            tm.get_token_async().await
        };
        if access_token.is_empty() {
            let _ = sender.send(error_sse("no valid token")).await;
            let _ = sender.send("data: [DONE]\n\n".to_string()).await;
            return;
        }

        let (user_id, enterprise_id, domain, department_info) = {
            let tm = state.token.lock().await;
            (
                tm.user_id.clone(),
                tm.enterprise_id.clone(),
                tm.domain.clone(),
                tm.department_info.clone(),
            )
        };
        let headers = build_headers(
            &state.config,
            &access_token,
            &user_id,
            &enterprise_id,
            &domain,
            &department_info,
        );

        let t_start = Instant::now();
        let mut has_content = false;

        // 2. 发起上游请求（流式）
        let resp = state
            .client
            .post(&url)
            .headers(headers)
            .json(&body)
            .timeout(std::time::Duration::from_secs(timeout))
            .send()
            .await;

        match resp {
            Err(e) => {
                tracing::error!("[{}] Upstream timeout (attempt {})", model, attempt);
                if attempt < max_attempts {
                    continue;
                }
                let _ = sender.send(error_sse("upstream timeout")).await;
                let _ = sender.send("data: [DONE]\n\n".to_string()).await;
                return;
            }
            Ok(resp) => {
                let status = resp.status();

                // 3. 401 → 刷新 token 重试
                if status == StatusCode::UNAUTHORIZED {
                    tracing::warn!("[{}] Got 401, refreshing token...", model);
                    let mut tm = state.token.lock().await;
                    tm.refresh_async().await;
                    drop(tm);
                    if attempt < max_attempts {
                        continue;
                    }
                    let _ = sender.send(error_sse("authentication failed")).await;
                    let _ = sender.send("data: [DONE]\n\n".to_string()).await;
                    return;
                }

                // 4. 非 200 → 透传错误
                if status != StatusCode::OK {
                    let text = resp.text().await.unwrap_or_default();
                    let truncated: String = text.chars().take(200).collect();
                    tracing::error!("[{}] Upstream {}: {}", model, status, truncated);
                    let _ = sender
                        .send(format!(
                            "data: {}\n\n",
                            serde_json::json!({"error": text})
                        ))
                        .await;
                    let _ = sender.send("data: [DONE]\n\n".to_string()).await;
                    return;
                }

                // 5. 200 → 逐行消费 SSE
                let mut stream = resp.bytes_stream();
                let mut buf: Vec<u8> = Vec::new();
                let mut done_sent = false;

                // 逐行处理（类似 Python aiter_lines）
                let mut current_line: Vec<u8> = Vec::new();
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(bytes) => {
                            buf.extend_from_slice(&bytes);
                            // 按行拆分处理
                            while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                                let line: Vec<u8> = buf.drain(..=pos).collect();
                                let line = &line[..line.len() - 1]; // 去掉 \n
                                let line_str = String::from_utf8_lossy(line);
                                current_line.clear();
                                current_line.extend_from_slice(line_str.as_bytes());
                                process_line(&current_line, &mut has_content, &mut done_sent, &mut sender).await;
                            }
                        }
                        Err(e) => {
                            tracing::error!("[{}] Read error during stream (attempt {}): {}", model, attempt, e);
                            break;
                        }
                    }
                }
                // 处理剩余 buffer
                if !buf.is_empty() {
                    let line_str = String::from_utf8_lossy(&buf);
                    current_line.clear();
                    current_line.extend_from_slice(line_str.as_bytes());
                    process_line(&current_line, &mut has_content, &mut done_sent, &mut sender).await;
                }

                let elapsed = t_start.elapsed().as_secs_f64();

                // 6. 空响应重试
                if !has_content && attempt < max_attempts {
                    tracing::warn!("[{}] Empty response, retrying... ({:.1}s)", model, elapsed);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    continue;
                }

                // 7. 补发 [DONE]
                if !done_sent {
                    let _ = sender.send("data: [DONE]\n\n".to_string()).await;
                }

                tracing::info!("[{}] stream {:.1}s", model, elapsed);
                return;
            }
        }
    }
}

/// 处理一行 SSE（与 Python 版逻辑一致）
async fn process_line(
    line: &[u8],
    has_content: &mut bool,
    done_sent: &mut bool,
    sender: &mut tokio::sync::mpsc::Sender<String>,
) {
    let line_str = String::from_utf8_lossy(line).to_string();
    if line_str.starts_with("data: ") {
        if line_str == "data: [DONE]" {
            *done_sent = true;
        } else {
            *has_content = true;
        }
        let _ = sender.send(format!("{}\n\n", line_str)).await;
    } else if !line_str.trim().is_empty() {
        *has_content = true;
        let _ = sender.send(format!("data: {}\n\n", line_str)).await;
    }
}

/// 非流式路径（与 Python _non_stream_response 一致）
pub async fn non_stream_response(
    state: &Arc<AppStateInner>,
    body: serde_json::Value,
    model: String,
) -> Result<axum::Json<serde_json::Value>, (StatusCode, String)> {
    let max_attempts = 2;
    let url = format!("{}/v2/chat/completions", state.config.wb_api_base);
    let timeout = timeout_for(&model, state.config.wb_timeout, state.config.wb_reasoning_timeout);

    for attempt in 1..=max_attempts {
        let access_token = {
            let mut tm = state.token.lock().await;
            tm.get_token_async().await
        };
        if access_token.is_empty() {
            return Err((StatusCode::SERVICE_UNAVAILABLE, "No valid WorkBuddy token".to_string()));
        }

        let (user_id, enterprise_id, domain, department_info) = {
            let tm = state.token.lock().await;
            (
                tm.user_id.clone(),
                tm.enterprise_id.clone(),
                tm.domain.clone(),
                tm.department_info.clone(),
            )
        };
        let headers = build_headers(
            &state.config,
            &access_token,
            &user_id,
            &enterprise_id,
            &domain,
            &department_info,
        );

        let t_start = Instant::now();
        let mut collected_content = String::new();
        let mut tool_calls_map: std::collections::BTreeMap<i64, serde_json::Value> = std::collections::BTreeMap::new();
        let mut finish_reason = "stop".to_string();
        let mut resp_model = model.clone();
        let mut usage = serde_json::Value::Null;

        let resp = state
            .client
            .post(&url)
            .headers(headers)
            .json(&body)
            .timeout(std::time::Duration::from_secs(timeout))
            .send()
            .await;

        match resp {
            Err(_) => {
                tracing::error!("[{}] Upstream timeout (attempt {})", model, attempt);
                if attempt < max_attempts {
                    continue;
                }
                return Err((StatusCode::GATEWAY_TIMEOUT, "Upstream timeout".to_string()));
            }
            Ok(resp) => {
                let status = resp.status();

                if status == StatusCode::UNAUTHORIZED {
                    tracing::warn!("[{}] Got 401, refreshing token...", model);
                    let mut tm = state.token.lock().await;
                    tm.refresh_async().await;
                    drop(tm);
                    if attempt < max_attempts {
                        continue;
                    }
                    return Err((StatusCode::UNAUTHORIZED, "Authentication failed".to_string()));
                }

                if status != StatusCode::OK {
                    let text = resp.text().await.unwrap_or_default();
                    return Err((status, text));
                }

                // 聚合 SSE 流
                let mut stream = resp.bytes_stream();
                let mut buf: Vec<u8> = Vec::new();
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(bytes) => {
                            buf.extend_from_slice(&bytes);
                            while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                                let line: Vec<u8> = buf.drain(..=pos).collect();
                                let line = &line[..line.len() - 1];
                                let line_str = String::from_utf8_lossy(line).to_string();
                                let text = line_str.strip_prefix("data: ").unwrap_or("").trim().to_string();
                                if text.is_empty() || text == "[DONE]" {
                                    continue;
                                }
                                if let Ok(chunk) = serde_json::from_str::<serde_json::Value>(&text) {
                                    process_nonstream_chunk(
                                        &chunk,
                                        &mut collected_content,
                                        &mut tool_calls_map,
                                        &mut finish_reason,
                                        &mut resp_model,
                                        &mut usage,
                                    );
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
                if !buf.is_empty() {
                    let line_str = String::from_utf8_lossy(&buf).to_string();
                    let text = line_str.strip_prefix("data: ").unwrap_or("").trim().to_string();
                    if !text.is_empty() && text != "[DONE]" {
                        if let Ok(chunk) = serde_json::from_str::<serde_json::Value>(&text) {
                            process_nonstream_chunk(
                                &chunk,
                                &mut collected_content,
                                &mut tool_calls_map,
                                &mut finish_reason,
                                &mut resp_model,
                                &mut usage,
                            );
                        }
                    }
                }

                // 空响应重试
                if collected_content.is_empty() && tool_calls_map.is_empty() && attempt < max_attempts {
                    tracing::warn!("[{}] Empty response, retrying...", model);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    continue;
                }

                let elapsed = t_start.elapsed().as_secs_f64();
                let prompt_t = usage.get("prompt_tokens").map(|v| v.to_string()).unwrap_or_else(|| "?".to_string());
                let compl_t = usage.get("completion_tokens").map(|v| v.to_string()).unwrap_or_else(|| "?".to_string());
                tracing::info!("[{}] non-stream {:.1}s  prompt={} completion={}", model, elapsed, prompt_t, compl_t);

                // 组装响应（与 Python 版一致）
                let message = if tool_calls_map.is_empty() {
                    serde_json::json!({
                        "role": "assistant",
                        "content": if collected_content.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(collected_content) },
                    })
                } else {
                    let mut msg = serde_json::json!({
                        "role": "assistant",
                        "content": if collected_content.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(collected_content) },
                    });
                    let tcs: Vec<serde_json::Value> = tool_calls_map.into_iter().map(|(_, v)| v).collect();
                    msg["tool_calls"] = serde_json::json!(tcs);
                    msg
                };

                let response = serde_json::json!({
                    "id": format!("chatcmpl-{}", uuid::Uuid::new_v4().simple().to_string()[..12].to_string()),
                    "object": "chat.completion",
                    "created": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
                    "model": resp_model,
                    "choices": [{
                        "index": 0,
                        "message": message,
                        "finish_reason": finish_reason,
                    }],
                    "usage": usage,
                });

                return Ok(axum::Json(response));
            }
        }
    }

    Err((StatusCode::BAD_GATEWAY, "Upstream returned empty response".to_string()))
}

/// 处理非流式聚合中的一个 SSE chunk（与 Python 版逻辑一致）
fn process_nonstream_chunk(
    chunk: &serde_json::Value,
    collected_content: &mut String,
    tool_calls_map: &mut std::collections::BTreeMap<i64, serde_json::Value>,
    finish_reason: &mut String,
    resp_model: &mut String,
    usage: &mut serde_json::Value,
) {
    if let Some(choices) = chunk.get("choices").and_then(|c| c.as_array()) {
        if let Some(choice) = choices.first() {
            if let Some(delta) = choice.get("delta") {
                if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                    collected_content.push_str(content);
                }
                // tool_calls 聚合
                if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                    for tc in tcs {
                        let idx = tc.get("index").and_then(|i| i.as_i64()).unwrap_or(0);
                        let entry = tool_calls_map.entry(idx).or_insert_with(|| {
                            serde_json::json!({
                                "id": "",
                                "type": "function",
                                "function": {"name": "", "arguments": ""},
                            })
                        });
                        if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                            if !id.is_empty() {
                                entry["id"] = serde_json::json!(id);
                            }
                        }
                        if let Some(fn_obj) = tc.get("function") {
                            if let Some(name) = fn_obj.get("name").and_then(|n| n.as_str()) {
                                if let Some(cur) = entry["function"]["name"].as_str() {
                                    entry["function"]["name"] = serde_json::json!(format!("{}{}", cur, name));
                                }
                            }
                            if let Some(args) = fn_obj.get("arguments").and_then(|a| a.as_str()) {
                                if let Some(cur) = entry["function"]["arguments"].as_str() {
                                    entry["function"]["arguments"] = serde_json::json!(format!("{}{}", cur, args));
                                }
                            }
                        }
                    }
                }
            }
            if let Some(fr) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                *finish_reason = fr.to_string();
            }
        }
    }
    if let Some(u) = chunk.get("usage") {
        if !u.is_null() {
            *usage = u.clone();
        }
    }
    if let Some(m) = chunk.get("model").and_then(|m| m.as_str()) {
        *resp_model = m.to_string();
    }
}

/// AppState 内部结构（含连接池）
pub struct AppStateInner {
    pub config: Arc<Config>,
    pub client: reqwest::Client,
    pub token: Mutex<TokenManager>,
    pub api_key: String,
}

pub type AppState = Arc<AppStateInner>;
