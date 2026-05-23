use serde::Deserialize;
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help,
    Doctor,
    Smoke(SmokeArgs),
    Fast(FastArgs),
    Browser,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmokeArgs {
    pub url: String,
    pub expect_status: u16,
    pub expect_text: Option<String>,
    pub expect_all: Vec<String>,
    pub expect_not: Vec<String>,
}

impl Default for SmokeArgs {
    fn default() -> Self {
        Self {
            url: "http://127.0.0.1:3000/".to_string(),
            expect_status: 200,
            expect_text: None,
            expect_all: Vec::new(),
            expect_not: Vec::new(),
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
pub struct FastArgs {
    pub config: PathBuf,
}

impl Default for FastArgs {
    fn default() -> Self {
        Self {
            config: PathBuf::from("aegis.toml"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct AegisConfig {
    pub base_url: Option<String>,
    #[serde(default)]
    pub smoke: Vec<SmokeCheckConfig>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct SmokeCheckConfig {
    pub name: Option<String>,
    pub path: Option<String>,
    pub url: Option<String>,
    pub expect: Option<String>,
    #[serde(default)]
    pub expect_all: Vec<String>,
    #[serde(default)]
    pub expect_not: Vec<String>,
    #[serde(default = "default_status")]
    pub status: u16,
}

fn default_status() -> u16 {
    200
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

pub fn fast(test_name: &str, test: impl FnOnce(&mut FastPage)) {
    let mut page = FastPage::new(test_name);
    test(&mut page);
}

pub fn browser(test_name: &str, _test: impl FnOnce(&mut BrowserPage)) {
    panic!(
        "Aegis browser test '{test_name}' needs the future headless browser engine. Use aegis::fast for response tests today."
    );
}

pub struct FastPage {
    test_name: String,
    client: reqwest::blocking::Client,
    current_url: Option<String>,
    current_body: String,
    current_status: Option<u16>,
}

impl FastPage {
    pub fn new(test_name: impl Into<String>) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent(format!("axonyx-aegis/{VERSION}"))
            .build()
            .unwrap_or_else(|error| panic!("failed to create Aegis HTTP client: {error}"));

        Self {
            test_name: test_name.into(),
            client,
            current_url: None,
            current_body: String::new(),
            current_status: None,
        }
    }

    pub fn goto(&mut self, url: &str) -> &mut Self {
        self.try_goto(url)
            .unwrap_or_else(|error| panic!("Aegis fast '{}' failed: {error}", self.test_name));
        self
    }

    pub fn click(&mut self, selector: &str) -> &mut Self {
        self.try_click(selector)
            .unwrap_or_else(|error| panic!("Aegis fast '{}' failed: {error}", self.test_name));
        self
    }

    pub fn expect_text(&mut self, expected: &str) -> &mut Self {
        self.try_expect_text(expected)
            .unwrap_or_else(|error| panic!("Aegis fast '{}' failed: {error}", self.test_name));
        self
    }

    pub fn expect_status(&mut self, expected: u16) -> &mut Self {
        self.try_expect_status(expected)
            .unwrap_or_else(|error| panic!("Aegis fast '{}' failed: {error}", self.test_name));
        self
    }

    pub fn expect_all(&mut self, expected: &[&str]) -> &mut Self {
        for text in expected {
            self.expect_text(text);
        }
        self
    }

    pub fn expect_not(&mut self, unexpected: &str) -> &mut Self {
        self.try_expect_not(unexpected)
            .unwrap_or_else(|error| panic!("Aegis fast '{}' failed: {error}", self.test_name));
        self
    }

    pub fn try_goto(&mut self, url: &str) -> Result<()> {
        let response = self
            .client
            .get(url)
            .header(reqwest::header::ACCEPT, "text/html,*/*")
            .send()
            .map_err(|error| AegisError::new(format!("failed to request {url}: {error}")))?;
        let status = response.status().as_u16();
        let body = response
            .text()
            .map_err(|error| AegisError::new(format!("failed to read response body: {error}")))?;

        self.current_url = Some(url.to_string());
        self.current_status = Some(status);
        self.current_body = body;
        Ok(())
    }

    pub fn try_click(&mut self, selector: &str) -> Result<()> {
        let href = href_from_selector(selector).ok_or_else(|| {
            AegisError::new(format!(
                "fast click currently supports selectors shaped like a[href='/path']; got '{selector}'"
            ))
        })?;
        let quoted_href_double = format!("href=\"{href}\"");
        let quoted_href_single = format!("href='{href}'");

        if !self.current_body.contains(&quoted_href_double)
            && !self.current_body.contains(&quoted_href_single)
        {
            return Err(AegisError::new(format!(
                "current page does not contain link href '{href}'"
            )));
        }

        let current_url = self
            .current_url
            .as_deref()
            .ok_or_else(|| AegisError::new("call goto before click"))?;
        let next_url = resolve_href(current_url, &href);
        self.try_goto(&next_url)
    }

    pub fn try_expect_text(&self, expected: &str) -> Result<()> {
        if self.current_body.contains(expected) {
            return Ok(());
        }

        Err(AegisError::new(format!(
            "expected current response body to contain '{expected}'"
        )))
    }

    pub fn try_expect_status(&self, expected: u16) -> Result<()> {
        let actual = self
            .current_status
            .ok_or_else(|| AegisError::new("call goto before expect_status"))?;
        if actual == expected {
            return Ok(());
        }

        Err(AegisError::new(format!(
            "expected HTTP status {expected}, got {actual}"
        )))
    }

    pub fn try_expect_not(&self, unexpected: &str) -> Result<()> {
        if !self.current_body.contains(unexpected) {
            return Ok(());
        }

        Err(AegisError::new(format!(
            "expected current response body not to contain '{unexpected}'"
        )))
    }
}

pub struct BrowserPage;

impl BrowserPage {
    pub fn goto(&mut self, _url: &str) -> &mut Self {
        self
    }

    pub fn click(&mut self, _selector: &str) -> &mut Self {
        self
    }

    pub fn expect_text(&mut self, _expected: &str) -> &mut Self {
        self
    }
}

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
        Command::Fast(args) => run_fast_suite(&args),
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
        "fast" => parse_fast_args(args).map(Command::Fast),
        "test" => parse_fast_args(args).map(Command::Fast),
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
            "--expect-not" => {
                index += 1;
                smoke.expect_not.push(
                    args.get(index)
                        .cloned()
                        .ok_or_else(|| AegisError::new("--expect-not requires a value"))?,
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

fn parse_fast_args(args: Vec<String>) -> Result<FastArgs> {
    let mut fast = FastArgs::default();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--config" | "-c" => {
                index += 1;
                fast.config = PathBuf::from(
                    args.get(index)
                        .cloned()
                        .ok_or_else(|| AegisError::new("--config requires a value"))?,
                );
            }
            "-h" | "--help" => {
                print_fast_help();
                return Ok(fast);
            }
            other => {
                return Err(AegisError::new(format!(
                    "unknown fast option '{other}'. Run `aegis fast --help`."
                )));
            }
        }

        index += 1;
    }

    Ok(fast)
}

pub fn run_smoke(args: &SmokeArgs) -> Result<SmokeReport> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(format!("axonyx-aegis/{VERSION}"))
        .build()
        .map_err(|error| AegisError::new(format!("failed to create HTTP client: {error}")))?;
    let response = client
        .get(&args.url)
        .header(reqwest::header::ACCEPT, "text/html,*/*")
        .send()
        .map_err(|error| AegisError::new(format!("failed to request {}: {error}", args.url)))?;
    let status = response.status().as_u16();
    let body = response
        .text()
        .map_err(|error| AegisError::new(format!("failed to read response body: {error}")))?;

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

    for expected in &args.expect_all {
        if !body.contains(expected) {
            return Err(AegisError::new(format!(
                "expected response body to contain '{expected}'"
            )));
        }
    }

    for unexpected in &args.expect_not {
        if body.contains(unexpected) {
            return Err(AegisError::new(format!(
                "expected response body not to contain '{unexpected}'"
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

pub fn run_fast_suite(args: &FastArgs) -> Result<()> {
    let config_source = fs::read_to_string(&args.config).map_err(|error| {
        AegisError::new(format!(
            "failed to read config '{}': {error}",
            args.config.display()
        ))
    })?;
    let config = parse_config(&config_source)?;

    if config.smoke.is_empty() {
        return Err(AegisError::new(format!(
            "config '{}' does not define any [[smoke]] checks",
            args.config.display()
        )));
    }

    println!("Aegis fast checks started");
    println!("  config: {}", args.config.display());

    for (index, check) in config.smoke.iter().enumerate() {
        let smoke = smoke_args_from_config(&config, check)?;
        let label = check
            .name
            .as_deref()
            .map(str::to_string)
            .unwrap_or_else(|| format!("smoke {}", index + 1));
        let report = run_smoke(&smoke).map_err(|error| {
            AegisError::new(format!("check '{label}' failed for {}: {error}", smoke.url))
        })?;

        println!(
            "  ok {label}: {} HTTP {} ({} bytes)",
            report.url, report.status, report.body_bytes
        );
    }

    println!(
        "Aegis fast checks passed: {} smoke check(s)",
        config.smoke.len()
    );
    Ok(())
}

pub fn parse_config(source: &str) -> Result<AegisConfig> {
    toml::from_str::<AegisConfig>(source)
        .map_err(|error| AegisError::new(format!("failed to parse aegis config: {error}")))
}

fn smoke_args_from_config(config: &AegisConfig, check: &SmokeCheckConfig) -> Result<SmokeArgs> {
    let url = match (check.url.as_deref(), check.path.as_deref()) {
        (Some(url), None) => url.to_string(),
        (None, Some(path)) => join_url(
            config
                .base_url
                .as_deref()
                .ok_or_else(|| AegisError::new("smoke check with path requires base_url"))?,
            path,
        ),
        (Some(_), Some(_)) => {
            return Err(AegisError::new(
                "smoke check must use either url or path, not both",
            ));
        }
        (None, None) => return Err(AegisError::new("smoke check requires url or path")),
    };

    Ok(SmokeArgs {
        url,
        expect_status: check.status,
        expect_text: check.expect.clone(),
        expect_all: check.expect_all.clone(),
        expect_not: check.expect_not.clone(),
    })
}

fn join_url(base_url: &str, path: &str) -> String {
    let base = base_url.trim_end_matches('/');
    let path = path.trim_start_matches('/');

    if path.is_empty() {
        format!("{base}/")
    } else {
        format!("{base}/{path}")
    }
}

fn href_from_selector(selector: &str) -> Option<String> {
    let marker = "a[href=";
    let rest = selector.trim().strip_prefix(marker)?.strip_suffix(']')?;
    let quote = rest.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let value = rest.strip_prefix(quote)?.strip_suffix(quote)?;
    Some(value.to_string())
}

fn resolve_href(current_url: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }

    let Some((scheme, rest)) = current_url.split_once("://") else {
        return href.to_string();
    };
    let authority = rest.split('/').next().unwrap_or(rest);

    if href.starts_with('/') {
        format!("{scheme}://{authority}{href}")
    } else {
        format!("{scheme}://{authority}/{href}")
    }
}

fn print_help() {
    println!("Aegis {VERSION}");
    println!("Rust-first E2E and QA runner for Axonyx applications.");
    println!();
    println!("Usage:");
    println!("  aegis doctor");
    println!("  aegis smoke --url http://127.0.0.1:3000 --expect Axonyx");
    println!("  aegis fast --config aegis.toml");
    println!("  aegis browser");
    println!();
    println!("Commands:");
    println!("  doctor   Print local runner readiness.");
    println!("  smoke    Run a fast HTTP smoke check against a local site.");
    println!("  fast     Run fast HTTP/response checks from aegis.toml.");
    println!("  test     Alias for fast, kept for convenience.");
    println!("  browser  Reserved placeholder for the future browser engine.");
}

fn print_smoke_help() {
    println!("Usage:");
    println!(
        "  aegis smoke --url http://127.0.0.1:3000 --status 200 --expect Axonyx --expect-not Error"
    );
}

fn print_fast_help() {
    println!("Usage:");
    println!("  aegis fast --config aegis.toml");
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
                expect_all: Vec::new(),
                expect_not: Vec::new(),
            })
        );
    }

    #[test]
    fn parses_https_smoke_target() {
        let command = parse_command([
            "smoke".to_string(),
            "--url".to_string(),
            "https://react.axonyx.dev/docs/getting-started".to_string(),
        ])
        .unwrap();

        let Command::Smoke(args) = command else {
            panic!("expected smoke command");
        };

        assert_eq!(args.url, "https://react.axonyx.dev/docs/getting-started");
        assert_eq!(args.expect_status, 200);
    }

    #[test]
    fn parses_default_fast_command() {
        let command = parse_command(["fast".to_string()]).unwrap();

        assert_eq!(
            command,
            Command::Fast(FastArgs {
                config: PathBuf::from("aegis.toml")
            })
        );
    }

    #[test]
    fn parses_test_alias_for_fast_command() {
        let command = parse_command(["test".to_string()]).unwrap();

        assert_eq!(
            command,
            Command::Fast(FastArgs {
                config: PathBuf::from("aegis.toml")
            })
        );
    }

    #[test]
    fn parses_fast_config_option() {
        let command = parse_command([
            "fast".to_string(),
            "--config".to_string(),
            "react.aegis.toml".to_string(),
        ])
        .unwrap();

        assert_eq!(
            command,
            Command::Fast(FastArgs {
                config: PathBuf::from("react.aegis.toml")
            })
        );
    }

    #[test]
    fn parses_config_smoke_checks() {
        let config = parse_config(
            r#"
base_url = "https://react.axonyx.dev"

[[smoke]]
name = "home"
path = "/"
expect = "Axonyx"

[[smoke]]
name = "docs"
path = "/docs/getting-started"
expect = "Getting Started"
status = 200
"#,
        )
        .unwrap();

        assert_eq!(config.base_url.as_deref(), Some("https://react.axonyx.dev"));
        assert_eq!(config.smoke.len(), 2);
        assert_eq!(config.smoke[0].status, 200);
        assert_eq!(config.smoke[1].expect.as_deref(), Some("Getting Started"));
    }

    #[test]
    fn parses_config_positive_and_negative_expectations() {
        let config = parse_config(
            r#"
base_url = "https://react.axonyx.dev"

[[smoke]]
name = "home"
path = "/"
expect_all = ["Axonyx", "React"]
expect_not = ["Internal Server Error"]
"#,
        )
        .unwrap();

        assert_eq!(config.smoke[0].expect_all, ["Axonyx", "React"]);
        assert_eq!(config.smoke[0].expect_not, ["Internal Server Error"]);
    }

    #[test]
    fn builds_smoke_args_from_base_url_and_path() {
        let config = AegisConfig {
            base_url: Some("https://react.axonyx.dev/".to_string()),
            smoke: vec![],
        };
        let check = SmokeCheckConfig {
            name: Some("docs".to_string()),
            path: Some("/docs/getting-started".to_string()),
            url: None,
            expect: Some("Getting Started".to_string()),
            expect_all: vec!["Docs".to_string()],
            expect_not: vec!["Internal Server Error".to_string()],
            status: 200,
        };

        let smoke = smoke_args_from_config(&config, &check).unwrap();

        assert_eq!(smoke.url, "https://react.axonyx.dev/docs/getting-started");
        assert_eq!(smoke.expect_text.as_deref(), Some("Getting Started"));
        assert_eq!(smoke.expect_all, ["Docs"]);
        assert_eq!(smoke.expect_not, ["Internal Server Error"]);
    }

    #[test]
    fn extracts_href_from_fast_click_selector() {
        assert_eq!(
            href_from_selector("a[href='/docs/getting-started']").as_deref(),
            Some("/docs/getting-started")
        );
        assert_eq!(
            href_from_selector("a[href=\"/components\"]").as_deref(),
            Some("/components")
        );
    }

    #[test]
    fn resolves_relative_href_from_current_url() {
        assert_eq!(
            resolve_href("https://react.axonyx.dev/docs", "/components"),
            "https://react.axonyx.dev/components"
        );
        assert_eq!(
            resolve_href("https://react.axonyx.dev/docs", "api/button"),
            "https://react.axonyx.dev/api/button"
        );
    }
}
