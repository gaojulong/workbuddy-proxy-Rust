/// 与 Python 版 server.py 完全一致的模型数据迁移

/// Cursor 模型名 → WorkBuddy 模型 ID
pub const CURSOR_TO_WB_MAP: &[(&str, &str)] = &[
    // Claude
    ("claude-4.6-opus-high", "claude-opus-4.6"),
    ("claude-4.6-opus-max", "claude-opus-4.6-1m"),
    ("claude-4.6-opus-high-thinking", "claude-opus-4.6"),
    ("claude-4.6-opus-high-thinking-fast", "claude-opus-4.6"),
    ("claude-4.6-opus-max-thinking", "claude-opus-4.6-1m"),
    ("claude-4.6-opus-max-thinking-fast", "claude-opus-4.6-1m"),
    ("claude-4.6-sonnet-medium", "claude-sonnet-4.6"),
    ("claude-4.6-sonnet-medium-thinking", "claude-sonnet-4.6-1m"),
    ("claude-4.5-opus-high", "claude-opus-4.5"),
    ("claude-4.5-opus-high-thinking", "claude-opus-4.5"),
    ("claude-4.5-sonnet", "claude-4.5"),
    ("claude-4.5-sonnet-thinking", "claude-4.5"),
    ("claude-4.5-haiku", "claude-haiku-4.5"),
    ("claude-4.5-haiku-thinking", "claude-haiku-4.5"),
    ("claude-opus-4.6", "claude-opus-4.6"),
    // Gemini
    ("gemini-3.1-pro", "gemini-3.0-pro"),
    ("gemini-3-flash", "gemini-3.1-flash-lite"),
    // Kimi
    ("kimi-k2.5", "kimi-k2.5-ioa"),
    ("kimi-k3", "kimi-k3"),
    ("kimi-k3-1", "kimi-k3-1"),
    // DeepSeek V4
    ("deepseek-v4-pro", "deepseek-v4-pro"),
    ("deepseek-v4-flash", "deepseek-v4-flash"),
    // Hunyuan 3
    ("hy3", "hy3"),
    // GLM 5.x
    ("glm-5.1", "glm-5.1"),
    ("glm-5.2", "glm-5.2"),
];

/// WB 模型 ID → 首选 Cursor 别名（用于 /v1/models）
pub const WB_TO_CURSOR_MAP: &[(&str, &str)] = &[
    ("claude-opus-4.6", "claude-4.6-opus-high"),
    ("claude-opus-4.6-1m", "claude-4.6-opus-max"),
    ("claude-sonnet-4.6", "claude-4.6-sonnet-medium"),
    ("claude-sonnet-4.6-1m", "claude-4.6-sonnet-medium-thinking"),
    ("claude-opus-4.5", "claude-4.5-opus-high"),
    ("claude-4.5", "claude-4.5-sonnet"),
    ("claude-haiku-4.5", "claude-4.5-haiku"),
    ("gemini-3.0-pro", "gemini-3.1-pro"),
    ("gemini-3.1-flash-lite", "gemini-3-flash"),
    ("kimi-k2.5-ioa", "kimi-k2.5"),
    ("kimi-k3", "kimi-k3"),
    ("kimi-k3-1", "kimi-k3-1"),
    ("deepseek-v4-pro", "deepseek-v4-pro"),
    ("deepseek-v4-flash", "deepseek-v4-flash"),
    ("hy3", "hy3"),
    ("glm-5.1", "glm-5.1"),
    ("glm-5.2", "glm-5.2"),
];

/// 推理模型（超时用 300s）
pub const REASONING_MODELS: &[&str] = &[
    "deepseek-r1",
    "deepseek-r1-0528-lkeap",
    "hunyuan-2.0-thinking-ioa",
    "deepseek-v4-pro",
    "deepseek-v4-flash",
    "glm-5.2",
];

/// 可用模型列表
pub const MODELS: &[(&str, &str)] = &[
    // DeepSeek
    ("deepseek-r1", "DeepSeek-R1"),
    ("deepseek-v3", "DeepSeek-V3"),
    ("deepseek-v3.2", "DeepSeek-V3.2"),
    ("deepseek-v3-1", "DeepSeek-V3.1"),
    ("deepseek-v3-0324", "DeepSeek-V3-0324"),
    ("deepseek-v3-1-volc", "DeepSeek-V3-1-Terminus"),
    ("deepseek-v3-0324-lkeap", "DeepSeek-V3-0324-LKEAP"),
    ("deepseek-r1-0528-lkeap", "DeepSeek-R1-0528-LKEAP"),
    ("deepseek-v3-2-volc-ioa", "DeepSeek-V3-2-Volc"),
    ("deepseek-v4-pro", "DeepSeek-V4-Pro"),
    ("deepseek-v4-flash", "DeepSeek-V4-Flash"),
    // Claude
    ("claude-4.5", "Claude-Sonnet-4.5"),
    ("claude-opus-4.5", "Claude-Opus-4.5"),
    ("claude-opus-4.6", "Claude-Opus-4.6"),
    ("claude-opus-4.6-1m", "Claude-Opus-4.6 (1M context)"),
    ("claude-sonnet-4.6", "Claude-Sonnet-4.6"),
    ("claude-sonnet-4.6-1m", "Claude-Sonnet-4.6 (1M context)"),
    ("claude-haiku-4.5", "Claude-Haiku-4.5"),
    // Gemini
    ("gemini-3.0-pro", "Gemini-3.0-Pro"),
    ("gemini-3.1-flash-lite", "Gemini-3.1-Flash-Lite"),
    // GLM
    ("glm-4.6", "GLM-4.6"),
    ("glm-4.7", "GLM-4.7"),
    ("glm-4.7-ioa", "GLM-4.7-IOA"),
    ("glm-5.0-ioa", "GLM-5.0"),
    ("glm-5.0-turbo-ioa", "GLM-5.0-Turbo"),
    ("glm-5v-turbo", "GLM-5v-Turbo"),
    ("glm-5v-turbo-ioa", "GLM-5v-Turbo-IOA"),
    ("glm-5.1", "GLM-5.1"),
    ("glm-5.2", "GLM-5.2"),
    // Hunyuan
    ("hunyuan-2.0-instruct", "Hunyuan-2.0-Instruct"),
    ("hunyuan-2.0-instruct-ioa", "Hunyuan-2.0-Instruct-IOA"),
    ("hunyuan-2.0-thinking-ioa", "Hunyuan-2.0-Thinking"),
    ("hy3", "Hunyuan-3 (Hy3)"),
    // Kimi
    ("kimi-k2.5-ioa", "Kimi-K2.5"),
    ("kimi-k3", "Kimi-K3"),
    ("kimi-k3-1", "Kimi-K3.1"),
    // Default
    ("codewise-default-model-v2", "Default (Codewise)"),
];

/// 与 Python `resolve_model()` 一致：无映射则透传
pub fn resolve_model(model: &str) -> String {
    for (cursor, wb) in CURSOR_TO_WB_MAP {
        if *cursor == model {
            return wb.to_string();
        }
    }
    model.to_string()
}

/// 与 Python `_timeout_for()` 一致
pub fn timeout_for(model: &str, default: u64, reasoning: u64) -> u64 {
    if REASONING_MODELS.contains(&model) {
        reasoning
    } else {
        default
    }
}

/// 构建 /v1/models 响应（与 Python 版顺序一致：先 Cursor 别名，再原始 WB 模型）
pub fn build_models_response() -> serde_json::Value {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut data: Vec<serde_json::Value> = Vec::new();

    // First: Cursor 兼容别名
    for (cursor_name, wb_id) in CURSOR_TO_WB_MAP {
        if seen.insert(cursor_name.to_string()) {
            // 从 MODELS 找显示名
            let display_name = MODELS
                .iter()
                .find(|(id, _)| id == wb_id)
                .map(|(_, name)| name.to_string())
                .unwrap_or_else(|| cursor_name.to_string());
            data.push(serde_json::json!({
                "id": cursor_name,
                "object": "model",
                "created": 1700000000,
                "owned_by": "workbuddy",
                "name": format!("{} (Cursor)", display_name),
            }));
        }
    }

    // Then: 原始 WB 模型
    for (id, name) in MODELS {
        if seen.insert(id.to_string()) {
            data.push(serde_json::json!({
                "id": id,
                "object": "model",
                "created": 1700000000,
                "owned_by": "workbuddy",
                "name": name,
            }));
        }
    }

    serde_json::json!({ "object": "list", "data": data })
}
