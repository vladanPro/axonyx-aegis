use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const INIT_AEGIS_TOML: &str = r#"base_url = "https://example.com"

[[fast]]
name = "home"
goto = "/"
expect_text = "Example"
expect_links = ["/docs"]
check_links = true
expect_not = ["Internal Server Error"]

[[fast]]
name = "docs"
goto = "/"
click = "a[href='/docs']"
expect_text = "Docs"
"#;
const INIT_CARGO_TOML: &str = r#"[package]
name = "aegis-tests"
version = "0.0.0"
edition = "2024"
publish = false

[dev-dependencies]
axonyx-aegis = "0.1.8"
"#;
const INIT_SRC_LIB_RS: &str = "//! Test-only crate for Aegis examples.\n";
const INIT_TESTS_FAST_RS: &str = "#[path = \"fast/navigation.rs\"]\nmod navigation;\n";
const INIT_TESTS_FAST_NAVIGATION_RS: &str = r#"#[test]
fn opens_docs() {
    aegis::fast("opens docs", |page| {
        page.goto("https://example.com");
        page.expect_status(200);
        page.expect_text("Example");
        page.expect_not("Internal Server Error");
    });
}
"#;
const INIT_TESTS_BROWSER_RS: &str = "#[path = \"browser/drawer.rs\"]\nmod drawer;\n";
const INIT_TESTS_BROWSER_DRAWER_RS: &str = r#"#[test]
#[ignore = "Aegis browser engine is reserved for a future release"]
fn opens_drawer() {
    aegis::browser("opens drawer", |page| {
        page.goto("https://example.com/components/drawer");
        page.click("[data-ax-drawer-open]");
        page.expect_text("Drawer");
    });
}
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help,
    Doctor,
    Init(InitArgs),
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
    pub format: OutputFormat,
    pub fail_fast: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitArgs {
    pub force: bool,
}

impl Default for FastArgs {
    fn default() -> Self {
        Self {
            config: PathBuf::from("aegis.toml"),
            format: OutputFormat::Text,
            fail_fast: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FastSuiteReport {
    pub config: String,
    pub passed: bool,
    pub check_count: usize,
    pub checks: Vec<FastCheckReport>,
    pub failures: Vec<FastFailureReport>,
    pub failure: Option<FastFailureReport>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FastCheckReport {
    pub name: String,
    pub url: String,
    pub status: u16,
    pub body_bytes: usize,
    pub matched_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FastFailureReport {
    pub check: String,
    pub error: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct AegisConfig {
    pub base_url: Option<String>,
    #[serde(default)]
    pub fast: Vec<FastCheckConfig>,
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

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct FastCheckConfig {
    pub name: Option<String>,
    pub goto: Option<String>,
    pub url: Option<String>,
    pub click: Option<String>,
    pub expect_text: Option<String>,
    pub expect_link: Option<String>,
    #[serde(default)]
    pub expect_links: Vec<String>,
    #[serde(default)]
    pub check_links: bool,
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
    reported: bool,
}

impl AegisError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            reported: false,
        }
    }

    fn reported(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            reported: true,
        }
    }

    pub fn was_reported(&self) -> bool {
        self.reported
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

    pub fn expect_link(&mut self, href: &str) -> &mut Self {
        self.try_expect_link(href)
            .unwrap_or_else(|error| panic!("Aegis fast '{}' failed: {error}", self.test_name));
        self
    }

    pub fn expect_links(&mut self, hrefs: &[&str]) -> &mut Self {
        for href in hrefs {
            self.expect_link(href);
        }
        self
    }

    pub fn check_links(&mut self) -> &mut Self {
        self.try_check_links()
            .unwrap_or_else(|error| panic!("Aegis fast '{}' failed: {error}", self.test_name));
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
        if !body_has_link_href(&self.current_body, &href) {
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

    pub fn try_expect_link(&self, href: &str) -> Result<()> {
        if body_has_link_href(&self.current_body, href) {
            return Ok(());
        }

        Err(AegisError::new(format!(
            "expected current response body to contain link href '{href}'"
        )))
    }

    pub fn try_check_links(&self) -> Result<()> {
        let current_url = self
            .current_url
            .as_deref()
            .ok_or_else(|| AegisError::new("call goto before check_links"))?;
        let current_origin = origin_from_url(current_url)
            .ok_or_else(|| AegisError::new("check_links requires an absolute current URL"))?;
        let mut checked = Vec::new();

        for href in extract_link_hrefs(&self.current_body) {
            if should_skip_link(&href) {
                continue;
            }

            let url = resolve_href(current_url, &href);
            if origin_from_url(&url).as_deref() != Some(current_origin.as_str()) {
                continue;
            }

            if checked.contains(&url) {
                continue;
            }

            let response = self
                .client
                .get(&url)
                .header(reqwest::header::ACCEPT, "text/html,*/*")
                .send()
                .map_err(|error| {
                    AegisError::new(format!("failed to check link '{href}' -> {url}: {error}"))
                })?;
            let status = response.status().as_u16();

            if status >= 400 {
                return Err(AegisError::new(format!(
                    "link '{href}' resolved to {url} returned HTTP {status}"
                )));
            }

            checked.push(url);
        }

        Ok(())
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
        Command::Init(args) => init_command(&args),
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
        "init" => parse_init_args(args).map(Command::Init),
        "smoke" => parse_smoke_args(args).map(Command::Smoke),
        "fast" => parse_fast_args(args).map(Command::Fast),
        "test" => parse_fast_args(args).map(Command::Fast),
        "browser" => Ok(Command::Browser),
        other => Err(AegisError::new(format!(
            "unknown command '{other}'. Run `aegis --help`."
        ))),
    }
}

fn parse_init_args(args: Vec<String>) -> Result<InitArgs> {
    let mut init = InitArgs { force: false };
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--force" | "-f" => init.force = true,
            "-h" | "--help" => {
                print_init_help();
                return Ok(init);
            }
            other => {
                return Err(AegisError::new(format!(
                    "unknown init option '{other}'. Run `aegis init --help`."
                )));
            }
        }

        index += 1;
    }

    Ok(init)
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
            "--format" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| AegisError::new("--format requires a value"))?;
                fast.format = parse_output_format(value)?;
            }
            "--fail-fast" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| AegisError::new("--fail-fast requires true or false"))?;
                fast.fail_fast = parse_bool_flag("--fail-fast", value)?;
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

fn parse_output_format(value: &str) -> Result<OutputFormat> {
    match value {
        "text" => Ok(OutputFormat::Text),
        "json" => Ok(OutputFormat::Json),
        other => Err(AegisError::new(format!(
            "unknown output format '{other}'. Expected 'text' or 'json'."
        ))),
    }
}

fn parse_bool_flag(name: &str, value: &str) -> Result<bool> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(AegisError::new(format!(
            "{name} must be true or false; got '{other}'"
        ))),
    }
}

fn init_command(args: &InitArgs) -> Result<()> {
    write_init_file(PathBuf::from("aegis.toml"), INIT_AEGIS_TOML, args.force)?;
    write_init_file(PathBuf::from("Cargo.toml"), INIT_CARGO_TOML, args.force)?;
    write_init_file(PathBuf::from("src/lib.rs"), INIT_SRC_LIB_RS, args.force)?;
    write_init_file(
        PathBuf::from("tests/fast.rs"),
        INIT_TESTS_FAST_RS,
        args.force,
    )?;
    write_init_file(
        PathBuf::from("tests/fast/navigation.rs"),
        INIT_TESTS_FAST_NAVIGATION_RS,
        args.force,
    )?;
    write_init_file(
        PathBuf::from("tests/browser.rs"),
        INIT_TESTS_BROWSER_RS,
        args.force,
    )?;
    write_init_file(
        PathBuf::from("tests/browser/drawer.rs"),
        INIT_TESTS_BROWSER_DRAWER_RS,
        args.force,
    )?;

    println!("Aegis project files created.");
    println!("Next:");
    println!("  aegis fast --config aegis.toml");
    println!("  cargo test --test fast");
    Ok(())
}

fn write_init_file(path: PathBuf, contents: &str, force: bool) -> Result<()> {
    if path.exists() && !force {
        return Err(AegisError::new(format!(
            "{} already exists; rerun with --force to overwrite",
            path.display()
        )));
    }

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|error| {
                AegisError::new(format!("failed to create '{}': {error}", parent.display()))
            })?;
        }
    }

    fs::write(&path, contents)
        .map_err(|error| AegisError::new(format!("failed to write '{}': {error}", path.display())))
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
    let report = collect_fast_suite_report(args);

    match (args.format, report) {
        (OutputFormat::Text, Ok(report)) => {
            print_fast_suite_report(&report);
            Ok(())
        }
        (OutputFormat::Json, Ok(report)) => print_fast_json_report(&report),
        (OutputFormat::Text, Err(error)) => Err(AegisError::new(error.message)),
        (OutputFormat::Json, Err(error)) => {
            if let Some(report) = error.report {
                print_fast_json_report(&report)?;
                Err(AegisError::reported(error.message))
            } else {
                Err(AegisError::new(error.message))
            }
        }
    }
}

fn collect_fast_suite_report(args: &FastArgs) -> FastSuiteResult {
    let config_source = fs::read_to_string(&args.config).map_err(|error| {
        FastSuiteError::new(format!(
            "failed to read config '{}': {error}",
            args.config.display()
        ))
    })?;
    let config =
        parse_config(&config_source).map_err(|error| FastSuiteError::new(error.message))?;
    let checks = collect_fast_checks(&config);

    if checks.is_empty() {
        return Err(FastSuiteError::new(format!(
            "config '{}' does not define any [[fast]] or [[smoke]] checks",
            args.config.display()
        )));
    }

    let mut reports = Vec::new();
    let mut failures = Vec::new();

    for (index, check) in checks.iter().enumerate() {
        let label = check
            .name
            .clone()
            .unwrap_or_else(|| format!("fast {}", index + 1));
        let report = match run_fast_check(&config, check) {
            Ok(report) => report,
            Err(error) => {
                let failure = FastFailureReport {
                    check: label,
                    error: error.to_string(),
                };
                failures.push(failure);

                if args.fail_fast {
                    return Err(FastSuiteError::with_report(
                        format!(
                            "check '{}' failed: {}",
                            failures[0].check, failures[0].error
                        ),
                        FastSuiteReport {
                            config: args.config.display().to_string(),
                            passed: false,
                            check_count: reports.len(),
                            checks: reports,
                            failures: failures.clone(),
                            failure: failures.first().cloned(),
                        },
                    ));
                }

                continue;
            }
        };
        reports.push(FastCheckReport {
            name: label,
            url: report.url,
            status: report.status,
            body_bytes: report.body_bytes,
            matched_text: report.matched_text,
        });
    }

    let passed = failures.is_empty();
    let report = FastSuiteReport {
        config: args.config.display().to_string(),
        passed,
        check_count: reports.len(),
        checks: reports,
        failure: failures.first().cloned(),
        failures,
    };

    if report.passed {
        Ok(report)
    } else {
        Err(FastSuiteError::with_report(
            format!("{} fast check(s) failed", report.failures.len()),
            report,
        ))
    }
}

type FastSuiteResult = std::result::Result<FastSuiteReport, FastSuiteError>;

struct FastSuiteError {
    message: String,
    report: Option<FastSuiteReport>,
}

impl FastSuiteError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            report: None,
        }
    }

    fn with_report(message: impl Into<String>, report: FastSuiteReport) -> Self {
        Self {
            message: message.into(),
            report: Some(report),
        }
    }
}

fn print_fast_json_report(report: &FastSuiteReport) -> Result<()> {
    let json = serde_json::to_string_pretty(report)
        .map_err(|error| AegisError::new(format!("failed to encode JSON report: {error}")))?;
    println!("{json}");
    Ok(())
}

fn print_fast_suite_report(report: &FastSuiteReport) {
    println!("Aegis fast checks started");
    println!("  config: {}", report.config);

    for check in &report.checks {
        println!(
            "  ok {}: {} HTTP {} ({} bytes)",
            check.name, check.url, check.status, check.body_bytes
        );
    }

    for failure in &report.failures {
        println!("  fail {}: {}", failure.check, failure.error);
    }

    if report.passed {
        println!("Aegis fast checks passed: {} check(s)", report.check_count);
    } else {
        println!(
            "Aegis fast checks failed: {} passed, {} failed",
            report.check_count,
            report.failures.len()
        );
    }
}

pub fn parse_config(source: &str) -> Result<AegisConfig> {
    toml::from_str::<AegisConfig>(source)
        .map_err(|error| AegisError::new(format!("failed to parse aegis config: {error}")))
}

fn collect_fast_checks(config: &AegisConfig) -> Vec<FastCheckConfig> {
    let mut checks = config.fast.clone();

    for smoke in &config.smoke {
        checks.push(FastCheckConfig {
            name: smoke.name.clone(),
            goto: smoke.path.clone(),
            url: smoke.url.clone(),
            click: None,
            expect_text: smoke.expect.clone(),
            expect_link: None,
            expect_links: Vec::new(),
            check_links: false,
            expect_all: smoke.expect_all.clone(),
            expect_not: smoke.expect_not.clone(),
            status: smoke.status,
        });
    }

    checks
}

fn run_fast_check(config: &AegisConfig, check: &FastCheckConfig) -> Result<SmokeReport> {
    let first_url = match (check.url.as_deref(), check.goto.as_deref()) {
        (Some(url), None) => url.to_string(),
        (None, Some(goto)) => resolve_config_url(config, goto)?,
        (Some(_), Some(_)) => {
            return Err(AegisError::new(
                "fast check must use either url or goto, not both",
            ));
        }
        (None, None) => return Err(AegisError::new("fast check requires url or goto")),
    };

    let mut page = FastPage::new(check.name.as_deref().unwrap_or("fast check"));
    page.try_goto(&first_url)?;
    page.try_expect_status(check.status)?;

    if let Some(selector) = check.click.as_deref() {
        page.try_click(selector)?;
        page.try_expect_status(check.status)?;
    }

    if let Some(expected) = check.expect_text.as_deref() {
        page.try_expect_text(expected)?;
    }

    if let Some(href) = check.expect_link.as_deref() {
        page.try_expect_link(href)?;
    }

    for href in &check.expect_links {
        page.try_expect_link(href)?;
    }

    for expected in &check.expect_all {
        page.try_expect_text(expected)?;
    }

    for unexpected in &check.expect_not {
        page.try_expect_not(unexpected)?;
    }

    if check.check_links {
        page.try_check_links()?;
    }

    Ok(SmokeReport {
        url: page.current_url.clone().unwrap_or(first_url),
        status: page.current_status.unwrap_or(check.status),
        body_bytes: page.current_body.len(),
        matched_text: check.expect_text.clone(),
    })
}

fn resolve_config_url(config: &AegisConfig, path_or_url: &str) -> Result<String> {
    if path_or_url.starts_with("http://") || path_or_url.starts_with("https://") {
        return Ok(path_or_url.to_string());
    }

    Ok(join_url(
        config
            .base_url
            .as_deref()
            .ok_or_else(|| AegisError::new("fast check with relative goto requires base_url"))?,
        path_or_url,
    ))
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

fn body_has_link_href(body: &str, href: &str) -> bool {
    let quoted_href_double = format!("href=\"{href}\"");
    let quoted_href_single = format!("href='{href}'");
    body.contains(&quoted_href_double) || body.contains(&quoted_href_single)
}

fn extract_link_hrefs(body: &str) -> Vec<String> {
    let mut hrefs = Vec::new();
    let mut rest = body;

    while let Some(index) = rest.find("href=") {
        rest = &rest[index + "href=".len()..];
        let Some(quote) = rest.chars().next() else {
            break;
        };

        if quote != '\'' && quote != '"' {
            continue;
        }

        rest = &rest[quote.len_utf8()..];
        let Some(end) = rest.find(quote) else {
            break;
        };
        hrefs.push(rest[..end].to_string());
        rest = &rest[end + quote.len_utf8()..];
    }

    hrefs
}

fn should_skip_link(href: &str) -> bool {
    let trimmed = href.trim();
    trimmed.is_empty()
        || trimmed.starts_with('#')
        || trimmed.starts_with("mailto:")
        || trimmed.starts_with("tel:")
        || trimmed.starts_with("javascript:")
        || trimmed.starts_with("data:")
}

fn origin_from_url(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split('/').next().unwrap_or(rest);
    Some(format!("{scheme}://{authority}"))
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
    println!("  aegis init");
    println!("  aegis smoke --url http://127.0.0.1:3000 --expect Axonyx");
    println!("  aegis fast --config aegis.toml");
    println!("  aegis browser");
    println!();
    println!("Commands:");
    println!("  doctor   Print local runner readiness.");
    println!("  init     Create aegis.toml and tests/ examples.");
    println!("  smoke    Run a fast HTTP smoke check against a local site.");
    println!("  fast     Run fast HTTP/response checks from aegis.toml.");
    println!("  test     Alias for fast, kept for convenience.");
    println!("  browser  Reserved placeholder for the future browser engine.");
}

fn print_init_help() {
    println!("Usage:");
    println!("  aegis init");
    println!("  aegis init --force");
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
    println!("  aegis fast --config aegis.toml --format json");
    println!("  aegis fast --config aegis.toml --fail-fast false");
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
                config: PathBuf::from("aegis.toml"),
                format: OutputFormat::Text,
                fail_fast: true,
            })
        );
    }

    #[test]
    fn parses_init_force_command() {
        let command = parse_command(["init".to_string(), "--force".to_string()]).unwrap();

        assert_eq!(command, Command::Init(InitArgs { force: true }));
    }

    #[test]
    fn parses_test_alias_for_fast_command() {
        let command = parse_command(["test".to_string()]).unwrap();

        assert_eq!(
            command,
            Command::Fast(FastArgs {
                config: PathBuf::from("aegis.toml"),
                format: OutputFormat::Text,
                fail_fast: true,
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
                config: PathBuf::from("react.aegis.toml"),
                format: OutputFormat::Text,
                fail_fast: true,
            })
        );
    }

    #[test]
    fn parses_fast_json_format_option() {
        let command = parse_command([
            "fast".to_string(),
            "--config".to_string(),
            "react.aegis.toml".to_string(),
            "--format".to_string(),
            "json".to_string(),
        ])
        .unwrap();

        assert_eq!(
            command,
            Command::Fast(FastArgs {
                config: PathBuf::from("react.aegis.toml"),
                format: OutputFormat::Json,
                fail_fast: true,
            })
        );
    }

    #[test]
    fn parses_fast_fail_fast_false_option() {
        let command = parse_command([
            "fast".to_string(),
            "--fail-fast".to_string(),
            "false".to_string(),
        ])
        .unwrap();

        assert_eq!(
            command,
            Command::Fast(FastArgs {
                config: PathBuf::from("aegis.toml"),
                format: OutputFormat::Text,
                fail_fast: false,
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
    fn parses_frontend_friendly_fast_checks() {
        let config = parse_config(
            r#"
base_url = "https://react.axonyx.dev"

[[fast]]
name = "opens docs"
goto = "/"
click = "a[href='/docs/getting-started']"
expect_text = "Getting Started"
expect_all = ["Axonyx", "Docs"]
expect_link = "/components"
expect_links = ["/api/button", "/react"]
check_links = true
expect_not = ["Internal Server Error"]
"#,
        )
        .unwrap();

        assert_eq!(config.fast.len(), 1);
        assert_eq!(
            config.fast[0].click.as_deref(),
            Some("a[href='/docs/getting-started']")
        );
        assert_eq!(
            config.fast[0].expect_text.as_deref(),
            Some("Getting Started")
        );
        assert_eq!(config.fast[0].expect_link.as_deref(), Some("/components"));
        assert_eq!(config.fast[0].expect_links, ["/api/button", "/react"]);
        assert!(config.fast[0].check_links);
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
    fn detects_link_href_in_html() {
        let html = r#"<a href="/docs">Docs</a><a href='/components'>Components</a>"#;
        assert!(body_has_link_href(html, "/docs"));
        assert!(body_has_link_href(html, "/components"));
        assert!(!body_has_link_href(html, "/missing"));
    }

    #[test]
    fn extracts_link_hrefs_from_html() {
        let html = r##"
<a href="/docs">Docs</a>
<a class="button" href='/components'>Components</a>
<a href="#top">Top</a>
"##;
        assert_eq!(extract_link_hrefs(html), ["/docs", "/components", "#top"]);
    }

    #[test]
    fn skips_non_http_navigation_links() {
        assert!(should_skip_link(""));
        assert!(should_skip_link("#top"));
        assert!(should_skip_link("mailto:hello@example.com"));
        assert!(should_skip_link("tel:+381"));
        assert!(should_skip_link("javascript:void(0)"));
        assert!(should_skip_link("data:text/plain,hello"));
        assert!(!should_skip_link("/docs"));
    }

    #[test]
    fn extracts_origin_from_absolute_url() {
        assert_eq!(
            origin_from_url("https://react.axonyx.dev/docs/getting-started").as_deref(),
            Some("https://react.axonyx.dev")
        );
        assert_eq!(origin_from_url("/docs"), None);
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
