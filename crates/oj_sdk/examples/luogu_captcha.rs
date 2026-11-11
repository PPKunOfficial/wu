use anyhow::{Context, Result};
use oj_sdk::LuoguClient;

#[tokio::main]
async fn main() -> Result<()> {
    let account = std::env::args()
        .nth(1)
        .context("usage: luogu_captcha <account>")?;

    let mut client = LuoguClient::anonymous()?;
    let methods = client
        .login_methods(&account)
        .await
        .context("fetching login methods")?;
    println!(
        "available: {:?}, default: {}",
        methods.available, methods.default_method
    );

    let captcha = client.fetch_captcha().await.context("fetching captcha")?;
    println!(
        "captcha: {} bytes, content type {}",
        captcha.image.len(),
        captcha.content_type
    );

    Ok(())
}
