use crate::cli::response_render::{RenderConfig, RenderEvent, ResponseRenderer};
use crate::config::{Config, ModelConfig, PromptConfig, ProviderConfig, resolve_api_key, resolve_header};
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
    integration_shell: Option<crate::cli::structs::IntegrationShell>,
) -> Result<Value> {
    // Prepare all fallible config before starting the renderer.
    let headers = create_headers(provider, model, config).await?;
    let mut messages = crate::prompt::render(prompt, input).await?;
    if let Some(shell) = integration_shell {
        let system = messages[0]["content"].as_str().unwrap_or_default();
        messages[0]["content"] = json!(format!(
            "{system}\nCurrent shell: {shell:?}. Operating system: {}. Generate commands for this shell and OS.",
            std::env::consts::OS
        ));
    }
    crate::log_trace!("Rendered prompt messages: {messages}");
    let body = create_request(messages, provider, model, config);
    let disable_stream = !body.get("stream").and_then(Value::as_bool).unwrap_or(false);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(provider.timeout_ms))
        .build()?;
    let url = completion_url(provider)?;
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
        display: config.display.clone(),
    });
    let mut usage = Value::Null;
    let result: Result<()> = async {
        if disable_stream {
            let response: Value = response.json().await.map_err(|_| "invalid chat response")?;
            check_response(&response)?;
            if !response["usage"].is_null() {
                usage = response["usage"].clone();
            }
            let message = &response["choices"][0]["message"];
            if let Some(text) = reasoning_text(message) {
                tx.send(RenderEvent::Reasoning(text.into())).await?;
            }
            if let Some(content) = message["content"].as_str() {
                tx.send(RenderEvent::Content(content.to_owned())).await?;
            }
        } else {
            let mut events = response.bytes_stream().eventsource();
            let mut done = false;
            while let Some(event) = events.next().await {
                // Consume the HTTP body through EOF even after the SSE sentinel.
                // Dropping it at [DONE] can reset an otherwise successful proxy connection.
                if done {
                    if let Err(error) = event {
                        crate::log_debug!("Chat stream transport ended after [DONE]: {error}");
                        break;
                    }
                    continue;
                }
                let event = event.map_err(|_| "chat stream failed")?;
                if event.data == "[DONE]" {
                    done = true;
                    continue;
                }
                let response: Value = serde_json::from_str(&event.data).map_err(|_| "invalid chat stream JSON")?;
                check_response(&response)?;
                if !response["usage"].is_null() {
                    usage = response["usage"].clone();
                }
                if let Some(choices) = response["choices"].as_array() {
                    for choice in choices {
                        if let Some(text) = reasoning_text(&choice["delta"]) {
                            tx.send(RenderEvent::Reasoning(text.into())).await?;
                        }
                        if let Some(content) = choice["delta"]["content"].as_str() {
                            tx.send(RenderEvent::Content(content.to_owned())).await?;
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
    Ok(usage)
}

fn reasoning_text(message: &Value) -> Option<&str> {
    message["reasoning_content"]
        .as_str()
        .or_else(|| message["reasoning"].as_str())
}

fn format_tokens(tokens: u64) -> String {
    let (value, suffix) = if tokens >= 1_000_000 {
        (tokens as f64 / 1_000_000.0, "m")
    } else if tokens >= 1_000 {
        (tokens as f64 / 1_000.0, "k")
    } else {
        return tokens.to_string();
    };
    let number = format!("{value:.2}");
    format!("{}{suffix}", number.trim_end_matches('0').trim_end_matches('.'))
}

pub(crate) fn format_usage(usage: &Value) -> String {
    let input = usage["prompt_tokens"].as_u64();
    let output = usage["completion_tokens"].as_u64();
    let cached = usage["prompt_tokens_details"]["cached_tokens"]
        .as_u64()
        .or_else(|| usage["prompt_cache_hit_tokens"].as_u64());
    let cache = match cached {
        Some(hit) if hit < 1_000 => hit.to_string(),
        Some(hit) => match input {
            Some(total) if total > 0 => format!("{:.1}%", hit as f64 / total as f64 * 100.0),
            _ => "n/a".into(),
        },
        None => "n/a".into(),
    };
    format!(
        "↑ {} tokens({} cached) ↓ {} tokens",
        input.map(format_tokens).unwrap_or_else(|| "n/a".into()),
        cache,
        output.map(format_tokens).unwrap_or_else(|| "n/a".into())
    )
}

fn check_response(response: &Value) -> Result<()> {
    if response.get("error").is_some() {
        return Err("API returned an error".into());
    }
    Ok(())
}

async fn create_headers(provider: &ProviderConfig, model: &ModelConfig, _config: &Config) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    if let Some(api_key) = &provider.api_key {
        let key = resolve_api_key(api_key)?;
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

fn completion_url(provider: &ProviderConfig) -> Result<reqwest::Url> {
    let mut url = reqwest::Url::parse(&provider.base_url)?;
    url.set_path(&format!("{}/chat/completions", url.path().trim_end_matches('/')));
    if !provider.query_params.is_empty() {
        url.query_pairs_mut().extend_pairs(&provider.query_params);
    }
    if url.query() == Some("") {
        url.set_query(None);
    }
    Ok(url)
}

fn create_request(messages: Value, provider: &ProviderConfig, model: &ModelConfig, config: &Config) -> Value {
    let mut body = json!({ "model": model.id, "stream": model.stream,
        "messages": messages });
    if model.reasoning {
        body["reasoning_effort"] = json!(model.thinking_level.as_ref().unwrap_or(&config.default_thinking_level));
    } else {
        let host = reqwest::Url::parse(&provider.base_url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned));
        if host.as_deref() == Some("openrouter.ai") {
            body["reasoning"] = json!({"enabled": false});
        } else if host.as_deref() == Some("api.deepseek.com")
            || model.id.rsplit('/').next().is_some_and(|id| id.starts_with("deepseek"))
        {
            body["thinking"] = json!({"type": "disabled"});
        } else {
            body["reasoning_effort"] = json!("none");
        }
    }
    let fields = body.as_object_mut().unwrap();
    fields.extend(provider.extra_body.clone());
    fields.extend(model.extra_body.clone());
    if config.display.usage && fields.get("stream").and_then(Value::as_bool) == Some(true) {
        fields
            .entry("stream_options")
            .or_insert_with(|| json!({"include_usage": true}));
    }
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
                "baseUrl": "http://localhost/v1", "apiKey": {"value":"example"},
                "headers": {"X-Route":"provider", "X-Shared":"shared"},
                "extraBody": {"temperature":0.7,"store":true,"nested":{"a":1}},
                "models": [{"id":"vendor/model", "headers":{"x-route":"model"},
                    "extraBody":{"temperature":0.2,"nested":{"b":2}},"removeBodyFields":["store"]}]
            }}
        }))
        .unwrap()
    }
    #[tokio::test]
    async fn streaming_drains_http_body_after_done() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                let n = socket.read(&mut buffer).await.unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buffer[..n]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                    let length: usize = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length:").map(str::trim))
                        .unwrap()
                        .parse()
                        .unwrap();
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n")
                .await
                .unwrap();
            let payload = "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n\ndata: [DONE]\n\n";
            socket
                .write_all(format!("{:x}\r\n{payload}\r\n", payload.len()).as_bytes())
                .await
                .unwrap();
            // The proxy sends the final HTTP chunk after the SSE sentinel.
            tokio::time::sleep(Duration::from_millis(150)).await;
            let payload = "data: ignored trailer\n\n";
            let sent = std::time::Instant::now();
            socket
                .write_all(format!("{:x}\r\n{payload}\r\n0\r\n\r\n", payload.len()).as_bytes())
                .await
                .unwrap();
            sent
        });
        let mut cfg = config();
        cfg.pure = true;
        let provider = cfg.providers.get_mut("gateway").unwrap();
        provider.base_url = format!("http://{address}/v1");
        provider.api_key = None;
        provider.max_retries = 0;
        provider.timeout_ms = 2000;
        provider.models[0].stream = true;
        let provider = &cfg.providers["gateway"];
        completion(
            "hello",
            provider,
            &provider.models[0],
            "test",
            &PromptConfig::default(),
            &cfg,
            None,
        )
        .await
        .unwrap();
        let finished = std::time::Instant::now();
        assert!(
            finished >= server.await.unwrap(),
            "completion returned before the final HTTP chunk"
        );
    }

    #[test]
    fn compact_usage_units_and_cache_threshold() {
        for (tokens, expected) in [
            (0, "0"),
            (999, "999"),
            (1000, "1k"),
            (1250, "1.25k"),
            (999999, "1000k"),
            (1000000, "1m"),
            (1250000, "1.25m"),
        ] {
            assert_eq!(format_tokens(tokens), expected);
        }
        assert_eq!(
            format_usage(&json!({"prompt_tokens":350,"completion_tokens":356,
            "prompt_cache_hit_tokens":200})),
            "↑ 350 tokens(200 cached) ↓ 356 tokens"
        );
        assert_eq!(
            format_usage(&json!({"prompt_tokens":2000,"completion_tokens":1250000,
            "prompt_cache_hit_tokens":999})),
            "↑ 2k tokens(999 cached) ↓ 1.25m tokens"
        );
        assert_eq!(
            format_usage(&json!({"prompt_tokens":2000,"completion_tokens":1000,
            "prompt_tokens_details":{"cached_tokens":1000}})),
            "↑ 2k tokens(50.0% cached) ↓ 1k tokens"
        );
        assert_eq!(format_usage(&Value::Null), "↑ n/a tokens(n/a cached) ↓ n/a tokens");
    }

    #[test]
    fn usage_and_reasoning_fields() {
        assert_eq!(
            format_usage(&json!({"prompt_tokens":100,"completion_tokens":20,
            "prompt_tokens_details":{"cached_tokens":25}})),
            "↑ 100 tokens(25 cached) ↓ 20 tokens"
        );
        assert!(format_usage(&Value::Null).contains("n/a cached"));
        assert_eq!(
            reasoning_text(&json!({"reasoning_content":"thinking"})),
            Some("thinking")
        );
        assert_eq!(reasoning_text(&json!({"reasoning":"thinking"})), Some("thinking"));
        let mut cfg = config();
        cfg.display.usage = true;
        let provider = &cfg.providers["gateway"];
        assert_eq!(
            create_request(json!([]), provider, &provider.models[0], &cfg)["stream_options"]["include_usage"],
            true
        );
    }
    #[test]
    fn model_stream_controls_request_and_usage_options() {
        let mut cfg = config();
        cfg.display.usage = true;
        let mut provider = cfg.providers["gateway"].clone();
        let mut model = provider.models[0].clone();
        for stream in [true, false] {
            model.stream = stream;
            let body = create_request(json!([]), &provider, &model, &cfg);
            assert_eq!(body["stream"], stream);
            assert_eq!(body.get("stream_options").is_some(), stream);
        }
        provider.extra_body.insert("stream".into(), json!(true));
        let body = create_request(json!([]), &provider, &model, &cfg);
        assert_eq!(body["stream_options"]["include_usage"], true);
        model.extra_body.insert("stream".into(), json!(false));
        let body = create_request(json!([]), &provider, &model, &cfg);
        assert_eq!(body["stream"], false);
        assert!(body.get("stream_options").is_none());
    }

    #[test]
    fn completion_urls_do_not_add_empty_queries() {
        let cfg = config();
        let mut provider = cfg.providers["gateway"].clone();
        for base in ["http://localhost/v1", "http://localhost/v1/", "http://localhost/v1?"] {
            provider.base_url = base.into();
            assert_eq!(
                completion_url(&provider).unwrap().as_str(),
                "http://localhost/v1/chat/completions"
            );
        }
        provider.base_url = "http://localhost/v1?existing=yes".into();
        assert_eq!(completion_url(&provider).unwrap().query(), Some("existing=yes"));
        provider.query_params.insert("region".into(), "sg".into());
        assert_eq!(
            completion_url(&provider).unwrap().query(),
            Some("existing=yes&region=sg")
        );
    }

    #[test]
    fn reasoning_false_sends_explicit_api_switches() {
        let cfg = config();
        let mut provider = cfg.providers["gateway"].clone();
        let mut model = provider.models[0].clone();
        model.reasoning = false;
        model.thinking_level = Some("high".into());
        assert_eq!(
            create_request(json!([]), &provider, &model, &cfg)["reasoning_effort"],
            "none"
        );
        model.id = "deepseek-flash".into();
        let body = create_request(json!([]), &provider, &model, &cfg);
        assert_eq!(body["thinking"], json!({"type":"disabled"}));
        assert!(body.get("reasoning_effort").is_none());
        provider.base_url = "https://openrouter.ai/api/v1".into();
        assert_eq!(
            create_request(json!([]), &provider, &model, &cfg)["reasoning"],
            json!({"enabled":false})
        );
        provider.base_url = "https://api.deepseek.com/v1".into();
        model.id = "custom-alias".into();
        assert_eq!(
            create_request(json!([]), &provider, &model, &cfg)["thinking"]["type"],
            "disabled"
        );
        model.extra_body.insert("thinking".into(), json!({"type":"enabled"}));
        assert_eq!(
            create_request(json!([]), &provider, &model, &cfg)["thinking"]["type"],
            "enabled"
        );
        model.remove_body_fields.push("thinking".into());
        assert!(
            create_request(json!([]), &provider, &model, &cfg)
                .get("thinking")
                .is_none()
        );
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
        let body = create_request(
            crate::prompt::render(&prompt, "hello").await.unwrap(),
            provider,
            model,
            &cfg,
        );
        assert_eq!(body["model"], "vendor/model");
        assert_eq!(body["reasoning_effort"], "custom-effort");
        assert_eq!(body["temperature"], 0.2);
        assert_eq!(body["nested"], json!({"b":2}));
        assert!(body.get("store").is_none());
        cfg.providers.get_mut("gateway").unwrap().models[0].reasoning = false;
        let provider = &cfg.providers["gateway"];
        assert_eq!(
            create_request(json!([]), provider, &provider.models[0], &cfg)["reasoning_effort"],
            "none"
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
                    let system = body["messages"][0]["content"].as_str().unwrap();
                    assert!(system.starts_with("system\nCurrent shell: Fish."));
                    assert!(system.contains(std::env::consts::OS));
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
            cfg.providers.get_mut("gateway").unwrap().models[0].stream = streaming;
            cfg.providers.get_mut("gateway").unwrap().api_key =
                Some(crate::config::encrypt_api_key("example").unwrap());
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
                completion(
                    "hello",
                    provider,
                    &provider.models[0],
                    "test",
                    &prompt,
                    &cfg,
                    Some(crate::cli::structs::IntegrationShell::Fish),
                ),
            )
            .await
            .unwrap()
            .unwrap();
            server.await.unwrap();
        }
    }
}
