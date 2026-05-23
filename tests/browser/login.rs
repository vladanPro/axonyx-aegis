#[test]
#[ignore = "browser engine is reserved for the next Aegis phase"]
fn login() {
    aegis::browser("login", |page| {
        page.goto("https://react.axonyx.dev/login");
        page.click("a[href='/profile/settings']");
        page.expect_text("Change theme");
    });
}
