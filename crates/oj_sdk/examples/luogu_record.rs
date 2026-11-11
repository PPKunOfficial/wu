use anyhow::{Context, Result};
use oj_sdk::{LuoguClient, Session};

#[tokio::main]
async fn main() -> Result<()> {
    let record_id: u64 = std::env::args()
        .nth(1)
        .context("usage: luogu_record <record-id>")?
        .parse()
        .context("record id")?;

    let mut client = match std::env::var("LUOGU_COOKIE") {
        Ok(cookie) => LuoguClient::with_session(Session::from_cookie_string(&cookie)?)?,
        Err(_) => LuoguClient::anonymous()?,
    };

    let record = client
        .get_record(record_id)
        .await
        .context("fetching record")?;

    println!("record {}: {}", record.id, record.status());
    println!(
        "score={} time={}ms memory={}KB language={}",
        record.score, record.time, record.memory, record.language
    );
    if let Some(problem) = &record.problem {
        println!("problem: {} {}", problem.pid, problem.name);
    }
    if let Some(source_code) = &record.source_code {
        println!("\n{source_code}");
    }

    Ok(())
}
