use serde_json::Value;

/// 与 Python `_parse_jwt_claims()` 一致：无签名验证解析 JWT。
/// jsonwebtoken 9.x 移除了 dangerous_insecure_decode，这里手动 base64 解码 payload。
#[derive(Debug, Clone, Default)]
pub struct Claims {
    pub user_id: String,
    pub enterprise_id: String,
    pub domain: String,
    pub exp: i64,
}

pub fn parse_jwt_claims(token: &str) -> Claims {
    let mut claims = Claims::default();

    // JWT 格式: header.payload.signature
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() < 2 {
        return claims;
    }
    let payload_b64 = parts[1];

    // base64url → 字节（填充处理）
    let payload_bytes = match base64url_decode(payload_b64) {
        Some(b) => b,
        None => return claims,
    };

    // 解析 JSON
    let payload: Value = match serde_json::from_slice(&payload_bytes) {
        Ok(v) => v,
        Err(_) => return claims,
    };

    claims.user_id = payload.get("sub").and_then(|v| v.as_str()).unwrap_or("").to_string();

    if let Some(iss) = payload.get("iss").and_then(|v| v.as_str()) {
        // iss 格式: https://<domain>/auth/realms/sso-<enterprise_id>
        // enterprise_id: /sso-([^/]+)$
        if let Some(idx) = iss.rfind("/sso-") {
            let rest = &iss[idx + 5..];
            let ent = rest.split('/').next().unwrap_or("");
            claims.enterprise_id = ent.to_string();
        }
        // domain: https?://([^/]+)
        let domain = iss
            .strip_prefix("https://")
            .or_else(|| iss.strip_prefix("http://"))
            .and_then(|rest| rest.split('/').next())
            .unwrap_or("");
        claims.domain = domain.to_string();
    }

    claims.exp = payload.get("exp").and_then(|v| v.as_i64()).unwrap_or(0);

    claims
}

/// base64url 解码（无填充）
fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    let padded = match input.len() % 4 {
        0 => input.to_string(),
        2 => format!("{}==", input),
        3 => format!("{}=", input),
        _ => return None,
    };
    base64::engine::general_purpose::URL_SAFE
        .decode(padded.as_bytes())
        .ok()
}

/// 判断 token 是否过期（提前 300s，与 Python `_is_expired` 一致）
pub fn is_expired(token: &str) -> bool {
    if token.is_empty() {
        return true;
    }
    let claims = parse_jwt_claims(token);
    if claims.exp == 0 {
        return false;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    now > (claims.exp - 300)
}

/// 记录 token 有效期（与 Python `_log_token_info` 一致）
pub fn log_token_info(token: &str) {
    let claims = parse_jwt_claims(token);
    if claims.exp > 0 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let hours = (claims.exp - now) as f64 / 3600.0;
        tracing::info!("Token valid, expires in {:.1}h", hours);
    } else {
        tracing::warn!("Could not decode token");
    }
}
