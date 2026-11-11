use anyhow::{Context, Result};
use oj_sdk::LuoguClient;

#[tokio::main]
async fn main() -> Result<()> {
    let pid = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "P1001".to_string());

    let mut client = LuoguClient::anonymous()?;
    let problem = client
        .get_problem(&pid)
        .await
        .with_context(|| format!("fetching problem {pid}"))?;

    println!(
        "{} {} [{}]",
        problem.pid,
        problem.name,
        problem.difficulty()
    );

    if let Some(content) = &problem.contenu {
        if let Some(description) = &content.description {
            println!("\n{description}");
        }
        if let Some(input_format) = &content.input_format {
            println!("\n输入格式：{input_format}");
        }
        if let Some(output_format) = &content.output_format {
            println!("\n输出格式：{output_format}");
        }
        if let Some(hint) = &content.hint {
            println!("\n提示：{hint}");
        }
    }

    if let Some(sample) = problem.samples.first() {
        println!("\n样例输入：\n{}", sample.input);
        println!("样例输出：\n{}", sample.output);
    }

    if let Some(first_time) = problem.limits.time.first() {
        println!("\n时间限制：{first_time} ms");
    }
    if let Some(first_memory) = problem.limits.memory.first() {
        println!("内存限制：{first_memory} KB");
    }

    Ok(())
}
