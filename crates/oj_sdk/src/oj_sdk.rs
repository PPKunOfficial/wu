//! A small client SDK for online judge sites, starting with Luogu.
//!
//! Luogu does not expose a stable public API, so the client reads the JSON that
//! the website embeds in its pages and calls the same endpoints the web frontend
//! uses. Sessions are cookie based: log in with an account and captcha, or
//! import a cookie string copied from a browser.

pub mod error;
pub mod luogu;
pub mod model;
pub mod session;

pub use error::{Error, Result};
pub use luogu::LuoguClient;
pub use model::*;
pub use session::Session;
