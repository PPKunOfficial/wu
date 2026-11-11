use anyhow::{Context, Result};
use oj_sdk::{LuoguClient, ProblemQuery};

#[tokio::main]
async fn main() -> Result<()> {
    let page_number = match std::env::args().nth(1) {
        Some(page) => page.parse().context("page number")?,
        None => 1,
    };
    let problem_type = std::env::args().nth(2).unwrap_or_else(|| "P".to_string());

    let mut client = LuoguClient::anonymous()?;
    let page = client
        .list_problems(&ProblemQuery {
            page: Some(page_number),
            problem_type: Some(problem_type),
            ..ProblemQuery::default()
        })
        .await
        .context("listing problems")?;

    println!("count={} per_page={}", page.count, page.per_page);
    for problem in page.result.iter().take(20) {
        println!(
            "{:<8} {:<40} {}",
            problem.pid,
            problem.name,
            problem.difficulty()
        );
    }

    Ok(())
}
