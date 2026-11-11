use std::fmt;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Provider {
    #[serde(default)]
    pub uid: u64,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProblemSummary {
    #[serde(default)]
    pub pid: String,
    #[serde(rename = "type", default)]
    pub problem_type: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub difficulty: u8,
    #[serde(rename = "fullScore", default)]
    pub full_score: u32,
    #[serde(default)]
    pub tags: Vec<u32>,
    #[serde(rename = "totalSubmit", default)]
    pub total_submit: u64,
    #[serde(rename = "totalAccepted", default)]
    pub total_accepted: u64,
    #[serde(default)]
    pub flag: serde_json::Value,
    #[serde(default)]
    pub provider: Option<Provider>,
}

impl ProblemSummary {
    pub fn difficulty(&self) -> Difficulty {
        Difficulty::from_id(self.difficulty)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProblemPage {
    #[serde(rename = "perPage", default)]
    pub per_page: u32,
    #[serde(default)]
    pub count: u64,
    #[serde(default)]
    pub result: Vec<ProblemSummary>,
    #[serde(default)]
    pub filter: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProblemContent {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub background: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "formatI", default)]
    pub input_format: Option<String>,
    #[serde(rename = "formatO", default)]
    pub output_format: Option<String>,
    #[serde(default)]
    pub hint: Option<String>,
    #[serde(default)]
    pub locale: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample {
    pub input: String,
    pub output: String,
}

impl<'de> Deserialize<'de> for Sample {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let (input, output) = <(String, String)>::deserialize(deserializer)?;
        Ok(Self { input, output })
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Limits {
    /// Time limits in milliseconds, one entry per subtask.
    #[serde(default)]
    pub time: Vec<u32>,
    /// Memory limits in kilobytes, one entry per subtask.
    #[serde(default)]
    pub memory: Vec<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Problem {
    #[serde(default)]
    pub pid: String,
    #[serde(rename = "type", default)]
    pub problem_type: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub difficulty: u8,
    #[serde(rename = "fullScore", default)]
    pub full_score: u32,
    #[serde(default)]
    pub tags: Vec<u32>,
    #[serde(rename = "totalSubmit", default)]
    pub total_submit: u64,
    #[serde(rename = "totalAccepted", default)]
    pub total_accepted: u64,
    #[serde(default)]
    pub flag: serde_json::Value,
    #[serde(default)]
    pub provider: Option<Provider>,
    #[serde(default)]
    pub contenu: Option<ProblemContent>,
    #[serde(default)]
    pub content: Option<serde_json::Value>,
    #[serde(default)]
    pub samples: Vec<Sample>,
    #[serde(default)]
    pub limits: Limits,
    #[serde(rename = "showScore", default)]
    pub show_score: bool,
    #[serde(rename = "acceptSolution", default)]
    pub accept_solution: bool,
    #[serde(rename = "acceptLanguages", default)]
    pub accept_languages: Vec<u32>,
    #[serde(default)]
    pub translations: Vec<String>,
    #[serde(rename = "lastLanguage", default)]
    pub last_language: Option<u32>,
}

impl Problem {
    pub fn difficulty(&self) -> Difficulty {
        Difficulty::from_id(self.difficulty)
    }

    pub fn description(&self) -> &str {
        self.contenu
            .as_ref()
            .and_then(|content| content.description.as_deref())
            .or_else(|| self.content.as_ref().and_then(serde_json::Value::as_str))
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct User {
    #[serde(default)]
    pub uid: u64,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Record {
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub status: i32,
    #[serde(rename = "enableO2", default)]
    pub enable_o2: bool,
    #[serde(default)]
    pub score: u32,
    #[serde(default)]
    pub time: u32,
    #[serde(default)]
    pub memory: u32,
    #[serde(rename = "sourceCodeLength", default)]
    pub source_code_length: Option<u32>,
    #[serde(rename = "submitTime", default)]
    pub submit_time: i64,
    #[serde(default)]
    pub language: u32,
    #[serde(default)]
    pub user: Option<User>,
    #[serde(default)]
    pub contest: Option<serde_json::Value>,
    #[serde(default)]
    pub problem: Option<ProblemSummary>,
    #[serde(rename = "sourceCode", default)]
    pub source_code: Option<String>,
    #[serde(default)]
    pub detail: Option<serde_json::Value>,
}

impl Record {
    pub fn status(&self) -> RecordStatus {
        RecordStatus::from_id(self.status)
    }

    pub fn is_finished(&self) -> bool {
        self.status().is_finished()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecordPage {
    #[serde(rename = "perPage", default)]
    pub per_page: u32,
    #[serde(default)]
    pub count: u64,
    #[serde(default)]
    pub result: Vec<Record>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrainingSummary {
    pub id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default)]
    pub training_type: serde_json::Value,
    #[serde(default)]
    pub provider: Option<serde_json::Value>,
    #[serde(rename = "createTime", default)]
    pub create_time: Option<i64>,
    #[serde(default)]
    pub deadline: Option<serde_json::Value>,
    #[serde(rename = "problemCount", default)]
    pub problem_count: u32,
    #[serde(rename = "markCount", default)]
    pub mark_count: u32,
}

impl TrainingSummary {
    pub fn provider_name(&self) -> Option<&str> {
        self.provider.as_ref().and_then(serde_json::Value::as_str)
    }

    pub fn is_due(&self) -> bool {
        self.deadline
            .as_ref()
            .and_then(serde_json::Value::as_i64)
            .is_some_and(|deadline| deadline > 0)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrainingPage {
    #[serde(rename = "perPage", default)]
    pub per_page: u32,
    #[serde(default)]
    pub count: u64,
    #[serde(default)]
    pub result: Vec<TrainingSummary>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Training {
    #[serde(flatten)]
    pub summary: TrainingSummary,
    #[serde(default)]
    pub marked: bool,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub problems: Vec<ProblemSummary>,
}

impl Training {
    pub fn id(&self) -> u64 {
        self.summary.id
    }

    pub fn name(&self) -> &str {
        &self.summary.name
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoginMethods {
    #[serde(default)]
    pub available: Vec<String>,
    #[serde(rename = "default", default)]
    pub default_method: String,
}

/// A captcha image. Luogu serves a 90x35 JPEG.
#[derive(Debug, Clone)]
pub struct Captcha {
    pub image: Vec<u8>,
    pub content_type: String,
}

#[derive(Debug, Clone, Default)]
pub struct ProblemQuery {
    pub page: Option<u32>,
    /// Luogu problem type: `P`, `B`, `T`, `U`, `CF`, `SP`, `AT`, `UVA`, or `all`.
    pub problem_type: Option<String>,
    pub keyword: Option<String>,
    pub difficulty: Option<Difficulty>,
    pub tag: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct RecordQuery {
    pub page: Option<u32>,
    pub pid: Option<String>,
    pub user: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Difficulty {
    #[default]
    Unrated,
    Introductory,
    PopularMinus,
    Popular,
    PopularPlus,
    Advanced,
    AdvancedPlus,
    Provincial,
    National,
    Unknown(u8),
}

impl Difficulty {
    pub fn from_id(id: u8) -> Self {
        match id {
            0 => Self::Unrated,
            1 => Self::Introductory,
            2 => Self::PopularMinus,
            3 => Self::Popular,
            4 => Self::PopularPlus,
            5 => Self::Advanced,
            6 => Self::AdvancedPlus,
            7 => Self::Provincial,
            8 => Self::National,
            other => Self::Unknown(other),
        }
    }

    pub fn id(self) -> u8 {
        match self {
            Self::Unrated => 0,
            Self::Introductory => 1,
            Self::PopularMinus => 2,
            Self::Popular => 3,
            Self::PopularPlus => 4,
            Self::Advanced => 5,
            Self::AdvancedPlus => 6,
            Self::Provincial => 7,
            Self::National => 8,
            Self::Unknown(id) => id,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Unrated => "暂无评定",
            Self::Introductory => "入门",
            Self::PopularMinus => "普及−",
            Self::Popular => "普及",
            Self::PopularPlus => "普及+/提高−",
            Self::Advanced => "提高",
            Self::AdvancedPlus => "提高+/省选−",
            Self::Provincial => "省选/NOI−",
            Self::National => "NOI/NOI+/CTS",
            Self::Unknown(_) => "未知难度",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    C,
    Cpp98,
    Cpp11,
    Cpp14,
    Cpp17,
    Cpp20,
    Cpp23,
    Cpp14Gcc9,
    Python3,
    PyPy3,
    Go,
    Rust,
    Java8,
    Java21,
    NodeJs,
    Unknown(u32),
}

impl Language {
    pub fn from_id(id: u32) -> Self {
        match id {
            2 => Self::C,
            3 => Self::Cpp98,
            4 => Self::Cpp11,
            11 => Self::Cpp14,
            12 => Self::Cpp17,
            27 => Self::Cpp20,
            34 => Self::Cpp23,
            28 => Self::Cpp14Gcc9,
            7 => Self::Python3,
            25 => Self::PyPy3,
            14 => Self::Go,
            15 => Self::Rust,
            8 => Self::Java8,
            33 => Self::Java21,
            9 => Self::NodeJs,
            other => Self::Unknown(other),
        }
    }

    pub fn id(self) -> u32 {
        match self {
            Self::C => 2,
            Self::Cpp98 => 3,
            Self::Cpp11 => 4,
            Self::Cpp14 => 11,
            Self::Cpp17 => 12,
            Self::Cpp20 => 27,
            Self::Cpp23 => 34,
            Self::Cpp14Gcc9 => 28,
            Self::Python3 => 7,
            Self::PyPy3 => 25,
            Self::Go => 14,
            Self::Rust => 15,
            Self::Java8 => 8,
            Self::Java21 => 33,
            Self::NodeJs => 9,
            Self::Unknown(id) => id,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::C => "C",
            Self::Cpp98 => "C++98",
            Self::Cpp11 => "C++11",
            Self::Cpp14 => "C++14",
            Self::Cpp17 => "C++17",
            Self::Cpp20 => "C++20",
            Self::Cpp23 => "C++23",
            Self::Cpp14Gcc9 => "C++14 (GCC 9)",
            Self::Python3 => "Python 3",
            Self::PyPy3 => "PyPy 3",
            Self::Go => "Go",
            Self::Rust => "Rust",
            Self::Java8 => "Java 8",
            Self::Java21 => "Java 21",
            Self::NodeJs => "Node.js LTS",
            Self::Unknown(_) => "未知语言",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordStatus {
    Waiting,
    Judging,
    CompileError,
    OutputLimitExceeded,
    MemoryLimitExceeded,
    TimeLimitExceeded,
    WrongAnswer,
    RuntimeError,
    UnknownError,
    Accepted,
    Unaccepted,
    HackSuccess,
    HackFailure,
    HackSkipped,
    Unshown,
    Unknown(i32),
}

impl RecordStatus {
    pub fn from_id(id: i32) -> Self {
        match id {
            0 => Self::Waiting,
            1 => Self::Judging,
            2 => Self::CompileError,
            3 => Self::OutputLimitExceeded,
            4 => Self::MemoryLimitExceeded,
            5 => Self::TimeLimitExceeded,
            6 => Self::WrongAnswer,
            7 => Self::RuntimeError,
            11 => Self::UnknownError,
            12 => Self::Accepted,
            14 => Self::Unaccepted,
            21 => Self::HackSuccess,
            22 => Self::HackFailure,
            23 => Self::HackSkipped,
            -1 => Self::Unshown,
            other => Self::Unknown(other),
        }
    }

    pub fn is_pending(self) -> bool {
        matches!(self, Self::Waiting | Self::Judging)
    }

    pub fn is_finished(self) -> bool {
        !self.is_pending()
    }

    pub fn is_accepted(self) -> bool {
        matches!(self, Self::Accepted)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Waiting => "Waiting",
            Self::Judging => "Judging",
            Self::CompileError => "Compile Error",
            Self::OutputLimitExceeded => "Output Limit Exceeded",
            Self::MemoryLimitExceeded => "Memory Limit Exceeded",
            Self::TimeLimitExceeded => "Time Limit Exceeded",
            Self::WrongAnswer => "Wrong Answer",
            Self::RuntimeError => "Runtime Error",
            Self::UnknownError => "Unknown Error",
            Self::Accepted => "Accepted",
            Self::Unaccepted => "Unaccepted",
            Self::HackSuccess => "Hack Success",
            Self::HackFailure => "Hack Failure",
            Self::HackSkipped => "Hack Skipped",
            Self::Unshown => "Unshown",
            Self::Unknown(_) => "Unknown",
        }
    }
}

impl fmt::Display for Difficulty {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

impl fmt::Display for Language {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

impl fmt::Display for RecordStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}
