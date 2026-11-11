use anyhow::{Context, Result};
use oj_sdk::{LuoguClient, Session};

#[tokio::main]
async fn main() -> Result<()> {
    let cookie =
        std::env::var("LUOGU_COOKIE").context("set LUOGU_COOKIE to a Cookie header value")?;
    let mut client = LuoguClient::with_session(Session::from_cookie_string(&cookie)?)?;

    let user = client.whoami().await.context("fetching current user")?;
    println!("logged in as {} (uid {})", user.name, user.uid);

    Ok(())
}
