use crate::{AegisError, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::blocking::Client;
use reqwest::{Method, Url};
use serde_json::{Value, json};
use std::time::Duration;

const ELEMENT: &str = "element-6066-11e4-a52e-4f735466cecf";

#[derive(Debug, Clone, Copy)]
pub enum Browser {
    Chrome,
    Edge,
}

#[derive(Debug, Clone)]
pub struct BrowserOptions {
    pub browser: Browser,
    pub javascript: bool,
    pub headless: bool,
    pub binary: Option<String>,
}

impl Default for BrowserOptions {
    fn default() -> Self {
        Self {
            browser: Browser::Chrome,
            javascript: true,
            headless: true,
            binary: None,
        }
    }
}

impl BrowserOptions {
    fn capabilities(self) -> Value {
        let (name, key) = match self.browser {
            Browser::Chrome => ("chrome", "goog:chromeOptions"),
            Browser::Edge => ("MicrosoftEdge", "ms:edgeOptions"),
        };
        let args = if self.headless {
            vec!["--headless=new"]
        } else {
            Vec::new()
        };
        let mut options = json!({ "args": args });
        if let Some(binary) = self.binary {
            options["binary"] = json!(binary);
        }
        if !self.javascript {
            options["prefs"] = json!({ "profile.managed_default_content_settings.javascript": 2 });
        }
        json!({ "capabilities": { "alwaysMatch": { "browserName": name, (key): options,
            "timeouts": { "implicit": 0, "pageLoad": 15000, "script": 5000 }
        } } })
    }
}

pub enum Key {
    Tab,
    Enter,
    ArrowRight,
}

impl Key {
    fn webdriver_value(self) -> &'static str {
        match self {
            Self::Tab => "\u{E004}",
            Self::Enter => "\u{E007}",
            Self::ArrowRight => "\u{E014}",
        }
    }
}

pub struct BrowserPage {
    client: Client,
    endpoint: Url,
    session: Option<String>,
    browser: Browser,
}

impl BrowserPage {
    pub fn connect(endpoint: &str, options: BrowserOptions) -> Result<Self> {
        let endpoint =
            Url::parse(endpoint).map_err(|_| AegisError::new("invalid WebDriver URL"))?;
        if endpoint.scheme() != "http"
            || !matches!(
                endpoint.host_str(),
                Some("127.0.0.1" | "[::1]" | "localhost")
            )
            || endpoint.port().is_none()
            || endpoint.path() != "/"
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
        {
            return Err(AegisError::new(
                "WebDriver must be a local HTTP origin with an explicit port",
            ));
        }
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(25))
            .build()
            .map_err(|_| AegisError::new("could not create WebDriver HTTP client"))?;
        let mut page = Self {
            client,
            endpoint,
            session: None,
            browser: options.browser,
        };
        let result = page.request(Method::POST, &["session"], Some(options.capabilities()))?;
        page.session = Some(
            result
                .get("sessionId")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .ok_or_else(|| AegisError::new("WebDriver did not return a session ID"))?
                .to_owned(),
        );
        Ok(page)
    }

    pub fn goto(&mut self, url: &str) -> &mut Self {
        self.try_goto(url).unwrap_or_else(|error| panic!("{error}"));
        self
    }

    pub fn try_goto(&self, url: &str) -> Result<()> {
        self.command(Method::POST, &["url"], Some(json!({ "url": url })))?;
        Ok(())
    }

    pub fn click(&mut self, selector: &str) -> &mut Self {
        self.try_click(selector)
            .unwrap_or_else(|error| panic!("{error}"));
        self
    }

    pub fn try_click(&self, selector: &str) -> Result<()> {
        let element = self.element(selector)?;
        self.command(
            Method::POST,
            &["element", &element, "click"],
            Some(json!({})),
        )?;
        Ok(())
    }

    /// Type into an editable element using the browser's native input actions.
    pub fn type_text(&self, selector: &str, text: &str) -> Result<()> {
        let element = self.element(selector)?;
        self.command(
            Method::POST,
            &["element", &element, "value"],
            Some(json!({ "text": text })),
        )?;
        Ok(())
    }

    /// Clear an editable element using WebDriver's element clear command.
    pub fn clear_text(&self, selector: &str) -> Result<()> {
        let element = self.element(selector)?;
        self.command(
            Method::POST,
            &["element", &element, "clear"],
            Some(json!({})),
        )?;
        Ok(())
    }

    pub fn expect_text(&mut self, expected: &str) -> &mut Self {
        let element = self
            .element("body")
            .unwrap_or_else(|error| panic!("{error}"));
        let text = self
            .command(Method::GET, &["element", &element, "text"], None)
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(
            text.as_str().is_some_and(|text| text.contains(expected)),
            "missing visible text: {expected}"
        );
        self
    }

    pub fn press(&self, key: Key) -> Result<()> {
        let value = key.webdriver_value();
        self.command(
            Method::POST,
            &["actions"],
            Some(json!({
                "actions": [{ "type": "key", "id": "keyboard", "actions": [
                    { "type": "keyDown", "value": value }, { "type": "keyUp", "value": value }
                ] }]
            })),
        )?;
        Ok(())
    }

    pub fn evaluate(&self, script: &str, args: &[Value]) -> Result<Value> {
        self.command(
            Method::POST,
            &["execute", "sync"],
            Some(json!({ "script": script, "args": args })),
        )
    }

    pub fn current_url(&self) -> Result<String> {
        self.command(Method::GET, &["url"], None)?
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| AegisError::new("WebDriver did not return the current URL"))
    }

    /// Set the layout viewport independently of the browser's minimum window size.
    pub fn set_viewport(&self, width: u32, height: u32) -> Result<()> {
        if width == 0 || height == 0 {
            return Err(AegisError::new("viewport dimensions must be positive"));
        }
        self.cdp(
            "Emulation.setDeviceMetricsOverride",
            json!({ "width": width, "height": height, "deviceScaleFactor": 1, "mobile": false }),
        )?;
        Ok(())
    }

    /// Capture the complete document as PNG without changing the test viewport.
    pub fn full_page_png(&self) -> Result<Vec<u8>> {
        let metrics = self.cdp("Page.getLayoutMetrics", json!({}))?;
        let content = &metrics["cssContentSize"];
        let viewport = self.evaluate("return { width: innerWidth, height: innerHeight }", &[])?;
        let dimension = |key: &str| {
            let dimension = |rect: &Value| {
                rect[key]
                    .as_f64()
                    .map(f64::ceil)
                    .filter(|value| *value > 0.0)
                    .ok_or_else(|| {
                        AegisError::new("browser returned invalid screenshot dimensions")
                    })
            };
            Ok::<_, AegisError>(dimension(content)?.max(dimension(&viewport)?))
        };
        let (width, height) = (dimension("width")?, dimension("height")?);
        let response = self.cdp(
            "Page.captureScreenshot",
            json!({
                "format": "png",
                "captureBeyondViewport": true,
                "clip": { "x": 0, "y": 0, "width": width, "height": height, "scale": 1 }
            }),
        )?;
        let encoded = response["data"]
            .as_str()
            .ok_or_else(|| AegisError::new("browser returned no screenshot data"))?;
        let png = STANDARD
            .decode(encoded)
            .map_err(|_| AegisError::new("browser returned invalid screenshot encoding"))?;
        if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err(AegisError::new("browser returned a non-PNG screenshot"));
        }
        Ok(png)
    }

    fn cdp(&self, method: &str, params: Value) -> Result<Value> {
        let vendor = match self.browser {
            Browser::Chrome => "goog",
            Browser::Edge => "ms",
        };
        self.command(
            Method::POST,
            &[vendor, "cdp", "execute"],
            Some(json!({ "cmd": method, "params": params })),
        )
    }

    /// Install a session cookie for the current document's origin.
    pub fn add_session_cookie(&self, name: &str, value: &str) -> Result<()> {
        self.command(
            Method::POST,
            &["cookie"],
            Some(json!({ "cookie": { "name": name, "value": value, "path": "/", "httpOnly": true } })),
        )?;
        Ok(())
    }

    pub fn close(mut self) -> Result<()> {
        self.end_session()
    }

    fn end_session(&mut self) -> Result<()> {
        if let Some(session) = self.session.take() {
            self.request(Method::DELETE, &["session", &session], None)?;
        }
        Ok(())
    }

    fn element(&self, selector: &str) -> Result<String> {
        self.command(
            Method::POST,
            &["element"],
            Some(json!({ "using": "css selector", "value": selector })),
        )?
        .get(ELEMENT)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| AegisError::new("WebDriver did not return an element ID"))
    }

    fn command(&self, method: Method, path: &[&str], body: Option<Value>) -> Result<Value> {
        let session = self
            .session
            .as_deref()
            .ok_or_else(|| AegisError::new("WebDriver session is closed"))?;
        let mut segments = vec!["session", session];
        segments.extend_from_slice(path);
        self.request(method, &segments, body)
    }

    fn request(&self, method: Method, path: &[&str], body: Option<Value>) -> Result<Value> {
        let mut url = self.endpoint.clone();
        url.path_segments_mut()
            .expect("HTTP URL has path segments")
            .pop_if_empty()
            .extend(path);
        let mut request = self.client.request(method, url);
        if let Some(body) = body {
            request = request
                .header("Content-Type", "application/json")
                .body(body.to_string());
        }
        let response = request
            .send()
            .map_err(|error| AegisError::new(format!("WebDriver request failed: {error}")))?;
        let status = response.status();
        let payload = response
            .text()
            .map_err(|_| AegisError::new("could not read WebDriver response"))?;
        let envelope: Value = serde_json::from_str(&payload)
            .map_err(|_| AegisError::new(format!("invalid WebDriver response (HTTP {status})")))?;
        let value = envelope
            .get("value")
            .ok_or_else(|| AegisError::new("WebDriver response missing value"))?;
        if !status.is_success() {
            let code = value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            return Err(AegisError::new(format!(
                "WebDriver command failed: {code} (HTTP {status})"
            )));
        }
        Ok(value.clone())
    }
}

impl Drop for BrowserPage {
    fn drop(&mut self) {
        let _ = self.end_session();
    }
}
