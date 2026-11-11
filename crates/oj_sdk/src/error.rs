/// Errors returned by the SDK.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Luogu returned status {status}: {body}")]
    Status { status: u16, body: String },

    #[error("not logged in")]
    NotLoggedIn,

    #[error("invalid credentials: {message}")]
    InvalidCredentials { message: String },

    #[error("password does not satisfy Luogu's policy: {message}")]
    PasswordPolicy { message: String },

    #[error("captcha is required or was rejected; complete it and retry with the code")]
    CaptchaRequired { raw: String },

    #[error("unsupported login method: {0}")]
    UnsupportedLoginMethod(String),

    #[error("failed to parse Luogu's response: {message}")]
    Parse { message: String },

    #[error("timed out waiting for record {record_id}")]
    Timeout { record_id: u64 },

    #[error("invalid cookie string: {message}")]
    InvalidCookie { message: String },
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn parse(message: impl Into<String>) -> Self {
        Self::Parse {
            message: message.into(),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct ErrorPayload {
    #[serde(rename = "errorType", default)]
    pub error_type: Option<String>,
    #[serde(rename = "errorMessage", default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

impl ErrorPayload {
    pub(crate) fn parse(body: &str) -> Option<Self> {
        serde_json::from_str(body).ok()
    }

    pub(crate) fn error_type(&self) -> &str {
        self.error_type.as_deref().unwrap_or_default()
    }

    /// A human-readable message, preferring `errorMessage` and falling back to
    /// a string `data` field.
    pub(crate) fn message(&self) -> String {
        if let Some(message) = &self.error_message {
            return message.clone();
        }
        match &self.data {
            Some(serde_json::Value::String(message)) => message.clone(),
            _ => String::new(),
        }
    }

    pub(crate) fn mentions(&self, fragment: &str) -> bool {
        self.error_type().contains(fragment)
    }
}

pub(crate) fn api_error(status: u16, body: String) -> Error {
    if let Some(payload) = ErrorPayload::parse(&body) {
        if payload.mentions("CaptchaNotMatchException")
            || payload.mentions("InvalidCaptchaException")
        {
            return Error::CaptchaRequired { raw: body };
        }
        if payload.mentions("UserNotLoggedIn") || payload.mentions("UserUnlogin") {
            return Error::NotLoggedIn;
        }
    }

    if status == 401 || status == 403 {
        return Error::NotLoggedIn;
    }

    Error::Status { status, body }
}

pub(crate) fn login_error(status: u16, body: String) -> Error {
    if let Some(payload) = ErrorPayload::parse(&body) {
        if payload.mentions("BadCredentialsException") {
            return Error::InvalidCredentials {
                message: payload.message(),
            };
        }
        if payload.mentions("BadRequestHttpException") {
            return Error::PasswordPolicy {
                message: payload.message(),
            };
        }
    }

    api_error(status, body)
}
