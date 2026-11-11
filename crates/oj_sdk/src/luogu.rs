use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::header;
use reqwest::{Method, Response};
use serde_json::Value;

use crate::error::{Error, Result, api_error, login_error};
use crate::model::{
    Captcha, Language, LoginMethods, Problem, ProblemPage, ProblemQuery, Record, RecordPage,
    RecordQuery, Training, TrainingPage, User,
};
use crate::session::Session;

const DEFAULT_BASE_URL: &str = "https://www.luogu.com.cn";
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";
const HTML_ACCEPT: &str =
    "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8";
const JSON_ACCEPT: &str = "application/json, text/plain, */*";

/// A client for luogu.com.cn.
///
/// The easiest way to get started is [`LuoguClient::login_with_password`] when
/// the caller can show a captcha image, or [`LuoguClient::with_session`] with a
/// cookie string copied from a browser.
pub struct LuoguClient {
    http: reqwest::Client,
    session: Session,
    base_url: String,
}

impl LuoguClient {
    /// A client with no session. Luogu's problem list and problem statements
    /// are readable anonymously.
    pub fn anonymous() -> Result<Self> {
        Self::with_session(Session::new())
    }

    pub fn with_session(session: Session) -> Result<Self> {
        let http = reqwest::Client::builder()
            .use_rustls_tls()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(Error::Http)?;

        Ok(Self {
            http,
            session,
            base_url: DEFAULT_BASE_URL.to_string(),
        })
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Logs in with an account and password, using the captcha code the user
    /// read from [`LuoguClient::fetch_captcha`].
    ///
    /// Luogu expects the captcha and password to be sent in one request, so the
    /// caller must fetch a captcha image, show it to the user, and pass the
    /// typed code here. The password is only used for this request and is never
    /// stored.
    pub async fn login_with_password(
        &mut self,
        account: &str,
        password: &str,
        captcha: &str,
    ) -> Result<()> {
        let login_page = format!("{}/auth/login", self.base_url);
        let csrf_token = self.csrf_token(&login_page).await?;

        let body = serde_json::json!({
            "username": account,
            "password": password,
            "captcha": captcha,
        });

        let request = self
            .request(Method::POST, format!("{}/do-auth/password", self.base_url))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::REFERER, &login_page)
            .header(header::ORIGIN, &self.base_url)
            .header("X-CSRF-TOKEN", csrf_token)
            .body(body.to_string());

        let (status, text) = self.send(request).await?;
        if !(200..300).contains(&status) {
            return Err(login_error(status, text));
        }

        // Successful responses contain `redirectTo`; an error can still come
        // back with a 200 status, so check the payload before accepting it.
        if let Ok(value) = serde_json::from_str::<Value>(&text) {
            if value.get("errorType").and_then(Value::as_str).is_some() {
                return Err(login_error(status, text));
            }
        }

        Ok(())
    }

    /// Fetches a captcha image and stores the cookies Luogu rotates with it.
    ///
    /// The returned JPEG can be displayed directly. Fetching a new captcha
    /// invalidates the previous one, so callers should show exactly one image
    /// and submit the code with [`LuoguClient::login_with_password`].
    /// Fetches the image captcha shown on the login page.
    pub async fn fetch_captcha(&mut self) -> Result<Captcha> {
        self.fetch_captcha_from("/lg4/captcha").await
    }

    /// Fetches the image captcha that Luogu asks for when submitting too fast.
    pub async fn fetch_submit_captcha(&mut self) -> Result<Captcha> {
        self.fetch_captcha_from("/api/verify/captcha").await
    }

    async fn fetch_captcha_from(&mut self, path: &str) -> Result<Captcha> {
        let request = self
            .request(Method::GET, format!("{}{}", self.base_url, path))
            .query(&[("_t", cache_bust())]);

        let response = request.send().await?;
        let status = response.status().as_u16();
        self.absorb_response_cookies(&response);

        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("image/jpeg")
            .to_string();

        let image = response.bytes().await?.to_vec();
        if !(200..300).contains(&status) {
            return Err(Error::Status {
                status,
                body: String::from_utf8_lossy(&image).into_owned(),
            });
        }

        Ok(Captcha {
            image,
            content_type,
        })
    }

    /// Returns the login methods available for an account, usually
    /// `["password"]`.
    pub async fn login_methods(&mut self, account: &str) -> Result<LoginMethods> {
        let request = self
            .request(Method::GET, format!("{}/auth/login-methods", self.base_url))
            .query(&[("login", account)]);

        let value = self.json(request).await?;
        serde_json::from_value(value)
            .map_err(|error| Error::parse(format!("login methods: {error}")))
    }

    /// The signed-in user, using the `_uid` cookie as the identity.
    pub async fn whoami(&mut self) -> Result<User> {
        let uid = self.session.uid().ok_or(Error::NotLoggedIn)?;
        let request = self.request(Method::GET, format!("{}/user/{}", self.base_url, uid));
        let (status, text) = self.send(request).await?;

        if status == 401 || status == 403 || status == 404 {
            return Err(Error::NotLoggedIn);
        }
        if !(200..300).contains(&status) {
            return Err(api_error(status, text));
        }

        let context = extract_lentille_context(&text)?;
        let user = context
            .get("data")
            .and_then(|data| data.get("user"))
            .ok_or(Error::NotLoggedIn)?;

        serde_json::from_value(user.clone())
            .map_err(|error| Error::parse(format!("user response: {error}")))
    }

    pub async fn list_problems(&mut self, query: &ProblemQuery) -> Result<ProblemPage> {
        let mut params: Vec<(&str, String)> = Vec::new();
        if let Some(page) = query.page {
            params.push(("page", page.to_string()));
        }
        if let Some(problem_type) = &query.problem_type {
            params.push(("type", problem_type.clone()));
        }
        if let Some(keyword) = &query.keyword {
            params.push(("keyword", keyword.clone()));
        }
        if let Some(difficulty) = query.difficulty {
            params.push(("difficulty", difficulty.id().to_string()));
        }
        if let Some(tag) = query.tag {
            params.push(("tag", tag.to_string()));
        }

        let request = self
            .request(Method::GET, format!("{}/problem/list", self.base_url))
            .query(&params);
        let (status, body) = self.send(request).await?;
        if !(200..300).contains(&status) {
            return Err(api_error(status, body));
        }
        parse_problem_list_page(&body)
    }

    pub async fn get_problem(&mut self, pid: &str) -> Result<Problem> {
        let html = self.page(self.problem_url(pid)).await?;
        parse_problem_page(&html)
    }

    /// Submits a solution and returns the record id.
    ///
    /// If Luogu asks for a captcha, this returns [`Error::CaptchaRequired`] and
    /// the caller should obtain a token with [`LuoguClient::fetch_captcha`] and
    /// retry with [`LuoguClient::submit_with_captcha`].
    pub async fn submit(
        &mut self,
        pid: &str,
        code: &str,
        language: Language,
        enable_o2: bool,
        contest_id: Option<u64>,
    ) -> Result<u64> {
        self.submit_with_captcha(pid, code, language, enable_o2, contest_id, None)
            .await
    }

    pub async fn submit_with_captcha(
        &mut self,
        pid: &str,
        code: &str,
        language: Language,
        enable_o2: bool,
        contest_id: Option<u64>,
        captcha: Option<&str>,
    ) -> Result<u64> {
        let problem_url = self.problem_url(pid);
        let csrf_token = self.csrf_token(&problem_url).await?;

        let mut body = serde_json::json!({
            "lang": language.id(),
            "code": code,
            "enableO2": if enable_o2 { 1 } else { 0 },
        });
        if let Some(captcha) = captcha {
            body["captcha"] = Value::String(captcha.to_string());
        }

        let mut request = self
            .request(
                Method::POST,
                format!("{}/fe/api/problem/submit/{}", self.base_url, pid),
            )
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::REFERER, &problem_url)
            .header(header::ORIGIN, &self.base_url)
            .header("X-CSRF-TOKEN", csrf_token)
            .body(body.to_string());

        if let Some(contest_id) = contest_id {
            request = request.query(&[("contestId", contest_id.to_string())]);
        }

        let (status, text) = self.send(request).await?;
        if !(200..300).contains(&status) {
            return Err(api_error(status, text));
        }

        let value: Value = serde_json::from_str(&text)
            .map_err(|error| Error::parse(format!("submit response: {error}")))?;
        if value.get("errorType").is_some() {
            return Err(api_error(status, text));
        }

        value
            .get("rid")
            .and_then(Value::as_u64)
            .ok_or_else(|| Error::parse("submit response has no rid"))
    }

    pub async fn get_record(&mut self, record_id: u64) -> Result<Record> {
        let request = self.request(
            Method::GET,
            format!("{}/record/{}", self.base_url, record_id),
        );
        let (status, text) = self.send(request).await?;

        if status == 401 || status == 403 {
            return Err(Error::NotLoggedIn);
        }
        if !(200..300).contains(&status) {
            return Err(api_error(status, text));
        }

        parse_record_page(&text)
    }

    pub async fn record_list(&mut self, query: &RecordQuery) -> Result<RecordPage> {
        let mut params: Vec<(&str, String)> = Vec::new();
        if let Some(page) = query.page {
            params.push(("page", page.to_string()));
        }
        if let Some(pid) = &query.pid {
            params.push(("pid", pid.clone()));
        }
        if let Some(user) = query.user {
            params.push(("user", user.to_string()));
        }

        let request = self
            .request(Method::GET, format!("{}/record/list", self.base_url))
            .query(&params);

        let (status, body) = self.send(request).await?;
        if !(200..300).contains(&status) {
            return Err(api_error(status, body));
        }
        parse_record_list_page(&body)
    }

    pub async fn list_trainings(&mut self, page: u32) -> Result<TrainingPage> {
        let request = self
            .request(Method::GET, format!("{}/training/list", self.base_url))
            .query(&[("page", page.to_string())]);

        let (status, body) = self.send(request).await?;
        if !(200..300).contains(&status) {
            return Err(api_error(status, body));
        }
        parse_training_list_page(&body)
    }

    pub async fn get_training(&mut self, training_id: u64) -> Result<Training> {
        let request = self.request(
            Method::GET,
            format!("{}/training/{}", self.base_url, training_id),
        );
        let (status, body) = self.send(request).await?;
        if !(200..300).contains(&status) {
            return Err(api_error(status, body));
        }
        parse_training_page(&body)
    }

    pub async fn wait_for_record(&mut self, record_id: u64, timeout: Duration) -> Result<Record> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let record = self.get_record(record_id).await?;
            if record.is_finished() {
                return Ok(record);
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(Error::Timeout { record_id });
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    fn request(&self, method: Method, url: String) -> reqwest::RequestBuilder {
        let mut builder = self
            .http
            .request(method, url)
            .header(header::ACCEPT, HTML_ACCEPT)
            .header("X-Requested-With", "XMLHttpRequest");

        if !self.session.is_empty() {
            builder = builder.header(header::COOKIE, self.session.to_cookie_string());
        }

        builder
    }

    async fn send(&mut self, request: reqwest::RequestBuilder) -> Result<(u16, String)> {
        let response = request.send().await?;
        let status = response.status().as_u16();
        self.absorb_response_cookies(&response);
        let text = response.text().await?;
        Ok((status, text))
    }

    async fn page(&mut self, url: String) -> Result<String> {
        let (status, body) = self.send(self.request(Method::GET, url)).await?;
        if !(200..300).contains(&status) {
            return Err(api_error(status, body));
        }
        Ok(body)
    }

    async fn json(&mut self, request: reqwest::RequestBuilder) -> Result<Value> {
        let request = request.header(header::ACCEPT, JSON_ACCEPT);
        let (status, body) = self.send(request).await?;
        if !(200..300).contains(&status) {
            return Err(api_error(status, body));
        }
        serde_json::from_str(&body).map_err(|error| Error::parse(format!("invalid JSON: {error}")))
    }

    async fn csrf_token(&mut self, page_url: &str) -> Result<String> {
        let html = self.page(page_url.to_string()).await?;
        extract_meta_csrf_token(&html).ok_or_else(|| Error::parse("csrf-token meta tag not found"))
    }

    fn absorb_response_cookies(&mut self, response: &Response) {
        let headers: Vec<String> = response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok().map(|value| value.to_string()))
            .collect();
        self.session
            .update_from_headers(headers.iter().map(String::as_str));
    }

    fn problem_url(&self, pid: &str) -> String {
        format!("{}/problem/{}", self.base_url, pid)
    }
}

fn cache_bust() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or(0);
    format!("{millis}")
}

/// Extracts the JSON embedded in Luogu's `lentille-context` script tag.
pub fn extract_lentille_context(html: &str) -> Result<Value> {
    let mut search_start = 0;
    while let Some(relative_start) = html[search_start..].find("<script") {
        let start = search_start + relative_start;
        let Some(relative_tag_end) = html[start..].find('>') else {
            break;
        };
        let tag_end = start + relative_tag_end;
        let tag = &html[start..tag_end];

        if tag.contains("id=\"lentille-context\"") {
            let content_start = tag_end + 1;
            let Some(relative_close) = html[content_start..].find("</script>") else {
                return Err(Error::parse("unterminated lentille-context script tag"));
            };
            let json = &html[content_start..content_start + relative_close];
            return serde_json::from_str(json)
                .map_err(|error| Error::parse(format!("lentille-context: {error}")));
        }

        search_start = tag_end + 1;
    }

    Err(Error::parse("lentille-context script tag not found"))
}

/// Extracts the CSRF token from `<meta name="csrf-token" content="...">`.
pub fn extract_meta_csrf_token(html: &str) -> Option<String> {
    let mut search_start = 0;
    while let Some(relative_start) = html[search_start..].find("<meta") {
        let start = search_start + relative_start;
        let Some(relative_end) = html[start..].find('>') else {
            break;
        };
        let end = start + relative_end;
        let tag = &html[start..end];

        if tag.contains("name=\"csrf-token\"") {
            if let Some(content_offset) = tag.find("content=\"") {
                let value_start = start + content_offset + "content=\"".len();
                if let Some(relative_quote) = html[value_start..].find('"') {
                    return Some(html[value_start..value_start + relative_quote].to_string());
                }
            }
        }

        search_start = end + 1;
    }

    None
}

fn response_data(value: &Value) -> Result<&Value> {
    value
        .get("data")
        .ok_or_else(|| Error::parse("missing data in response"))
}

fn check_template(context: &Value, expected: &str) -> Result<()> {
    let template = context
        .get("template")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if template != expected {
        return Err(Error::parse(format!(
            "expected template {expected:?}, got {template:?}"
        )));
    }
    Ok(())
}

pub fn parse_problem_page(html: &str) -> Result<Problem> {
    let context = extract_lentille_context(html)?;
    check_template(&context, "problem.show")?;

    let problem = response_data(&context)?
        .get("problem")
        .ok_or_else(|| Error::parse("missing data.problem"))?;

    serde_json::from_value(problem.clone())
        .map_err(|error| Error::parse(format!("problem response: {error}")))
}

pub fn parse_problem_list_page(html: &str) -> Result<ProblemPage> {
    let context = extract_lentille_context(html)?;
    check_template(&context, "problem.list")?;
    parse_problem_list_value(&context)
}

pub fn parse_record_page(html: &str) -> Result<Record> {
    let context = extract_lentille_context(html)?;
    check_template(&context, "record.show")?;

    let record = response_data(&context)?
        .get("record")
        .ok_or_else(|| Error::parse("missing data.record"))?;

    serde_json::from_value(record.clone())
        .map_err(|error| Error::parse(format!("record response: {error}")))
}

pub fn parse_record_list_page(html: &str) -> Result<RecordPage> {
    let context = extract_lentille_context(html)?;
    check_template(&context, "record.list")?;
    parse_record_list_value(&context)
}

pub fn parse_training_list_page(html: &str) -> Result<TrainingPage> {
    let context = extract_lentille_context(html)?;
    check_template(&context, "training.list")?;

    let trainings = response_data(&context)?
        .get("trainings")
        .ok_or_else(|| Error::parse("missing data.trainings"))?;

    serde_json::from_value(trainings.clone())
        .map_err(|error| Error::parse(format!("training list response: {error}")))
}

pub fn parse_training_page(html: &str) -> Result<Training> {
    let context = extract_lentille_context(html)?;
    check_template(&context, "training.show")?;

    let training = response_data(&context)?
        .get("training")
        .ok_or_else(|| Error::parse("missing data.training"))?;

    serde_json::from_value(training.clone())
        .map_err(|error| Error::parse(format!("training response: {error}")))
}

pub fn parse_user_page(html: &str) -> Result<User> {
    let context = extract_lentille_context(html)?;
    check_template(&context, "user.show")?;

    let user = response_data(&context)?
        .get("user")
        .ok_or_else(|| Error::parse("missing data.user"))?;

    serde_json::from_value(user.clone())
        .map_err(|error| Error::parse(format!("user response: {error}")))
}

fn parse_problem_list_value(value: &Value) -> Result<ProblemPage> {
    let problems = response_data(value)?
        .get("problems")
        .ok_or_else(|| Error::parse("missing data.problems"))?;

    serde_json::from_value(problems.clone())
        .map_err(|error| Error::parse(format!("problem list response: {error}")))
}

fn parse_record_list_value(value: &Value) -> Result<RecordPage> {
    let records = response_data(value)?
        .get("records")
        .ok_or_else(|| Error::parse("missing data.records"))?;

    serde_json::from_value(records.clone())
        .map_err(|error| Error::parse(format!("record list response: {error}")))
}
