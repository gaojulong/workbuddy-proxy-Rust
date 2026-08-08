//! JWT 解析单元测试（与 Python 版行为一致）

use workbuddy_proxy_rust::jwt::{is_expired, parse_jwt_claims};

/// 构造一个测试 JWT（header.payload.signature，payload 不签名）
fn make_jwt(payload: &serde_json::Value) -> String {
    use base64::Engine;
    let header = serde_json::json!({"alg": "none", "typ": "JWT"});
    let h = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(header.to_string());
    let p = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string());
    format!("{}.{}.sig", h, p)
}

#[test]
fn test_parse_claims_full() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let jwt = make_jwt(&serde_json::json!({
        "sub": "user_1234567890",
        "iss": "https://auth.example.com/auth/realms/sso-enterprise_abc",
        "exp": now + 3600,
    }));
    let claims = parse_jwt_claims(&jwt);
    assert_eq!(claims.user_id, "user_1234567890");
    assert_eq!(claims.enterprise_id, "enterprise_abc");
    assert_eq!(claims.domain, "auth.example.com");
    assert!(claims.exp > now);
}

#[test]
fn test_parse_claims_empty_iss() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let jwt = make_jwt(&serde_json::json!({
        "sub": "u1",
        "exp": now + 100,
    }));
    let claims = parse_jwt_claims(&jwt);
    assert_eq!(claims.user_id, "u1");
    assert_eq!(claims.enterprise_id, "");
    assert_eq!(claims.domain, "");
}

#[test]
fn test_parse_claims_invalid() {
    // 无效 token
    let claims = parse_jwt_claims("not-a-jwt");
    assert_eq!(claims.user_id, "");
    assert_eq!(claims.enterprise_id, "");
    // 空 token
    let claims = parse_jwt_claims("");
    assert_eq!(claims.user_id, "");
}

#[test]
fn test_is_expired_empty() {
    assert!(is_expired(""));
}

#[test]
fn test_is_expired_fresh() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let jwt = make_jwt(&serde_json::json!({"exp": now + 3600}));
    assert!(!is_expired(&jwt));
}

#[test]
fn test_is_expired_past() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let jwt = make_jwt(&serde_json::json!({"exp": now - 100}));
    assert!(is_expired(&jwt));
}

#[test]
fn test_is_expired_within_300s_buffer() {
    // 提前 300s 视为过期（与 Python 一致）
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let jwt = make_jwt(&serde_json::json!({"exp": now + 100})); // 100s 后过期 < 300s 缓冲
    assert!(is_expired(&jwt));
}
