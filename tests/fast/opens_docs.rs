#[test]
#[ignore = "example hits the live React adapter site"]
fn opens_docs() {
    aegis::fast("opens docs", |page| {
        page.goto("https://react.axonyx.dev");
        page.click("a[href='/docs/getting-started']");
        page.expect_text("Getting Started");
    });
}
