use std::time::Duration;

use reqwest::{
    header::{ACCEPT, CONTENT_TYPE, USER_AGENT},
    StatusCode,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct AiSettings {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

#[derive(Debug, Clone)]
pub struct AiConnectionTest {
    pub model_count: Option<usize>,
    pub chat_ok: bool,
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const USER_AGENT_VALUE: &str = "StudyPulse/0.2.2";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<AiMessage>,
    temperature: f32,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: AiMessage,
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelInfo>,
}

#[derive(Debug, Deserialize)]
struct ModelInfo {
    id: String,
}

pub async fn chat_completion(
    settings: &AiSettings,
    messages: Vec<AiMessage>,
) -> Result<String, String> {
    let endpoint = endpoint(settings, "chat/completions");
    let request = ChatCompletionRequest {
        model: settings.model.clone(),
        messages,
        temperature: 0.8,
    };

    let parsed: ChatCompletionResponse = post_json(&endpoint, &settings.api_key, &request).await?;
    parsed
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message.content)
        .filter(|content| !content.trim().is_empty())
        .ok_or_else(|| "AI 响应没有返回有效内容。".into())
}

pub async fn test_connection(settings: &AiSettings) -> Result<AiConnectionTest, String> {
    let model_count = match list_models(settings).await {
        Ok(models) => Some(models.len()),
        Err(models_error) => match test_chat(settings).await {
            Ok(()) => {
                return Ok(AiConnectionTest {
                    model_count: None,
                    chat_ok: true,
                });
            }
            Err(chat_error) => {
                return Err(format!(
                    "模型列表检测失败：{}；聊天测试失败：{}",
                    models_error, chat_error
                ));
            }
        },
    };

    test_chat(settings).await?;
    Ok(AiConnectionTest {
        model_count,
        chat_ok: true,
    })
}

pub async fn list_models(settings: &AiSettings) -> Result<Vec<String>, String> {
    let endpoint = endpoint(settings, "models");
    let response: ModelsResponse = get_json(&endpoint, &settings.api_key).await?;
    Ok(response.data.into_iter().map(|model| model.id).collect())
}

async fn test_chat(settings: &AiSettings) -> Result<(), String> {
    let reply = chat_completion(
        settings,
        vec![AiMessage {
            role: "user".into(),
            content: "请只回复 OK，用于测试 API 是否可用。".into(),
        }],
    )
    .await?;

    if reply.trim().is_empty() {
        Err("AI 接口已响应，但内容为空。".into())
    } else {
        Ok(())
    }
}

async fn post_json<TResponse: for<'de> Deserialize<'de>, TRequest: Serialize>(
    endpoint: &str,
    api_key: &str,
    request: &TRequest,
) -> Result<TResponse, String> {
    let client = http_client()?;
    let response = client
        .post(endpoint)
        .bearer_auth(api_key)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(ACCEPT, "application/json")
        .header(CONTENT_TYPE, "application/json")
        .json(request)
        .send()
        .await
        .map_err(readable_transport_error)?;

    parse_response(response).await
}

async fn get_json<TResponse: for<'de> Deserialize<'de>>(
    endpoint: &str,
    api_key: &str,
) -> Result<TResponse, String> {
    let client = http_client()?;
    let response = client
        .get(endpoint)
        .bearer_auth(api_key)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(ACCEPT, "application/json")
        .send()
        .await
        .map_err(readable_transport_error)?;

    parse_response(response).await
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|error| format!("AI HTTP 客户端初始化失败: {error}"))
}

async fn parse_response<TResponse: for<'de> Deserialize<'de>>(
    response: reqwest::Response,
) -> Result<TResponse, String> {
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| format!("AI 响应读取失败: {error}"))?;
    if !status.is_success() {
        return Err(readable_http_error(status, &text));
    }

    serde_json::from_str(&text).map_err(|error| {
        format!(
            "AI 响应不是 OpenAI 兼容 JSON 格式: {}；响应摘要: {}",
            error,
            summarize_error_body(&text)
        )
    })
}

fn endpoint(settings: &AiSettings, path: &str) -> String {
    format!("{}/{}", settings.base_url.trim_end_matches('/'), path)
}

fn readable_transport_error(error: reqwest::Error) -> String {
    let message = error.to_string();
    if error.is_timeout() {
        "AI 请求超时，请检查网络或 API 服务状态。".into()
    } else if error.is_connect() {
        format!("AI 连接失败，请检查 API URL 或网络状态: {message}")
    } else if message.contains("connection closed")
        || message.contains("connection reset")
        || message.contains("接收时发生错误")
        || message.contains("closed before")
    {
        format!("API 网关主动断开连接，请稍后重试或切换供应商: {message}")
    } else {
        format!("AI 请求失败: {message}")
    }
}

fn readable_http_error(status: StatusCode, body: &str) -> String {
    let body = summarize_error_body(body);
    let status_code = status.as_u16();
    if status == StatusCode::FORBIDDEN {
        if body.is_empty() {
            return "服务端返回 403 Forbidden，可能是 Key、来源限制或网关策略导致。".into();
        }
        return format!(
            "服务端返回 403 Forbidden，可能是 Key、来源限制或网关策略导致: {body}"
        );
    }

    if status == StatusCode::UNAUTHORIZED {
        if body.is_empty() {
            return "服务端返回 401 Unauthorized，请检查 API Key。".into();
        }
        return format!("服务端返回 401 Unauthorized，请检查 API Key: {body}");
    }

    if status == StatusCode::NOT_FOUND {
        if body.is_empty() {
            return "服务端返回 404，请检查 Base URL 或模型接口路径。".into();
        }
        return format!("服务端返回 404，请检查 Base URL 或模型接口路径: {body}");
    }

    if body.is_empty() {
        format!("AI API 返回 HTTP {status_code}。")
    } else {
        format!("AI API 返回 HTTP {status_code}: {body}")
    }
}

fn summarize_error_body(body: &str) -> String {
    let compact = body.split_whitespace().collect::<Vec<_>>().join(" ");
    redact_api_like_secrets(&compact)
        .chars()
        .take(500)
        .collect()
}

fn redact_api_like_secrets(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        if value[index..].starts_with("sk-") {
            output.push_str("[redacted-api-key]");
            while let Some((_, next)) = chars.peek() {
                if next.is_ascii_alphanumeric() || *next == '-' || *next == '_' {
                    chars.next();
                } else {
                    break;
                }
            }
        } else {
            output.push(ch);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_403_has_specific_message_and_redacts_key() {
        let api_key = "sk-test-redacted-secret";
        let error = readable_http_error(
            StatusCode::FORBIDDEN,
            &format!(r#"{{"error":"bad key {api_key}"}}"#),
        );

        assert!(error.contains("403"));
        assert!(error.contains("网关策略"));
        assert!(!error.contains(api_key));
        assert!(error.contains("[redacted-api-key]"));
    }

    #[test]
    fn http_401_points_to_api_key() {
        let error = readable_http_error(StatusCode::UNAUTHORIZED, "");

        assert!(error.contains("401"));
        assert!(error.contains("API Key"));
    }

    #[test]
    fn long_error_body_is_trimmed() {
        let error = readable_http_error(StatusCode::BAD_GATEWAY, &"x".repeat(800));

        assert!(error.contains("502"));
        assert!(error.len() < 560);
    }

    #[test]
    fn endpoint_joins_base_url_and_path() {
        let settings = AiSettings {
            base_url: "https://example.test/v1/".into(),
            api_key: "sk-test".into(),
            model: "demo".into(),
        };

        assert_eq!(
            endpoint(&settings, "chat/completions"),
            "https://example.test/v1/chat/completions"
        );
    }
}
