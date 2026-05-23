#[test]
#[ignore = "example hits the live React adapter site"]
fn opens_docs() {
    aegis::fast("opens docs", |page| {
        page.goto("https://react.axonyx.dev");
        page.click("a[href='/docs/getting-started']");
        page.expect_all(&["Axonyx", "Docs"]);
        page.expect_text("Getting Started");
        page.expect_not("Internal Server Error");
    });
}
