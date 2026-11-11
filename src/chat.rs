use crate::cli::response_render::{RenderConfig, ResponseRenderer};
use crate::config::{Config, ModelConfig, PromptConfig, ProviderConfig, resolve_auth, resolve_header};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};
use serde_json::{Value, json};
use std::{error::Error, time::Duration};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub async fn completion(
    input: &str,
    provider: &ProviderConfig,
    model: &ModelConfig,
    prompt_name: &str,
    prompt: &PromptConfig,
    config: &Config,
) -> Result<()> {
    // Prepare all fallible config before starting the renderer.
    let headers = create_headers(provider, model, config).await?;
    let messages = crate::prompt::render(prompt, input).await?;
    let body = create_request(messages, provider, model, config);
    let disable_stream = !body.get("stream").and_then(Value::as_bool).unwrap_or(false);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(provider.timeout_ms))
        .build()?;
    let mut url = reqwest::Url::parse(&provider.base_url)?;
    url.set_path(&format!("{}/chat/completions", url.path().trim_end_matches('/')));
    url.query_pairs_mut().extend_pairs(&provider.query_params);
    let mut attempt = 0;
    let response = loop {
        let response = client
            .post(url.clone())
            .headers(headers.clone())
            .json(&body)
            .send()
            .await;
        let retry = match &response {
            Ok(response) => response.status() == 429 || response.status().is_server_error(),
            Err(error) => error.is_connect() || error.is_timeout(),
        };
        if retry && attempt < provider.max_retries {
            tokio::time::sleep(Duration::from_millis(250 * (1u64 << attempt.min(5)))).await;
            attempt += 1;
            continue;
        }
        // Do not expose server error bodies or URLs: they may contain credentials.
        let response = response.map_err(|_| "chat request failed (connection or timeout)")?;
        if !response.status().is_success() {
            return Err(format!("chat request failed: HTTP {}", response.status()).into());
        }
        break response;
    };
    let renderer = ResponseRenderer::new();
    let (tx, task) = renderer.start_render(RenderConfig {
        pure: config.pure,
        model_name: model.get_name(),
        prompt_config_name: prompt_name.into(),
        type_speed: 30,
        disable_stream,
    });
    let result: Result<()> = async {
        if disable_stream {
            let response: Value = response.json().await.map_err(|_| "invalid chat response")?;
            check_response(&response)?;
            if let Some(content) = response["choices"][0]["message"]["content"].as_str() {
                tx.send(content.to_owned()).await?;
            }
        } else {
            let mut events = response.bytes_stream().eventsource();
            let mut done = false;
            while let Some(event) = events.next().await {
                let event = event.map_err(|_| "chat stream failed")?;
                if event.data == "[DONE]" {
                    done = true;
                    break;
                }
                let response: Value = serde_json::from_str(&event.data).map_err(|_| "invalid chat stream JSON")?;
                check_response(&response)?;
                if let Some(choices) = response["choices"].as_array() {
                    for choice in choices {
                        if let Some(content) = choice["delta"]["content"].as_str() {
                            tx.send(content.to_owned()).await?;
                        }
                    }
                }
            }
            if !done {
                return Err("chat stream ended before [DONE]".into());
            }
        }
        Ok(())
    }
    .await;
    drop(tx);
    task.await?;
    result?;
    if !config.pure {
        renderer.render_tail_bar();
    }
    Ok(())
}

fn check_response(response: &Value) -> Result<()> {
    if response.get("error").is_some() {
        return Err("API returned an error".into());
    }
    Ok(())
}

async fn create_headers(provider: &ProviderConfig, model: &ModelConfig, config: &Config) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    if let Some(api_key) = &provider.api_key {
        let key = resolve_auth(api_key, &config.auth)?;
        let mut value = HeaderValue::from_str(&format!("Bearer {key}")).map_err(|_| "invalid API credential header")?;
        value.set_sensitive(true);
        headers.insert(AUTHORIZATION, value);
    }
    // Header names are case insensitive: normalize before choosing overrides.
    let mut custom = std::collections::HashMap::new();
    for source in [&provider.headers, &model.headers] {
        for (name, value) in source {
            let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| "invalid configured header name")?;
            custom.insert(name, value);
        }
    }
    for (name, value) in custom {
        let value = resolve_header(value).await?;
        let mut value = HeaderValue::from_str(&value).map_err(|_| "invalid configured header value")?;
        value.set_sensitive(true);
        headers.insert(name, value);
    }
    Ok(headers)
}

fn create_request(messages: Value, provider: &ProviderConfig, model: &ModelConfig, config: &Config) -> Value {
    let mut body = json!({ "model": model.id, "stream": !config.disable_stream,
        "messages": messages });
    if model.reasoning {
        body["reasoning_effort"] = json!(model.thinking_level.as_ref().unwrap_or(&config.default_thinking_level));
    }
    let fields = body.as_object_mut().unwrap();
    fields.extend(provider.extra_body.clone());
    fields.extend(model.extra_body.clone());
    for field in &model.remove_body_fields {
        fields.remove(field);
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Config {
        serde_json::from_value(json!({
            "defaultThinkingLevel": "custom-effort",
            "providers": {"gateway": {
                "baseUrl": "http://localhost/v1", "apiKey": "example",
                "headers": {"X-Route":"provider", "X-Shared":"shared"},
                "extraBody": {"temperature":0.7,"store":true,"nested":{"a":1}},
                "models": [{"id":"vendor/model", "headers":{"x-route":"model"},
                    "extraBody":{"temperature":0.2,"nested":{"b":2}},"removeBodyFields":["store"]}]
            }}
        }))
        .unwrap()
    }
    #[tokio::test]
    async fn request_merge_and_reasoning() {
        let mut cfg = config();
        let prompt = PromptConfig {
            content: "system".into(),
            ..Default::default()
        };
        let provider = &cfg.providers["gateway"];
        let model = &provider.models[0];
        let body = create_request(crate::prompt::render(&prompt, "hello").await.unwrap(), provider, model, &cfg);
        assert_eq!(body["model"], "vendor/model");
        assert_eq!(body["reasoning_effort"], "custom-effort");
        assert_eq!(body["temperature"], 0.2);
        assert_eq!(body["nested"], json!({"b":2}));
        assert!(body.get("store").is_none());
        cfg.providers.get_mut("gateway").unwrap().models[0].reasoning = false;
        let provider = &cfg.providers["gateway"];
        assert!(
            create_request(crate::prompt::render(&prompt, "hello").await.unwrap(), provider, &provider.models[0], &cfg)
                .get("reasoning_effort")
                .is_none()
        );
    }
    #[tokio::test]
    async fn header_overrides_are_case_insensitive() {
        let cfg = config();
        let provider = &cfg.providers["gateway"];
        let headers = create_headers(provider, &provider.models[0], &cfg).await.unwrap();
        assert_eq!(headers["authorization"], "Bearer example");
        assert_eq!(headers["x-route"], "model");
        assert_eq!(headers["x-shared"], "shared");
    }
    #[tokio::test]
    async fn mock_api_streaming_nonstreaming_and_retries() {
        use tokio::{
            io::{AsyncReadExt, AsyncWriteExt},
            net::TcpListener,
        };
        for streaming in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                for attempt in 0..2 {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    let mut request = Vec::new();
                    let header_end = loop {
                        let mut buf = [0u8; 2048];
                        let n = socket.read(&mut buf).await.unwrap();
                        assert!(n > 0);
                        request.extend_from_slice(&buf[..n]);
                        if let Some(pos) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                            break pos + 4;
                        }
                    };
                    let headers = String::from_utf8_lossy(&request[..header_end]).to_lowercase();
                    let length: usize = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length: "))
                        .unwrap()
                        .trim()
                        .parse()
                        .unwrap();
                    while request.len() < header_end + length {
                        let mut buf = [0u8; 2048];
                        let n = socket.read(&mut buf).await.unwrap();
                        assert!(n > 0);
                        request.extend_from_slice(&buf[..n]);
                    }
                    assert!(headers.starts_with("post /v1/chat/completions?region=sg "));
                    assert!(headers.contains("authorization: bearer example"));
                    assert!(headers.contains("x-route: model"));
                    let body: Value = serde_json::from_slice(&request[header_end..header_end + length]).unwrap();
                    assert_eq!(body["model"], "vendor/model");
                    assert_eq!(body["stream"], streaming);
                    assert_eq!(body["messages"][0]["content"], "system");
                    assert_eq!(body["messages"][1]["content"], "Question: hello");
                    let (status, content_type, payload) = if attempt == 0 {
                        ("503 Service Unavailable", "application/json", "{}")
                    } else if streaming {
                        (
                            "200 OK",
                            "text/event-stream",
                            "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\r\n\r\ndata: [DONE]\r\n\r\n",
                        )
                    } else {
                        (
                            "200 OK",
                            "application/json",
                            "{\"choices\":[{\"message\":{\"content\":\"ok\"}}]}",
                        )
                    };
                    let response = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                        payload.len()
                    );
                    socket.write_all(response.as_bytes()).await.unwrap();
                }
            });
            let mut cfg = config();
            cfg.pure = true;
            cfg.disable_stream = !streaming;
            let provider = cfg.providers.get_mut("gateway").unwrap();
            provider.base_url = format!("http://{address}/v1/");
            provider.query_params.insert("region".into(), "sg".into());
            provider.timeout_ms = 2000;
            let provider = &cfg.providers["gateway"];
            let prompt = PromptConfig {
                system: Some("system".into()),
                user: Some("Question: {{ input }}".into()),
                ..Default::default()
            };
            tokio::time::timeout(
                Duration::from_secs(5),
                completion("hello", provider, &provider.models[0], "test", &prompt, &cfg),
            )
            .await
            .unwrap()
            .unwrap();
            server.await.unwrap();
        }
    }
}
