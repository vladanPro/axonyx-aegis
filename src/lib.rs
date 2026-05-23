use std::fmt;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help,
    Doctor,
    Smoke(SmokeArgs),
    Browser,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmokeArgs {
    pub url: String,
    pub expect_status: u16,
    pub expect_text: Option<String>,
}

impl Default for SmokeArgs {
    fn default() -> Self {
        Self {
            url: "http://127.0.0.1:3000/".to_string(),
            expect_status: 200,
            expect_text: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmokeReport {
    pub url: String,
    pub status: u16,
    pub body_bytes: usize,
    pub matched_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AegisError {
    message: String,
}

impl AegisError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for AegisError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AegisError {}

pub type Result<T> = std::result::Result<T, AegisError>;

pub fn run_from_env() -> Result<()> {
    run(std::env::args().skip(1))
}

pub fn run(args: impl IntoIterator<Item = String>) -> Result<()> {
    match parse_command(args)? {
        Command::Help => {
            print_help();
            Ok(())
        }
        Command::Doctor => {
            print_doctor();
            Ok(())
        }
        Command::Smoke(args) => {
            let report = run_smoke(&args)?;
            println!("Aegis smoke passed");
            println!("  url: {}", report.url);
            println!("  status: {}", report.status);
            println!("  body: {} bytes", report.body_bytes);
            if let Some(text) = report.matched_text {
                println!("  matched: {text}");
            }
            Ok(())
        }
        Command::Browser => {
            println!("Aegis browser engine is reserved for the next phase.");
            println!("Planned shape:");
            println!("  aegis browser --url http://127.0.0.1:3000 --check components");
            println!("  cargo ax test browser");
            println!("Status: preview placeholder only; no browser was launched.");
            Ok(())
        }
    }
}

pub fn parse_command(args: impl IntoIterator<Item = String>) -> Result<Command> {
    let mut args = args.into_iter().collect::<Vec<_>>();

    if args.is_empty() || matches!(args[0].as_str(), "-h" | "--help" | "help") {
        return Ok(Command::Help);
    }

    let command = args.remove(0);
    match command.as_str() {
        "doctor" => Ok(Command::Doctor),
        "smoke" => parse_smoke_args(args).map(Command::Smoke),
        "browser" => Ok(Command::Browser),
        other => Err(AegisError::new(format!(
            "unknown command '{other}'. Run `aegis --help`."
        ))),
    }
}

fn parse_smoke_args(args: Vec<String>) -> Result<SmokeArgs> {
    let mut smoke = SmokeArgs::default();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--url" => {
                index += 1;
                smoke.url = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| AegisError::new("--url requires a value"))?;
            }
            "--status" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| AegisError::new("--status requires a value"))?;
                smoke.expect_status = value
                    .parse::<u16>()
                    .map_err(|_| AegisError::new("--status must be a valid HTTP status code"))?;
            }
            "--expect" => {
                index += 1;
                smoke.expect_text = Some(
                    args.get(index)
                        .cloned()
                        .ok_or_else(|| AegisError::new("--expect requires a value"))?,
                );
            }
            "-h" | "--help" => {
                print_smoke_help();
                return Ok(smoke);
            }
            other => {
                return Err(AegisError::new(format!(
                    "unknown smoke option '{other}'. Run `aegis smoke --help`."
                )));
            }
        }

        index += 1;
    }

    Ok(smoke)
}

pub fn run_smoke(args: &SmokeArgs) -> Result<SmokeReport> {
    let request = parse_http_url(&args.url)?;
    let mut stream = TcpStream::connect((request.host.as_str(), request.port))
        .map_err(|error| AegisError::new(format!("failed to connect to {}: {error}", args.url)))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|error| AegisError::new(format!("failed to set read timeout: {error}")))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .map_err(|error| AegisError::new(format!("failed to set write timeout: {error}")))?;

    let request_text = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: axonyx-aegis/{}\r\nAccept: text/html,*/*\r\nConnection: close\r\n\r\n",
        request.path, request.authority, VERSION
    );
    stream
        .write_all(request_text.as_bytes())
        .map_err(|error| AegisError::new(format!("failed to write HTTP request: {error}")))?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| AegisError::new(format!("failed to read HTTP response: {error}")))?;

    let (status, body) = parse_http_response(&response)?;
    if status != args.expect_status {
        return Err(AegisError::new(format!(
            "expected HTTP {}, got {} for {}",
            args.expect_status, status, args.url
        )));
    }

    if let Some(expected) = args.expect_text.as_deref() {
        if !body.contains(expected) {
            return Err(AegisError::new(format!(
                "expected response body to contain '{expected}'"
            )));
        }
    }

    Ok(SmokeReport {
        url: args.url.clone(),
        status,
        body_bytes: body.len(),
        matched_text: args.expect_text.clone(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HttpRequestTarget {
    host: String,
    port: u16,
    authority: String,
    path: String,
}

fn parse_http_url(url: &str) -> Result<HttpRequestTarget> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| AegisError::new("Aegis smoke currently supports http:// URLs only"))?;
    let (authority, path) = match rest.split_once('/') {
        Some((authority, path)) => (authority, format!("/{path}")),
        None => (rest, "/".to_string()),
    };

    if authority.is_empty() {
        return Err(AegisError::new("URL host is missing"));
    }

    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() => {
            let port = port
                .parse::<u16>()
                .map_err(|_| AegisError::new("URL port must be a valid u16"))?;
            (host.to_string(), port)
        }
        _ => (authority.to_string(), 80),
    };

    Ok(HttpRequestTarget {
        host,
        port,
        authority: authority.to_string(),
        path,
    })
}

fn parse_http_response(response: &str) -> Result<(u16, &str)> {
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| AegisError::new("HTTP response did not include a header/body split"))?;
    let status_line = headers
        .lines()
        .next()
        .ok_or_else(|| AegisError::new("HTTP response status line is missing"))?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| AegisError::new("HTTP response status code is missing"))?
        .parse::<u16>()
        .map_err(|_| AegisError::new("HTTP response status code is invalid"))?;

    Ok((status, body))
}

fn print_help() {
    println!("Aegis {VERSION}");
    println!("Rust-first E2E and QA runner for Axonyx applications.");
    println!();
    println!("Usage:");
    println!("  aegis doctor");
    println!("  aegis smoke --url http://127.0.0.1:3000 --expect Axonyx");
    println!("  aegis browser");
    println!();
    println!("Commands:");
    println!("  doctor   Print local runner readiness.");
    println!("  smoke    Run a fast HTTP smoke check against a local site.");
    println!("  browser  Reserved placeholder for the future browser engine.");
}

fn print_smoke_help() {
    println!("Usage:");
    println!("  aegis smoke --url http://127.0.0.1:3000 --status 200 --expect Axonyx");
}

fn print_doctor() {
    println!("Aegis doctor");
    println!("  runner: ok");
    println!("  http smoke: ok");
    println!("  browser engine: reserved");
    println!("  axonyx bridge: planned");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_help_command() {
        assert_eq!(parse_command(Vec::<String>::new()).unwrap(), Command::Help);
    }

    #[test]
    fn parses_smoke_args() {
        let command = parse_command([
            "smoke".to_string(),
            "--url".to_string(),
            "http://127.0.0.1:4173/docs".to_string(),
            "--status".to_string(),
            "200".to_string(),
            "--expect".to_string(),
            "Axonyx".to_string(),
        ])
        .unwrap();

        assert_eq!(
            command,
            Command::Smoke(SmokeArgs {
                url: "http://127.0.0.1:4173/docs".to_string(),
                expect_status: 200,
                expect_text: Some("Axonyx".to_string()),
            })
        );
    }

    #[test]
    fn parses_http_url_with_port_and_path() {
        let target = parse_http_url("http://127.0.0.1:3000/components/button").unwrap();

        assert_eq!(target.host, "127.0.0.1");
        assert_eq!(target.port, 3000);
        assert_eq!(target.authority, "127.0.0.1:3000");
        assert_eq!(target.path, "/components/button");
    }

    #[test]
    fn rejects_https_for_initial_smoke_runner() {
        let error = parse_http_url("https://axonyx.dev").unwrap_err();

        assert!(error.to_string().contains("http://"));
    }

    #[test]
    fn parses_http_status_and_body() {
        let response = "HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\nAxonyx";
        let (status, body) = parse_http_response(response).unwrap();

        assert_eq!(status, 200);
        assert_eq!(body, "Axonyx");
    }
}
