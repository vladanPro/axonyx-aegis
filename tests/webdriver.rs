use aegis::{Browser, BrowserOptions, BrowserPage, Key};
use std::io::Write;
use std::net::TcpListener;
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn rejects_nonlocal_driver_origins() {
    for url in [
        "https://127.0.0.1:9515",
        "http://example.com:9515",
        "http://user@localhost:9515",
        "http://localhost:9515/wd/hub",
        "http://localhost",
    ] {
        assert!(BrowserPage::connect(url, BrowserOptions::default()).is_err());
    }
}

#[test]
#[ignore = "requires a running local WebDriver and its matching browser"]
fn native_keyboard_scroll_works_with_page_scripts_disabled() {
    let endpoint = std::env::var("AEGIS_WEBDRIVER_URL").expect("set AEGIS_WEBDRIVER_URL");
    let browser = match std::env::var("AEGIS_TEST_BROWSER").as_deref() {
        Ok("edge") => Browser::Edge,
        Ok("chrome") | Err(std::env::VarError::NotPresent) => Browser::Chrome,
        other => panic!("unsupported AEGIS_TEST_BROWSER: {other:?}"),
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    thread::spawn(move || {
        let html = r#"<!doctype html><title>Aegis browser fixture</title><body style="min-height: 1100px">
<div id="table" tabindex="0" style="width:150px;overflow-x:auto">
  <div style="width:900px">Posts table</div>
</div>
<a href="/next">Next page</a>
<input id="label" aria-label="Label">
<script>document.documentElement.setAttribute("data-script-ran", "yes")</script>"#;
        for mut stream in listener.incoming().flatten() {
            use std::io::{BufRead, BufReader};
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut line = String::new();
            for header in BufReader::new(&stream).lines() {
                match header {
                    Ok(header) if !header.is_empty() => line = header,
                    _ => break,
                }
            }
            if !line.is_empty() {
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
                    html.len()
                );
            }
        }
    });
    let url = format!("http://{address}");
    for javascript in [true, false] {
        let mut page = BrowserPage::connect(
            &endpoint,
            BrowserOptions {
                browser,
                javascript,
                ..BrowserOptions::default()
            },
        )
        .unwrap();
        page.goto(&url).expect_text("Posts table");
        page.set_viewport(320, 844).unwrap();
        assert_eq!(page.evaluate("return window.innerWidth", &[]).unwrap(), 320);
        let png = page.full_page_png().unwrap();
        let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
        let height = u32::from_be_bytes(png[20..24].try_into().unwrap());
        assert_eq!(width, 320);
        assert!(
            height >= 1100,
            "screenshot must include content below the viewport"
        );
        assert_eq!(
            page.evaluate(
                "return document.documentElement.hasAttribute('data-script-ran')",
                &[]
            )
            .unwrap(),
            javascript
        );
        page.press(Key::Tab).unwrap();
        assert_eq!(
            page.evaluate("return document.activeElement.id", &[])
                .unwrap(),
            "table"
        );
        page.press(Key::ArrowRight).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let offset = page
                .evaluate("return document.getElementById('table').scrollLeft", &[])
                .unwrap();
            if offset.as_f64().is_some_and(|offset| offset > 0.0) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "keyboard did not scroll the region"
            );
            thread::sleep(Duration::from_millis(40));
        }
        page.click("a");
        assert_eq!(page.current_url().unwrap(), format!("{url}/next"));
        page.type_text("#label", "Legura fixture").unwrap();
        assert_eq!(
            page.evaluate("return document.getElementById('label').value", &[])
                .unwrap(),
            "Legura fixture"
        );
        page.clear_text("#label").unwrap();
        assert_eq!(
            page.evaluate("return document.getElementById('label').value", &[])
                .unwrap(),
            ""
        );
        page.close().unwrap();
    }
}
