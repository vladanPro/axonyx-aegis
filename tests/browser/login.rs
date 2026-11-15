#[test]
#[ignore = "requires local Axonyx docs on port 3000 and a running ChromeDriver"]
fn opens_docs() {
    aegis::browser("opens docs", |page| {
        page.goto("http://127.0.0.1:3000");
        page.click("a[href='/docs/getting-started']");
        page.expect_text("Getting Started");
    });
}
