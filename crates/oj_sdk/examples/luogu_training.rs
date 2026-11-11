use anyhow::{Context, Result};
use oj_sdk::{LuoguClient, Session};

#[tokio::main]
async fn main() -> Result<()> {
    let page_number = match std::env::args().nth(1) {
        Some(page) => page.parse().context("page number")?,
        None => 1,
    };

    let mut client = match std::env::var("LUOGU_COOKIE") {
        Ok(cookie) => LuoguClient::with_session(Session::from_cookie_string(&cookie)?)?,
        Err(_) => LuoguClient::anonymous()?,
    };

    let page = client
        .list_trainings(page_number)
        .await
        .context("listing trainings")?;

    println!("count={} per_page={}", page.count, page.per_page);
    for training in &page.result {
        println!(
            "{:<6} {:<32} problems={} marks={}",
            training.id, training.name, training.problem_count, training.mark_count
        );
    }

    let Some(training_id) = page.result.first().map(|training| training.id) else {
        return Ok(());
    };
    let training = client
        .get_training(training_id)
        .await
        .with_context(|| format!("loading training {training_id}"))?;
    println!(
        "\ntraining {} {} (problems={})",
        training.id(),
        training.name(),
        training.problems.len()
    );
    for problem in training.problems.iter().take(10) {
        println!(
            "  {:<8} {:<40} {}",
            problem.pid,
            problem.name,
            problem.difficulty()
        );
    }

    Ok(())
}
