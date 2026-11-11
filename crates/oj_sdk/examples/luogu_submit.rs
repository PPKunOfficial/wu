use std::time::Duration;

use anyhow::{Context, Result, bail};
use oj_sdk::{Language, LuoguClient, Session};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let pid = args.get(1).cloned().unwrap_or_else(|| "P1001".to_string());
    let source_path = args
        .get(2)
        .cloned()
        .context("usage: luogu_submit <pid> <source-file> [language] [--o2]")?;

    let language = match args
        .iter()
        .skip(3)
        .find(|argument| !argument.starts_with("--"))
    {
        Some(language) => parse_language(language)?,
        None => Language::Cpp17,
    };
    let enable_o2 = args.iter().any(|argument| argument == "--o2");

    let code =
        std::fs::read_to_string(&source_path).with_context(|| format!("reading {source_path}"))?;
    if code.trim().is_empty() {
        bail!("{source_path} is empty");
    }

    let cookie = std::env::var("LUOGU_COOKIE")
        .context("set LUOGU_COOKIE to a Cookie header value from a logged-in browser")?;
    let session = Session::from_cookie_string(&cookie)?;
    let mut client = LuoguClient::with_session(session)?;

    let record_id = match client.submit(&pid, &code, language, enable_o2, None).await {
        Ok(record_id) => record_id,
        Err(oj_sdk::Error::CaptchaRequired { .. }) => {
            let captcha = client
                .fetch_submit_captcha()
                .await
                .context("fetching submit captcha")?;
            let path = std::env::temp_dir().join("luogu_submit_captcha.jpg");
            std::fs::write(&path, &captcha.image)
                .with_context(|| format!("writing {}", path.display()))?;
            eprintln!("Luogu wants a captcha; image saved to {}", path.display());
            eprint!("Enter the captcha code: ");
            let mut captcha_code = String::new();
            std::io::stdin()
                .read_line(&mut captcha_code)
                .context("reading captcha code")?;
            let captcha_code = captcha_code.trim();
            if captcha_code.is_empty() {
                bail!("captcha code is empty");
            }
            client
                .submit_with_captcha(&pid, &code, language, enable_o2, None, Some(captcha_code))
                .await
                .with_context(|| format!("submitting to {pid} with captcha"))?
        }
        Err(error) => {
            return Err(anyhow::Error::new(error).context(format!("submitting to {pid}")));
        }
    };

    println!("submitted: rid={record_id}");

    let record = client
        .wait_for_record(record_id, Duration::from_secs(120))
        .await
        .context("waiting for verdict")?;

    println!(
        "verdict: {} score={} time={}ms memory={}KB",
        record.status(),
        record.score,
        record.time,
        record.memory
    );

    Ok(())
}

fn parse_language(value: &str) -> Result<Language> {
    match value.to_ascii_lowercase().as_str() {
        "c" => Ok(Language::C),
        "cpp98" => Ok(Language::Cpp98),
        "cpp11" => Ok(Language::Cpp11),
        "cpp14" => Ok(Language::Cpp14),
        "cpp17" => Ok(Language::Cpp17),
        "cpp20" => Ok(Language::Cpp20),
        "cpp23" => Ok(Language::Cpp23),
        "python3" => Ok(Language::Python3),
        "pypy3" => Ok(Language::PyPy3),
        "go" => Ok(Language::Go),
        "rust" => Ok(Language::Rust),
        "java8" => Ok(Language::Java8),
        "java21" => Ok(Language::Java21),
        "node" | "nodejs" => Ok(Language::NodeJs),
        other => other
            .parse::<u32>()
            .map(Language::from_id)
            .context("language name or numeric id"),
    }
}
