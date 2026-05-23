use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn pick_free_port() -> Option<u16> {
    match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => Some(listener.local_addr().expect("local addr").port()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => None,
        Err(e) => panic!("bind ephemeral port failed: {e}"),
    }
}

fn spawn_mock(port: u16, scenario: Option<&str>) -> Child {
    let bin = env!("CARGO_BIN_EXE_mock_provider");
    let mut cmd = Command::new(bin);
    cmd.env("KODE_MOCK_BIND", format!("127.0.0.1:{port}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(s) = scenario {
        cmd.env("KODE_MOCK_SCENARIO", s);
    }
    cmd.spawn().expect("spawn mock_provider")
}

fn wait_for_health(port: u16, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Ok((status, body)) = http_get(port, "/health") {
            if status == 200 && body.contains("\"ok\":true") {
                return;
            }
        }
        thread::sleep(Duration::from_millis(40));
    }
    panic!("mock provider did not become healthy in time");
}

fn http_get(port: u16, path: &str) -> std::io::Result<(u16, String)> {
    http_request(port, "GET", path, None)
}

fn http_post(port: u16, path: &str, json_body: &str) -> std::io::Result<(u16, String)> {
    http_request(port, "POST", path, Some(json_body))
}

fn http_request(port: u16, method: &str, path: &str, body: Option<&str>) -> std::io::Result<(u16, String)> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let payload = body.unwrap_or("");
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        payload.len(),
        payload
    );
    stream.write_all(req.as_bytes())?;
    stream.flush()?;

    let mut raw = String::new();
    stream.read_to_string(&mut raw)?;
    let (head, body) = raw
        .split_once("\r\n\r\n")
        .unwrap_or((raw.as_str(), ""));
    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    Ok((status, body.to_string()))
}

fn write_mock_config(config_root: &PathBuf, port: u16) {
    let kode_dir = config_root.join("kode");
    fs::create_dir_all(&kode_dir).expect("create config dir");
    let cfg = format!(
        r#"model = "mock_openai/gpt-4o-mini"

[providers.mock_openai]
base_url = "http://127.0.0.1:{port}/v1"
api_key = "mock-key"
api_style = "openai"
models = ["gpt-4o-mini", "gpt-4o"]

[providers.mock_anthropic]
base_url = "http://127.0.0.1:{port}"
api_key = "mock-key"
api_style = "anthropic"
anthropic_version = "2023-06-01"
models = ["claude-sonnet-4-5"]

[agent]
max_steps = 8
temperature = 0.1

[context]
max_tokens = 32768
strategy = "sliding"

[cost]
show = true
"#
    );
    fs::write(kode_dir.join("config.toml"), cfg).expect("write config");
}

#[test]
fn mock_provider_serves_models_and_openai_stream() {
    let Some(port) = pick_free_port() else {
        eprintln!("skipping: local socket bind is not permitted in this environment");
        return;
    };
    let mut mock = spawn_mock(port, None);
    wait_for_health(port, Duration::from_secs(2));

    let (status, models) = http_get(port, "/v1/models").expect("GET /v1/models");
    assert_eq!(status, 200);
    assert!(models.contains("gpt-4o-mini"));

    let (status, stream) = http_post(
        port,
        "/v1/chat/completions",
        r#"{"model":"gpt-4o-mini","stream":true,"messages":[{"role":"user","content":"hi"}]}"#,
    )
    .expect("POST /v1/chat/completions");
    assert_eq!(status, 200);
    assert!(stream.contains("data:"));
    assert!(stream.contains("[DONE]"));

    let _ = mock.kill();
    let _ = mock.wait();
}

#[test]
fn kode_returns_non_zero_and_pretty_json_on_auth_error() {
    let Some(port) = pick_free_port() else {
        eprintln!("skipping: local socket bind is not permitted in this environment");
        return;
    };
    let mut mock = spawn_mock(port, Some("auth_error"));
    wait_for_health(port, Duration::from_secs(2));

    let mut cfg_root = std::env::temp_dir();
    cfg_root.push(format!("kode-mock-e2e-{}-{}", std::process::id(), port));
    write_mock_config(&cfg_root, port);

    let kode_bin = env!("CARGO_BIN_EXE_kode");
    let output = Command::new(kode_bin)
        .env("XDG_CONFIG_HOME", &cfg_root)
        .arg("--model")
        .arg("mock_openai/gpt-4o-mini")
        .arg("--prompt")
        .arg("should fail")
        .output()
        .expect("run kode");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("\"invalid_api_key\""), "{stderr}");
    assert!(stderr.contains("\n  \"error\":"), "{stderr}");

    let _ = mock.kill();
    let _ = mock.wait();
    let _ = fs::remove_dir_all(cfg_root);
}
