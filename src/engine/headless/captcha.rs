//! CAPTCHA detection and solving (reference crawler `pkg/engine/headless/captcha` with
//! the capsolver provider as a native HTTP client).

/// Detect common CAPTCHA providers in page content (reference crawler `identify`).
pub fn looks_like_captcha(content: &str) -> bool {
    let lower = content.to_lowercase();
    lower.contains("recaptcha")
        || lower.contains("g-recaptcha")
        || lower.contains("hcaptcha")
        || lower.contains("turnstile")
        || lower.contains("captcha-container")
}

/// The CAPTCHA provider/sitekey type detected in the page.
pub fn detect_kind(content: &str) -> Option<&'static str> {
    let lower = content.to_lowercase();
    if lower.contains("recaptcha") || lower.contains("g-recaptcha") {
        Some("ReCaptchaV2TaskProxyLess")
    } else if lower.contains("hcaptcha") {
        Some("HCaptchaTaskProxyLess")
    } else if lower.contains("turnstile") {
        Some("AntiTurnstileTaskProxyLess")
    } else {
        None
    }
}

/// Extract a sitekey parameter from the page (best-effort).
pub fn extract_sitekey(content: &str) -> Option<String> {
    for marker in ["data-sitekey=\"", "data-sitekey='", "sitekey: '"] {
        if let Some(idx) = content.find(marker) {
            let rest = &content[idx + marker.len()..];
            let end = rest.find(|c| c == '"' || c == '\'').unwrap_or(0);
            if end > 0 {
                return Some(rest[..end].to_string());
            }
        }
    }
    None
}

/// Solve a CAPTCHA through the capsolver API (blocking; called from the render
/// thread). Returns the solution token.
pub fn solve_with_provider_blocking(
    provider: &str,
    api_key: &str,
    page_content: &str,
    page_url: &str,
) -> Result<String, String> {
    if provider != "capsolver" {
        return Err(format!("unsupported captcha solver provider: {provider}"));
    }
    let task_type = detect_kind(page_content)
        .ok_or_else(|| "no known captcha kind detected".to_string())?;
    let site_key = extract_sitekey(page_content)
        .ok_or_else(|| "no captcha sitekey found".to_string())?;

    let payload = serde_json::json!({
        "clientKey": api_key,
        "task": {
            "type": task_type,
            "websiteURL": page_url,
            "websiteKey": site_key,
        }
    });

    // Blocking HTTP via a short-lived runtime-free client is not possible with
    // reqwest (async); use ureq-style manual call through std? Use tokio
    // block_in_place-free approach: spawn a tiny thread with its own runtime.
    let handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("runtime error: {e}"))?;
        rt.block_on(async move {
            let client = reqwest::Client::new();
            let resp = client
                .post("https://api.capsolver.com/createTask")
                .json(&payload)
                .timeout(std::time::Duration::from_secs(30))
                .send()
                .await
                .map_err(|e| format!("capsolver request failed: {e}"))?;
            let body: serde_json::Value =
                resp.json().await.map_err(|e| format!("capsolver decode failed: {e}"))?;
            body.get("taskId")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| format!("capsolver createTask failed: {body}"))
        })
    });

    let task_id = handle
        .join()
        .map_err(|_| "capsolver thread panicked".to_string())??;

    // Poll getTaskResult up to ~2 minutes.
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_secs(3));
        let handle = std::thread::spawn({
            let api_key = api_key.to_string();
            let task_id = task_id.clone();
            move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| format!("runtime error: {e}"))?;
                rt.block_on(async move {
                    let client = reqwest::Client::new();
                    let resp = client
                        .post("https://api.capsolver.com/getTaskResult")
                        .json(&serde_json::json!({
                            "clientKey": api_key,
                            "taskId": task_id,
                        }))
                        .timeout(std::time::Duration::from_secs(30))
                        .send()
                        .await
                        .map_err(|e| format!("capsolver poll failed: {e}"))?;
                    let body: serde_json::Value = resp
                        .json()
                        .await
                        .map_err(|e| format!("capsolver decode failed: {e}"))?;
                    Ok::<Option<String>, String>(
                        body.get("solution")
                            .and_then(|s| s.get("gRecaptchaResponse").or_else(|| s.get("token")))
                            .and_then(|t| t.as_str())
                            .map(|s| s.to_string()),
                    )
                })
            }
        });
        let result: Option<String> = handle
            .join()
            .map_err(|_| "capsolver thread panicked".to_string())??;
        if let Some(token) = result {
            return Ok(token);
        }
    }
    Err("capsolver task did not complete in time".into())
}

/// JS that injects a solved token into the page callback input.
pub fn token_injection_js(token: &str) -> String {
    format!(
        r#"
(function() {{
  var token = {token:?};
  var el = document.getElementById('g-recaptcha-response')
        || document.querySelector('[name=h-captcha-response]')
        || document.querySelector('[name=cf-turnstile-response]');
  if (el) {{ el.value = token; el.style.display = 'none'; }}
  var cb = document.querySelector('[name=g-recaptcha-response]') || el;
  if (cb && window.__celestia_captcha_callback) {{ window.__celestia_captcha_callback(token); }}
}})();
"#,
        token = token
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_captcha() {
        assert!(looks_like_captcha("<div class=\"g-recaptcha\" data-sitekey=\"abc\"></div>"));
        assert!(looks_like_captcha("hcaptcha challenge"));
        assert!(!looks_like_captcha("normal page"));
    }

    #[test]
    fn test_detect_kind() {
        assert_eq!(detect_kind("<div class=g-recaptcha>"), Some("ReCaptchaV2TaskProxyLess"));
        assert_eq!(detect_kind("use hcaptcha here"), Some("HCaptchaTaskProxyLess"));
        assert_eq!(detect_kind("cf turnstile"), Some("AntiTurnstileTaskProxyLess"));
        assert_eq!(detect_kind("plain"), None);
    }

    #[test]
    fn test_extract_sitekey() {
        let page = "<div class=\"g-recaptcha\" data-sitekey=\"6Lc_abCd\"></div>";
        assert_eq!(extract_sitekey(page).as_deref(), Some("6Lc_abCd"));
        assert_eq!(extract_sitekey("no key"), None);
    }

    #[test]
    fn test_token_injection_js() {
        let js = token_injection_js("TOK");
        assert!(js.contains("TOK"));
        assert!(js.contains("g-recaptcha-response"));
    }

    #[test]
    fn test_unsupported_provider() {
        assert!(solve_with_provider_blocking("nope", "k", "x", "https://x").is_err());
    }
}
