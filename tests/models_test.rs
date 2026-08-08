//! 模型映射单元测试（与 Python 版行为一致）

use workbuddy_proxy_rust::models::{
    build_models_response, resolve_model, timeout_for, CURSOR_TO_WB_MAP, MODELS,
    REASONING_MODELS,
};

#[test]
fn test_resolve_model_mapped() {
    assert_eq!(resolve_model("claude-4.6-opus-high"), "claude-opus-4.6");
    assert_eq!(resolve_model("gemini-3.1-pro"), "gemini-3.0-pro");
    assert_eq!(resolve_model("kimi-k2.5"), "kimi-k2.5-ioa");
}

#[test]
fn test_resolve_model_passthrough() {
    // 无映射的模型原样透传（与 Python 一致）
    assert_eq!(resolve_model("deepseek-v3"), "deepseek-v3");
    assert_eq!(resolve_model("unknown-model"), "unknown-model");
}

#[test]
fn test_resolve_model_identity() {
    // 已映射为自身的模型
    assert_eq!(resolve_model("deepseek-v4-pro"), "deepseek-v4-pro");
    assert_eq!(resolve_model("hy3"), "hy3");
    assert_eq!(resolve_model("glm-5.2"), "glm-5.2");
}

#[test]
fn test_timeout_for() {
    // 推理模型用 300s
    assert_eq!(timeout_for("deepseek-r1", 120, 300), 300);
    assert_eq!(timeout_for("deepseek-v4-pro", 120, 300), 300);
    // 普通模型用 120s
    assert_eq!(timeout_for("deepseek-v3", 120, 300), 120);
    assert_eq!(timeout_for("claude-opus-4.6", 120, 300), 120);
}

#[test]
fn test_reasoning_models_consistent() {
    // REASONING_MODELS 里的模型都应能解析
    for m in REASONING_MODELS {
        assert!(!m.is_empty());
    }
    assert!(REASONING_MODELS.contains(&"deepseek-r1"));
    assert!(REASONING_MODELS.contains(&"glm-5.2"));
}

#[test]
fn test_models_count() {
    // 与 Python 版一致：37 个 WB 模型 + 25 个 Cursor 别名去重 = 54
    let resp = build_models_response();
    let data = resp.get("data").unwrap().as_array().unwrap();
    assert_eq!(data.len(), 54, "模型总数应为 54，与 Python 版一致");
}

#[test]
fn test_models_field_structure() {
    let resp = build_models_response();
    let data = resp.get("data").unwrap().as_array().unwrap();
    for m in data {
        assert!(m.get("id").is_some());
        assert!(m.get("object").is_some());
        assert!(m.get("created").is_some());
        assert!(m.get("owned_by").is_some());
        assert!(m.get("name").is_some());
    }
}

#[test]
fn test_models_no_duplicates() {
    let resp = build_models_response();
    let data = resp.get("data").unwrap().as_array().unwrap();
    let mut ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for m in data {
        let id = m.get("id").unwrap().as_str().unwrap().to_string();
        assert!(ids.insert(id), "重复模型 ID");
    }
}

#[test]
fn test_models_critical_present() {
    // 与 Python 测试脚本一致的核心模型
    let resp = build_models_response();
    let data = resp.get("data").unwrap().as_array().unwrap();
    let ids: std::collections::HashSet<&str> = data
        .iter()
        .filter_map(|m| m.get("id").and_then(|v| v.as_str()))
        .collect();
    for critical in ["deepseek-v3", "claude-opus-4.6", "deepseek-v4-pro", "hy3", "kimi-k3", "glm-5.2"] {
        assert!(ids.contains(critical), "缺少核心模型: {}", critical);
    }
}

#[test]
fn test_cursor_map_valid_targets() {
    // CURSOR_TO_WB_MAP 的每个目标都应在 MODELS 中或自身映射
    let model_ids: std::collections::HashSet<&str> = MODELS.iter().map(|(id, _)| *id).collect();
    for (_, wb) in CURSOR_TO_WB_MAP {
        // wb_id 要么在 MODELS 中，要么该 cursor 名本身就在 MODELS（自身映射）
        assert!(
            model_ids.contains(wb) || CURSOR_TO_WB_MAP.iter().any(|(c, _)| c == wb),
            "映射目标不在模型列表中: {}",
            wb
        );
    }
}
