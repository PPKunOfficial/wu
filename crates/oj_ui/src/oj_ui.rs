use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use editor::Editor;
use gpui::{
    Action, AnyElement, App, AsyncWindowContext, ClipboardItem, Context, Entity, EventEmitter,
    FocusHandle, Focusable, Image, ImageFormat, InteractiveElement, IntoElement, ParentElement,
    Pixels, Render, SharedString, StatefulInteractiveElement, Styled, WeakEntity, Window, actions,
    div, img, px,
};
use markdown::{Markdown, MarkdownElement, MarkdownFont, MarkdownOptions, MarkdownStyle};
use serde_json::Value;
mod latex;
use oj_sdk::{
    Difficulty, Error, Language, LuoguClient, Problem, ProblemQuery, ProblemSummary, Record,
    RecordQuery, RecordStatus, Session, Training, TrainingSummary, User,
};
use ui::prelude::*;
use util::ResultExt as _;
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
    item::Item,
};

const OJ_PANEL_KEY: &str = "oj_panel";

actions!(
    oj_ui,
    [
        /// Toggles focus on the online judge panel.
        ToggleOjPanel,
    ]
);

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _window, _cx| {
        workspace.register_action(|workspace, _: &ToggleOjPanel, window, cx| {
            workspace.toggle_panel_focus::<OjPanel>(window, cx);
        });
    })
    .detach();
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Problems,
    Trainings,
    Records,
}

enum View {
    List,
    Training {
        training: Box<Training>,
        markdown: Option<Entity<Markdown>>,
    },
    Record {
        record: Box<Record>,
    },
    Login,
}

pub struct OjPanel {
    focus_handle: FocusHandle,
    workspace: Option<WeakEntity<Workspace>>,
    dock_position: DockPosition,
    session: Session,
    user: Option<User>,
    tab: Tab,
    view: View,
    search: Entity<Editor>,
    problems: Vec<ProblemSummary>,
    problem_count: u64,
    problem_page: u32,
    trainings: Vec<TrainingSummary>,
    training_count: u64,
    training_page: u32,
    records: Vec<Record>,
    record_count: u64,
    record_page: u32,
    loading: bool,
    error: Option<SharedString>,
    login_account: Entity<Editor>,
    login_password: Entity<Editor>,
    login_captcha_code: Entity<Editor>,
    login_cookie: Entity<Editor>,
    login_captcha: Option<Arc<Image>>,
    login_error: Option<SharedString>,
    login_loading: bool,
}

impl OjPanel {
    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> Result<Entity<Self>> {
        let workspace_handle = workspace.clone();
        workspace.update_in(&mut cx, move |_workspace, window, cx| {
            let panel = cx.new(|cx| {
                let mut panel = Self::new(window, cx);
                panel.workspace = Some(workspace_handle);
                panel
            });
            panel.update(cx, |panel, cx| panel.reload(cx));
            panel
        })
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("搜索题号、题目名或关键词", window, cx);
            editor
        });
        let login_account = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("用户名 / UID / 邮箱", window, cx);
            editor
        });
        let login_password = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("密码", window, cx);
            editor.set_masked(true, cx);
            editor
        });
        let login_captcha_code = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("图形验证码", window, cx);
            editor
        });
        let login_cookie = cx.new(|cx| {
            let mut editor = Editor::auto_height(3, 8, window, cx);
            editor.set_placeholder_text(
                "从浏览器 DevTools 复制 Cookie（需含 __client_id 与 _uid）",
                window,
                cx,
            );
            editor
        });

        Self {
            focus_handle: cx.focus_handle(),
            workspace: None,
            dock_position: DockPosition::Left,
            session: Session::new(),
            user: None,
            tab: Tab::Problems,
            view: View::List,
            search,
            problems: Vec::new(),
            problem_count: 0,
            problem_page: 1,
            trainings: Vec::new(),
            training_count: 0,
            training_page: 1,
            records: Vec::new(),
            record_count: 0,
            record_page: 1,
            loading: false,
            error: None,
            login_account,
            login_password,
            login_captcha_code,
            login_cookie,
            login_captcha: None,
            login_error: None,
            login_loading: false,
        }
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        if self.session.uid().is_some() {
            self.refresh_user(cx);
        }
        self.load_problems(1, cx);
    }

    fn client(&self) -> oj_sdk::Result<LuoguClient> {
        if self.session.is_empty() {
            LuoguClient::anonymous()
        } else {
            LuoguClient::with_session(self.session.clone())
        }
    }

    fn client_or_error(&mut self, cx: &mut Context<Self>) -> Option<LuoguClient> {
        match self.client() {
            Ok(client) => Some(client),
            Err(error) => {
                self.error = Some(error.to_string().into());
                cx.notify();
                None
            }
        }
    }

    fn select_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        self.view = View::List;
        self.error = None;
        match tab {
            Tab::Problems => {
                if self.problems.is_empty() {
                    self.load_problems(1, cx);
                }
            }
            Tab::Trainings => {
                if self.trainings.is_empty() {
                    self.load_trainings(1, cx);
                }
            }
            Tab::Records => {
                if self.user.is_none() {
                    self.view = View::Login;
                    self.login_error = None;
                } else if self.records.is_empty() {
                    self.load_records(1, cx);
                }
            }
        }
        cx.notify();
    }

    fn search_problems(&mut self, cx: &mut Context<Self>) {
        self.tab = Tab::Problems;
        self.view = View::List;
        self.load_problems(1, cx);
    }

    fn load_problems(&mut self, page: u32, cx: &mut Context<Self>) {
        let keyword = {
            let text = self.search.read(cx).text(cx);
            let text = text.trim().to_string();
            (!text.is_empty()).then_some(text)
        };
        let problem_type = if keyword.is_some() { "all" } else { "P" };
        let query = ProblemQuery {
            page: Some(page),
            problem_type: Some(problem_type.to_string()),
            keyword,
            difficulty: None,
            tag: None,
        };
        let Some(mut client) = self.client_or_error(cx) else {
            return;
        };
        let task = gpui_tokio::Tokio::spawn_result(cx, async move {
            let result = client.list_problems(&query).await;
            anyhow::Ok((client.session().clone(), result))
        });

        self.loading = true;
        self.error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                this.loading = false;
                match outcome {
                    Ok((session, Ok(page_data))) => {
                        this.session = session;
                        this.problems = page_data.result;
                        this.problem_count = page_data.count;
                        this.problem_page = page;
                    }
                    Ok((session, Err(error))) => {
                        this.session = session;
                        this.error = Some(error.to_string().into());
                    }
                    Err(error) => {
                        this.error = Some(error.to_string().into());
                    }
                }
                cx.notify();
            })
            .log_err();
        })
        .detach();
    }

    fn load_trainings(&mut self, page: u32, cx: &mut Context<Self>) {
        let Some(mut client) = self.client_or_error(cx) else {
            return;
        };
        let task = gpui_tokio::Tokio::spawn_result(cx, async move {
            let result = client.list_trainings(page).await;
            anyhow::Ok(result)
        });

        self.loading = true;
        self.error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                this.loading = false;
                match outcome {
                    Ok(Ok(page_data)) => {
                        this.trainings = page_data.result;
                        this.training_count = page_data.count;
                        this.training_page = page;
                    }
                    Ok(Err(error)) => this.error = Some(error.to_string().into()),
                    Err(error) => this.error = Some(error.to_string().into()),
                }
                cx.notify();
            })
            .log_err();
        })
        .detach();
    }

    fn load_records(&mut self, page: u32, cx: &mut Context<Self>) {
        let Some(user) = self.user.as_ref().map(|user| user.uid) else {
            self.view = View::Login;
            self.login_error = Some("请先登录后查看提交记录".into());
            cx.notify();
            return;
        };
        let query = RecordQuery {
            page: Some(page),
            pid: None,
            user: Some(user),
        };
        let Some(mut client) = self.client_or_error(cx) else {
            return;
        };
        let task = gpui_tokio::Tokio::spawn_result(cx, async move {
            let result = client.record_list(&query).await;
            anyhow::Ok((client.session().clone(), result))
        });

        self.loading = true;
        self.error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                this.loading = false;
                match outcome {
                    Ok((session, Ok(page_data))) => {
                        this.session = session;
                        this.records = page_data.result;
                        this.record_count = page_data.count;
                        this.record_page = page;
                    }
                    Ok((session, Err(error))) => {
                        this.session = session;
                        this.error = Some(error.to_string().into());
                    }
                    Err(error) => {
                        this.error = Some(error.to_string().into());
                    }
                }
                cx.notify();
            })
            .log_err();
        })
        .detach();
    }

    fn load_tab_page(&mut self, tab: Tab, page: u32, cx: &mut Context<Self>) {
        match tab {
            Tab::Problems => self.load_problems(page, cx),
            Tab::Trainings => self.load_trainings(page, cx),
            Tab::Records => self.load_records(page, cx),
        }
    }

    fn refresh_user(&mut self, cx: &mut Context<Self>) {
        if self.session.is_empty() {
            return;
        }
        let Some(mut client) = self.client_or_error(cx) else {
            return;
        };
        let task = gpui_tokio::Tokio::spawn_result(cx, async move {
            let result = client.whoami().await;
            anyhow::Ok((client.session().clone(), result))
        });

        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                match outcome {
                    Ok((session, Ok(user))) => {
                        this.session = session;
                        this.user = Some(user);
                    }
                    Ok((session, Err(_))) => {
                        this.session = session;
                        this.user = None;
                    }
                    Err(_) => {
                        this.user = None;
                    }
                }
                cx.notify();
            })
            .log_err();
        })
        .detach();
    }

    fn submit_current_file(&mut self, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace.clone() else {
            self.error = Some("编辑器尚未准备好".into());
            cx.notify();
            return;
        };
        let active = workspace.update(cx, |workspace, cx| {
            let editor = workspace.active_item_as::<editor::Editor>(cx)?;
            let text = editor.read(cx).text(cx);
            let file_name = workspace
                .active_item(cx)
                .and_then(|item| item.project_path(cx))
                .and_then(|project_path| project_path.path.file_name().map(str::to_string));
            Some((text, file_name))
        });
        let (text, file_name) = match active {
            Ok(Some(active)) => active,
            _ => {
                self.error = Some("没有打开的可提交文件".into());
                cx.notify();
                return;
            }
        };
        let Some(file_name) = file_name else {
            self.error = Some("当前标签没有文件名，无法识别题号".into());
            cx.notify();
            return;
        };
        let Some(pid) = problem_id_from_file_name(&file_name) else {
            self.error = Some(format!("无法从文件名 {file_name} 识别题号").into());
            cx.notify();
            return;
        };
        self.error = None;
        self.open_problem(pid, Some(text), cx);
    }

    fn open_problem(&mut self, pid: String, prefill: Option<String>, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        let Some(mut client) = self.client_or_error(cx) else {
            return;
        };
        let task = gpui_tokio::Tokio::spawn_result(cx, async move {
            let result = client.get_problem(&pid).await;
            anyhow::Ok((client.session().clone(), result))
        });

        self.loading = true;
        self.error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update_in(cx, |this, window, cx| {
                this.loading = false;
                match outcome {
                    Ok((session, Ok(problem))) => {
                        this.session = session;
                        let statement = problem_statement(&problem);
                        let statement =
                            latex::preprocess_math(&statement, cx.theme().colors().text);
                        let markdown = cx.new(|cx| {
                            Markdown::new_with_options(
                                statement.into(),
                                None,
                                None,
                                markdown_options(),
                                cx,
                            )
                        });
                        let problem_pid = problem.pid.clone();
                        let existing_prefill = prefill.clone();
                        let view = cx.new(|cx| {
                            OjProblemView::new(
                                problem.clone(),
                                markdown.clone(),
                                this.session.clone(),
                                prefill,
                                window,
                                cx,
                            )
                        });
                        if let Some(workspace) = this.workspace.clone() {
                            workspace
                                .update(cx, |workspace, cx| {
                                    let pane = workspace.active_pane().clone();
                                    pane.update(cx, |pane, cx| {
                                        let existing = pane
                                            .items_of_type::<OjProblemView>()
                                            .find(|view| view.read(cx).problem.pid == problem_pid);
                                        if let Some(existing) = existing {
                                            if let Some(index) = pane.index_for_item(&existing) {
                                                existing.update(cx, |view, cx| {
                                                    view.open_submit(existing_prefill, window, cx)
                                                });
                                                pane.activate_item(index, true, true, window, cx);
                                                return;
                                            }
                                        }
                                        pane.add_item(
                                            Box::new(view.clone()),
                                            true,
                                            true,
                                            None,
                                            window,
                                            cx,
                                        );
                                    });
                                })
                                .log_err();
                        }
                        this.view = View::List;
                    }
                    Ok((session, Err(error))) => {
                        this.session = session;
                        this.error = Some(error.to_string().into());
                    }
                    Err(error) => {
                        this.error = Some(error.to_string().into());
                    }
                }
                cx.notify();
            })
            .log_err();
        })
        .detach();
    }

    fn open_training(&mut self, training_id: u64, cx: &mut Context<Self>) {
        let Some(mut client) = self.client_or_error(cx) else {
            return;
        };
        let task = gpui_tokio::Tokio::spawn_result(cx, async move {
            let result = client.get_training(training_id).await;
            anyhow::Ok((client.session().clone(), result))
        });

        self.loading = true;
        self.error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                this.loading = false;
                match outcome {
                    Ok((session, Ok(training))) => {
                        this.session = session;
                        let description = training
                            .description
                            .clone()
                            .unwrap_or_else(|| "暂无题单说明。".to_string());
                        let description =
                            latex::preprocess_math(&description, cx.theme().colors().text);
                        let markdown = cx.new(|cx| {
                            Markdown::new_with_options(
                                description.into(),
                                None,
                                None,
                                markdown_options(),
                                cx,
                            )
                        });
                        this.view = View::Training {
                            training: Box::new(training),
                            markdown: Some(markdown),
                        };
                    }
                    Ok((session, Err(error))) => {
                        this.session = session;
                        this.error = Some(error.to_string().into());
                    }
                    Err(error) => {
                        this.error = Some(error.to_string().into());
                    }
                }
                cx.notify();
            })
            .log_err();
        })
        .detach();
    }

    fn open_record(&mut self, record_id: u64, cx: &mut Context<Self>) {
        let Some(mut client) = self.client_or_error(cx) else {
            return;
        };
        let task = gpui_tokio::Tokio::spawn_result(cx, async move {
            let result = client.get_record(record_id).await;
            anyhow::Ok((client.session().clone(), result))
        });

        self.loading = true;
        self.error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                this.loading = false;
                match outcome {
                    Ok((session, Ok(record))) => {
                        this.session = session;
                        this.view = View::Record {
                            record: Box::new(record),
                        };
                    }
                    Ok((session, Err(error))) => {
                        this.session = session;
                        this.error = Some(error.to_string().into());
                    }
                    Err(error) => {
                        this.error = Some(error.to_string().into());
                    }
                }
                cx.notify();
            })
            .log_err();
        })
        .detach();
    }

    fn fetch_login_captcha(&mut self, cx: &mut Context<Self>) {
        let Some(mut client) = self.client_or_error(cx) else {
            return;
        };
        let task = gpui_tokio::Tokio::spawn_result(cx, async move {
            let result = client.fetch_captcha().await;
            anyhow::Ok((client.session().clone(), result))
        });

        self.login_loading = true;
        self.login_error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                this.login_loading = false;
                match outcome {
                    Ok((session, Ok(captcha))) => {
                        this.session = session;
                        this.login_captcha = Some(Arc::new(Image::from_bytes(
                            ImageFormat::Jpeg,
                            captcha.image,
                        )));
                    }
                    Ok((session, Err(error))) => {
                        this.session = session;
                        this.login_error = Some(error.to_string().into());
                    }
                    Err(error) => this.login_error = Some(error.to_string().into()),
                }
                cx.notify();
            })
            .log_err();
        })
        .detach();
    }

    fn login_with_cookie(&mut self, cx: &mut Context<Self>) {
        let cookie = self.login_cookie.read(cx).text(cx);
        let session = match Session::from_cookie_string(&cookie) {
            Ok(session) => session,
            Err(error) => {
                self.login_error = Some(error.to_string().into());
                cx.notify();
                return;
            }
        };
        if session.uid().is_none() {
            self.login_error = Some("Cookie 中缺少 _uid，无法确认登录状态".into());
            cx.notify();
            return;
        }

        let mut client = match LuoguClient::with_session(session) {
            Ok(client) => client,
            Err(error) => {
                self.login_error = Some(error.to_string().into());
                cx.notify();
                return;
            }
        };
        let task = gpui_tokio::Tokio::spawn_result(cx, async move {
            let result = client.whoami().await;
            anyhow::Ok((client.session().clone(), result))
        });

        self.login_loading = true;
        self.login_error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                this.login_loading = false;
                match outcome {
                    Ok((session, Ok(user))) => {
                        this.session = session;
                        this.user = Some(user);
                        this.finish_login(cx);
                    }
                    Ok((session, Err(error))) => {
                        this.session = session;
                        this.login_error = Some(error.to_string().into());
                    }
                    Err(error) => this.login_error = Some(error.to_string().into()),
                }
                cx.notify();
            })
            .log_err();
        })
        .detach();
    }

    fn login_with_password(&mut self, cx: &mut Context<Self>) {
        let account = self.login_account.read(cx).text(cx).trim().to_string();
        let password = self.login_password.read(cx).text(cx);
        let captcha = self.login_captcha_code.read(cx).text(cx).trim().to_string();
        if account.is_empty() || password.is_empty() {
            self.login_error = Some("请输入账号和密码".into());
            cx.notify();
            return;
        }
        if captcha.is_empty() {
            self.login_error = Some("请输入图片验证码".into());
            cx.notify();
            return;
        }

        let Some(mut client) = self.client_or_error(cx) else {
            return;
        };
        let task = gpui_tokio::Tokio::spawn_result(cx, async move {
            let result = async {
                client
                    .login_with_password(&account, &password, &captcha)
                    .await?;
                client.whoami().await
            }
            .await;
            anyhow::Ok((client.session().clone(), result))
        });

        self.login_loading = true;
        self.login_error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                this.login_loading = false;
                match outcome {
                    Ok((session, Ok(user))) => {
                        this.session = session;
                        this.user = Some(user);
                        this.login_captcha = None;
                        this.finish_login(cx);
                    }
                    Ok((session, Err(error))) => {
                        this.session = session;
                        if matches!(error, Error::CaptchaRequired { .. }) {
                            this.login_captcha = None;
                            this.login_error = Some("验证码错误，请重新获取并输入".into());
                        } else {
                            this.login_error = Some(error.to_string().into());
                        }
                    }
                    Err(error) => this.login_error = Some(error.to_string().into()),
                }
                cx.notify();
            })
            .log_err();
        })
        .detach();
    }

    fn finish_login(&mut self, cx: &mut Context<Self>) {
        self.tab = Tab::Problems;
        self.view = View::List;
        self.error = None;
        self.login_error = None;
        self.load_problems(1, cx);
    }

    fn logout(&mut self, cx: &mut Context<Self>) {
        self.session = Session::new();
        self.user = None;
        self.records.clear();
        self.record_count = 0;
        self.login_captcha = None;
        self.view = View::Login;
        cx.notify();
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let borders = cx.theme().colors().border;
        v_flex()
            .flex_none()
            .border_b_1()
            .border_color(borders)
            .child(
                h_flex()
                    .px_2()
                    .py_1()
                    .gap_1()
                    .items_center()
                    .child(self.render_tab(Tab::Problems, "题库", cx))
                    .child(self.render_tab(Tab::Trainings, "题单", cx))
                    .child(self.render_tab(Tab::Records, "记录", cx))
                    .child(div().flex_1())
                    .child(
                        Button::new("oj-submit-file", "提交文件")
                            .on_click(cx.listener(|this, _, _, cx| this.submit_current_file(cx))),
                    )
                    .when_some(self.user.clone(), |this, user| {
                        this.child(
                            Label::new(user.name)
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        )
                    })
                    .child(if self.user.is_some() {
                        Button::new("oj-logout", "登出")
                            .on_click(cx.listener(|this, _, _, cx| this.logout(cx)))
                            .into_any_element()
                    } else {
                        Button::new("oj-login", "登录")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.view = View::Login;
                                this.login_error = None;
                                cx.notify();
                            }))
                            .into_any_element()
                    }),
            )
            .child(
                h_flex()
                    .px_2()
                    .pb_1()
                    .gap_1()
                    .items_center()
                    .child(div().flex_1().child(self.search.clone()))
                    .child(
                        Button::new("oj-search", "搜索")
                            .on_click(cx.listener(|this, _, _, cx| this.search_problems(cx))),
                    ),
            )
            .when_some(self.error.clone(), |this, error| {
                this.child(
                    v_flex()
                        .px_2()
                        .pb_1()
                        .child(Label::new(error).size(LabelSize::Small).color(Color::Error)),
                )
            })
            .when(self.loading, |this| {
                this.child(
                    v_flex().px_2().pb_1().child(
                        Label::new("加载中…")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
                )
            })
    }

    fn render_tab(&self, tab: Tab, label: &'static str, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.tab == tab;
        let selected_bg = cx.theme().colors().element_selected;
        let hover_bg = cx.theme().colors().element_hover;
        div()
            .id(("oj-tab", tab as usize))
            .px_2()
            .py_1()
            .rounded_md()
            .cursor_pointer()
            .when(selected, |this| this.bg(selected_bg))
            .hover(|this| this.bg(hover_bg))
            .on_click(cx.listener(move |this, _, _, cx| this.select_tab(tab, cx)))
            .child(Label::new(label).size(LabelSize::Small).color(if selected {
                Color::Default
            } else {
                Color::Muted
            }))
            .into_any_element()
    }

    fn render_content(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match &self.view {
            View::List => match self.tab {
                Tab::Problems => self.render_problem_list(cx).into_any_element(),
                Tab::Trainings => self.render_training_list(cx).into_any_element(),
                Tab::Records => self.render_record_list(cx).into_any_element(),
            },
            View::Training { training, markdown } => self
                .render_training(training, markdown.as_ref(), window, cx)
                .into_any_element(),
            View::Record { record } => self.render_record(record, cx).into_any_element(),
            View::Login => self.render_login(cx).into_any_element(),
        }
    }

    fn render_problem_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let hover_bg = cx.theme().colors().element_hover;
        v_flex()
            .flex_1()
            .child(
                v_flex()
                    .flex_1()
                    .id("oj-problems-scroll")
                    .overflow_y_scroll()
                    .children(self.problems.iter().enumerate().map(|(index, problem)| {
                        let pid = problem.pid.clone();
                        let difficulty = problem.difficulty();
                        h_flex()
                            .id(("oj-problem", index))
                            .px_2()
                            .py_1()
                            .gap_2()
                            .items_center()
                            .cursor_pointer()
                            .hover(|this| this.bg(hover_bg))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_problem(pid.clone(), None, cx)
                            }))
                            .child(
                                Label::new(problem.pid.clone())
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            )
                            .child(Label::new(problem.name.clone()).size(LabelSize::Small))
                            .child(div().flex_1())
                            .child(
                                Label::new(difficulty.label())
                                    .size(LabelSize::XSmall)
                                    .color(difficulty_color(difficulty)),
                            )
                    })),
            )
            .child(self.render_pager(Tab::Problems, self.problem_page, self.problem_count, 50, cx))
            .into_any_element()
    }

    fn render_training_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let hover_bg = cx.theme().colors().element_hover;
        v_flex()
            .flex_1()
            .child(
                v_flex()
                    .flex_1()
                    .id("oj-trainings-scroll")
                    .overflow_y_scroll()
                    .children(self.trainings.iter().enumerate().map(|(index, training)| {
                        let training_id = training.id;
                        h_flex()
                            .id(("oj-training", index))
                            .px_2()
                            .py_1()
                            .gap_2()
                            .items_center()
                            .cursor_pointer()
                            .hover(|this| this.bg(hover_bg))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_training(training_id, cx)
                            }))
                            .child(Label::new(training.name.clone()).size(LabelSize::Small))
                            .child(div().flex_1())
                            .child(
                                Label::new(format!("{} 题", training.problem_count))
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted),
                            )
                    })),
            )
            .child(self.render_pager(
                Tab::Trainings,
                self.training_page,
                self.training_count,
                18,
                cx,
            ))
            .into_any_element()
    }

    fn render_record_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let hover_bg = cx.theme().colors().element_hover;
        if self.view_is_login() {
            return self.render_login(cx).into_any_element();
        }
        v_flex()
            .flex_1()
            .child(
                v_flex()
                    .flex_1()
                    .id("oj-records-scroll")
                    .overflow_y_scroll()
                    .children(self.records.iter().enumerate().map(|(index, record)| {
                        let record_id = record.id;
                        let status = record.status();
                        h_flex()
                            .id(("oj-record", index))
                            .px_2()
                            .py_1()
                            .gap_2()
                            .items_center()
                            .cursor_pointer()
                            .hover(|this| this.bg(hover_bg))
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.open_record(record_id, cx)),
                            )
                            .child(
                                Label::new(format!("#{}", record.id))
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted),
                            )
                            .child(
                                Label::new(status.label())
                                    .size(LabelSize::Small)
                                    .color(record_status_color(status)),
                            )
                            .child(
                                Label::new(
                                    record
                                        .problem
                                        .as_ref()
                                        .map(|problem| problem.pid.clone())
                                        .unwrap_or_default(),
                                )
                                .size(LabelSize::XSmall),
                            )
                            .child(div().flex_1())
                            .child(
                                Label::new(format!("{} ms", record.time))
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted),
                            )
                    })),
            )
            .child(self.render_pager(Tab::Records, self.record_page, self.record_count, 20, cx))
            .into_any_element()
    }

    fn view_is_login(&self) -> bool {
        matches!(self.view, View::Login)
    }

    fn render_pager(
        &self,
        tab: Tab,
        page: u32,
        total: u64,
        per_page: u32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let max_page = if per_page == 0 {
            1
        } else {
            total.div_ceil(per_page as u64).max(1) as u32
        };
        h_flex()
            .flex_none()
            .px_2()
            .py_1()
            .gap_2()
            .items_center()
            .border_t_1()
            .border_color(cx.theme().colors().border)
            .child(
                Label::new(format!("第 {page} / {max_page} 页"))
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
            .child(div().flex_1())
            .child(
                Button::new(("oj-prev", tab as usize), "上一页")
                    .disabled(page <= 1)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.load_tab_page(tab, page.saturating_sub(1), cx)
                    })),
            )
            .child(
                Button::new(("oj-next", tab as usize), "下一页")
                    .disabled(page >= max_page)
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.load_tab_page(tab, page + 1, cx)),
                    ),
            )
    }

    fn render_training(
        &self,
        training: &Training,
        markdown: Option<&Entity<Markdown>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let borders = cx.theme().colors().border;
        let hover_bg = cx.theme().colors().element_hover;
        let style = MarkdownStyle::themed(MarkdownFont::Preview, window, cx);
        v_flex()
            .flex_1()
            .child(
                v_flex()
                    .flex_none()
                    .px_3()
                    .py_2()
                    .gap_1()
                    .border_b_1()
                    .border_color(borders)
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                Label::new(format!("#{}", training.id()))
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            )
                            .child(div().flex_1())
                            .child(
                                Button::new("oj-training-back", "返回").on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.view = View::List;
                                        cx.notify();
                                    },
                                )),
                            ),
                    )
                    .child(Label::new(training.name().to_string()).size(LabelSize::Large))
                    .child(
                        Label::new(format!(
                            "{} 道题{}",
                            training.problems.len(),
                            training
                                .summary
                                .provider_name()
                                .map(|provider| format!(" · {provider}"))
                                .unwrap_or_default()
                        ))
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .id("oj-training-scroll")
                    .overflow_y_scroll()
                    .px_3()
                    .py_2()
                    .when_some(markdown, |this, markdown| {
                        this.child(MarkdownElement::new(markdown.clone(), style))
                    })
                    .children(
                        training
                            .problems
                            .iter()
                            .enumerate()
                            .map(|(index, problem)| {
                                let pid = problem.pid.clone();
                                h_flex()
                                    .id(("oj-training-problem", index))
                                    .px_2()
                                    .py_1()
                                    .gap_2()
                                    .items_center()
                                    .cursor_pointer()
                                    .hover(|this| this.bg(hover_bg))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.open_problem(pid.clone(), None, cx)
                                    }))
                                    .child(
                                        Label::new(problem.pid.clone())
                                            .size(LabelSize::Small)
                                            .color(Color::Muted),
                                    )
                                    .child(Label::new(problem.name.clone()).size(LabelSize::Small))
                            }),
                    ),
            )
    }

    fn render_record(&self, record: &Record, cx: &mut Context<Self>) -> impl IntoElement {
        let status = record.status();
        v_flex()
            .flex_1()
            .p_3()
            .gap_2()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(Label::new(format!("记录 #{}", record.id)).size(LabelSize::Large))
                    .child(
                        Label::new(status.label())
                            .size(LabelSize::Default)
                            .color(record_status_color(status)),
                    )
                    .child(div().flex_1())
                    .child(Button::new("oj-record-back", "返回").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.view = View::List;
                            cx.notify();
                        },
                    ))),
            )
            .child(
                Label::new(format!(
                    "得分 {} · 时间 {} ms · 内存 {} KB · 语言 {}",
                    record.score,
                    record.time,
                    record.memory,
                    Language::from_id(record.language).label()
                ))
                .size(LabelSize::Small),
            )
            .when_some(record.problem.clone(), |this, problem| {
                this.child(
                    Label::new(format!("题目：{} {}", problem.pid, problem.name))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
            })
            .when_some(record.source_code.clone(), |this, code| {
                let code_for_copy = code.clone();
                this.child(
                    v_flex()
                        .gap_1()
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(
                                    Label::new("源码")
                                        .size(LabelSize::Small)
                                        .color(Color::Muted),
                                )
                                .child(div().flex_1())
                                .child(Button::new("oj-copy-code", "复制源码").on_click(
                                    cx.listener(move |_, _, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            code_for_copy.clone(),
                                        ));
                                    }),
                                )),
                        )
                        .child(Label::new(truncate_source(&code)).size(LabelSize::XSmall)),
                )
            })
    }

    fn render_login(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let borders = cx.theme().colors().border;
        let muted = Color::Muted;
        v_flex()
            .flex_1()
            .id("oj-login-scroll")
            .overflow_y_scroll()
            .p_3()
            .gap_2()
            .child(Label::new("登录洛谷").size(LabelSize::Large))
            .child(
                Label::new("方式一：从浏览器 DevTools → Application → Cookies 复制 Cookie")
                    .size(LabelSize::XSmall)
                    .color(muted),
            )
            .child(self.login_cookie.clone())
            .child(
                Button::new("oj-login-cookie", "使用 Cookie 登录")
                    .disabled(self.login_loading)
                    .on_click(cx.listener(|this, _, _, cx| this.login_with_cookie(cx))),
            )
            .child(div().h(px(1.)).my_1().bg(borders))
            .child(
                Label::new("方式二：账号密码（验证码会显示在下方，填好后登录）")
                    .size(LabelSize::XSmall)
                    .color(muted),
            )
            .child(self.login_account.clone())
            .child(self.login_password.clone())
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(div().flex_1().child(self.login_captcha_code.clone()))
                    .child(
                        Button::new("oj-login-captcha", "获取验证码")
                            .disabled(self.login_loading)
                            .on_click(cx.listener(|this, _, _, cx| this.fetch_login_captcha(cx))),
                    )
                    .when_some(self.login_captcha.clone(), |this, image| {
                        this.child(img(image).w(px(90.)).h(px(35.)))
                    }),
            )
            .child(
                Button::new("oj-login-password", "登录")
                    .disabled(self.login_loading)
                    .on_click(cx.listener(|this, _, _, cx| this.login_with_password(cx))),
            )
            .when_some(self.login_error.clone(), |this, error| {
                this.child(Label::new(error).size(LabelSize::Small).color(Color::Error))
            })
    }
}

impl Focusable for OjPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for OjPanel {}

impl Panel for OjPanel {
    fn persistent_name() -> &'static str {
        "Online Judge"
    }

    fn panel_key() -> &'static str {
        OJ_PANEL_KEY
    }

    fn activation_focus_handle(&self, cx: &App) -> FocusHandle {
        self.search.focus_handle(cx)
    }

    fn position(&self, _window: &Window, _cx: &App) -> DockPosition {
        self.dock_position
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        matches!(position, DockPosition::Left | DockPosition::Right)
    }

    fn set_position(
        &mut self,
        position: DockPosition,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dock_position = position;
        cx.notify();
    }

    fn default_size(&self, _window: &Window, _cx: &App) -> Pixels {
        px(420.)
    }

    fn min_size(&self, _window: &Window, _cx: &App) -> Option<Pixels> {
        Some(px(320.))
    }

    fn icon(&self, _window: &Window, _cx: &App) -> Option<IconName> {
        Some(IconName::Book)
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("Online Judge")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleOjPanel)
    }

    fn activation_priority(&self) -> u32 {
        5
    }
}

impl Render for OjPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .bg(cx.theme().colors().panel_background)
            .track_focus(&self.focus_handle)
            .child(self.render_header(cx))
            .child(self.render_content(window, cx))
    }
}

fn difficulty_color(difficulty: Difficulty) -> Color {
    match difficulty {
        Difficulty::Unrated | Difficulty::Unknown(_) | Difficulty::Introductory => Color::Muted,
        Difficulty::PopularMinus | Difficulty::Popular => Color::Success,
        Difficulty::PopularPlus | Difficulty::Advanced => Color::Warning,
        Difficulty::AdvancedPlus | Difficulty::Provincial | Difficulty::National => Color::Error,
    }
}

fn record_status_color(status: RecordStatus) -> Color {
    match status {
        RecordStatus::Accepted => Color::Success,
        RecordStatus::CompileError
        | RecordStatus::WrongAnswer
        | RecordStatus::RuntimeError
        | RecordStatus::MemoryLimitExceeded
        | RecordStatus::TimeLimitExceeded
        | RecordStatus::OutputLimitExceeded => Color::Error,
        RecordStatus::Unaccepted | RecordStatus::UnknownError => Color::Warning,
        RecordStatus::Waiting | RecordStatus::Judging | RecordStatus::Unshown => Color::Muted,
        RecordStatus::HackSuccess => Color::Success,
        RecordStatus::HackFailure | RecordStatus::HackSkipped => Color::Warning,
        RecordStatus::Unknown(_) => Color::Muted,
    }
}

fn markdown_options() -> MarkdownOptions {
    MarkdownOptions {
        parse_block_quotes: true,
        parse_html: true,
        parse_heading_slugs: true,
        render_metadata_blocks: true,
        ..Default::default()
    }
}

fn problem_statement(problem: &Problem) -> String {
    let mut markdown = String::new();
    if let Some(content) = &problem.contenu {
        for (title, text) in [
            ("题目背景", content.background.as_deref()),
            ("题目描述", content.description.as_deref()),
            ("输入格式", content.input_format.as_deref()),
            ("输出格式", content.output_format.as_deref()),
        ] {
            if let Some(text) = text {
                if !text.trim().is_empty() {
                    markdown.push_str(&format!("## {title}\n\n{text}\n\n"));
                }
            }
        }
        if let Some(hint) = &content.hint {
            if !hint.trim().is_empty() {
                markdown.push_str(&format!("## 提示\n\n{hint}\n\n"));
            }
        }
    } else if !problem.description().is_empty() {
        markdown.push_str(problem.description());
    }
    if markdown.trim().is_empty() {
        markdown.push_str("暂无题面。");
    }
    markdown
}

fn samples_text(problem: &Problem) -> String {
    let mut text = String::new();
    for (index, sample) in problem.samples.iter().enumerate() {
        text.push_str(&format!(
            "样例 {}\n输入:\n{}\n输出:\n{}\n\n",
            index + 1,
            sample.input.trim_end(),
            sample.output.trim_end()
        ));
    }
    text
}

/// Extracts a Luogu problem id from a file name such as `P1001.cpp` or
/// `CF1A.rs`, so submitting an opened file targets the problem it belongs to.
fn problem_id_from_file_name(file_name: &str) -> Option<String> {
    let stem = file_name
        .rsplit_once('.')
        .map(|(stem, _extension)| stem)
        .unwrap_or(file_name);
    let bytes = stem.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if !bytes[index].is_ascii_alphanumeric() {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && bytes[index].is_ascii_alphabetic() {
            index += 1;
        }
        let prefix = &stem[start..index];
        let digits_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        let digits = &stem[digits_start..index];
        if digits.is_empty() || digits.len() > 7 {
            continue;
        }
        // Codeforces-style ids have a trailing problem letter, e.g. CF1A.
        let trailing_start = index;
        while index < bytes.len()
            && bytes[index].is_ascii_alphabetic()
            && index - trailing_start < 2
        {
            index += 1;
        }
        let prefix_upper = if prefix.is_empty() {
            "P".to_string()
        } else {
            prefix.to_ascii_uppercase()
        };
        let known_prefix = matches!(
            prefix_upper.as_str(),
            "" | "P" | "B" | "T" | "U" | "CF" | "SP" | "AT" | "UVA"
        );
        if known_prefix && prefix.len() <= 4 {
            return Some(format!("{}{}", prefix_upper, &stem[digits_start..index]));
        }
    }
    None
}

fn truncate_source(source: &str) -> String {
    const LIMIT: usize = 4000;
    if source.chars().count() <= LIMIT {
        source.to_string()
    } else {
        let mut truncated: String = source.chars().take(LIMIT).collect();
        truncated.push_str("\n…");
        truncated
    }
}

pub struct OjProblemView {
    focus_handle: FocusHandle,
    problem: Problem,
    markdown: Entity<Markdown>,
    session: Session,
    code_editor: Entity<Editor>,
    language: Language,
    show_submit: bool,
    submit_pid: String,
    submitting: bool,
    waiting_for_record: bool,
    rid: Option<u64>,
    record: Option<Record>,
    submit_error: Option<SharedString>,
    captcha_image: Option<Arc<Image>>,
    captcha_input: Entity<Editor>,
}

impl OjProblemView {
    fn new(
        problem: Problem,
        markdown: Entity<Markdown>,
        session: Session,
        prefill: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let show_submit = prefill.is_some();
        let code_editor = cx.new(|cx| {
            let mut editor = Editor::multi_line(window, cx);
            editor.set_placeholder_text("在此编写代码，完成后点提交", window, cx);
            if let Some(code) = prefill {
                editor.set_text(code, window, cx);
            }
            editor
        });
        let captcha_input = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("图片验证码", window, cx);
            editor
        });
        let submit_pid = problem.pid.clone();
        Self {
            focus_handle: cx.focus_handle(),
            problem,
            markdown,
            session,
            code_editor,
            language: Language::Cpp17,
            show_submit,
            submit_pid,
            submitting: false,
            waiting_for_record: false,
            rid: None,
            record: None,
            submit_error: None,
            captcha_image: None,
            captcha_input,
        }
    }

    fn open_submit(&mut self, code: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.show_submit = true;
        if let Some(code) = code {
            self.code_editor
                .update(cx, |editor, cx| editor.set_text(code, window, cx));
        }
        cx.notify();
    }

    fn render_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors();
        let difficulty = self.problem.difficulty();
        v_flex()
            .px_6()
            .pt_4()
            .pb_2()
            .gap_1()
            .border_b_1()
            .border_color(colors.border)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Label::new(self.problem.pid.clone())
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .child(Label::new(self.problem.name.clone()).size(LabelSize::Large))
                    .child(div().flex_1())
                    .child(
                        Button::new(
                            "oj-toggle-submit",
                            if self.show_submit {
                                "收起提交"
                            } else {
                                "提交"
                            },
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_submit = !this.show_submit;
                            cx.notify();
                        })),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Label::new(difficulty.label())
                            .size(LabelSize::Small)
                            .color(difficulty_color(difficulty)),
                    )
                    .child(
                        Label::new(format!(
                            "提交 {} · 通过 {}",
                            self.problem.total_submit, self.problem.total_accepted
                        ))
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                    ),
            )
            .into_any_element()
    }

    fn render_samples(&self, cx: &mut Context<Self>) -> AnyElement {
        let borders = cx.theme().colors().border;
        let background = cx.theme().colors().element_background;
        let all_samples = samples_text(&self.problem);
        v_flex()
            .mt_4()
            .pt_2()
            .w_full()
            .border_t_1()
            .border_color(borders)
            .child(
                h_flex()
                    .px_2()
                    .py_1()
                    .gap_2()
                    .items_center()
                    .child(
                        Label::new(format!("样例数据（{}）", self.problem.samples.len()))
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("oj-view-copy-all-samples", "复制全部").on_click(cx.listener(
                            move |_, _, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    all_samples.clone(),
                                ));
                            },
                        )),
                    ),
            )
            .children(
                self.problem
                    .samples
                    .iter()
                    .enumerate()
                    .map(|(index, sample)| {
                        let input = sample.input.clone();
                        let output = sample.output.clone();
                        v_flex()
                            .mx_2()
                            .mb_1()
                            .p_2()
                            .gap_1()
                            .rounded_md()
                            .bg(background)
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        Label::new(format!("样例 {}", index + 1))
                                            .size(LabelSize::XSmall)
                                            .color(Color::Muted),
                                    )
                                    .child(div().flex_1())
                                    .child(
                                        Button::new(("oj-view-copy-input", index), "复制输入")
                                            .on_click(cx.listener(move |_, _, _, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    input.clone(),
                                                ));
                                            })),
                                    )
                                    .child(
                                        Button::new(("oj-view-copy-output", index), "复制输出")
                                            .on_click(cx.listener(move |_, _, _, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    output.clone(),
                                                ));
                                            })),
                                    ),
                            )
                            .child(
                                Label::new(format!(
                                    "输入:\n{}\n输出:\n{}",
                                    sample.input.trim_end(),
                                    sample.output.trim_end()
                                ))
                                .size(LabelSize::Small),
                            )
                    }),
            )
            .into_any_element()
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let code = self.code_editor.read(cx).text(cx);
        if code.trim().is_empty() {
            self.submit_error = Some("代码不能为空".into());
            cx.notify();
            return;
        }
        self.submit_code(code, None, cx);
    }

    fn submit_with_captcha(&mut self, cx: &mut Context<Self>) {
        let captcha = self.captcha_input.read(cx).text(cx).trim().to_string();
        if captcha.is_empty() {
            self.submit_error = Some("请输入图片验证码".into());
            cx.notify();
            return;
        }
        let code = self.code_editor.read(cx).text(cx);
        self.submit_code(code, Some(captcha), cx);
    }

    fn submit_code(&mut self, code: String, captcha: Option<String>, cx: &mut Context<Self>) {
        if self.submitting || self.waiting_for_record {
            return;
        }
        if self.session.uid().is_none() {
            self.submit_error = Some("请先在左侧面板登录".into());
            cx.notify();
            return;
        }
        let pid = self.submit_pid.clone();
        let language = self.language;
        let session = self.session.clone();
        self.submitting = true;
        self.submit_error = None;
        self.captcha_image = None;
        self.record = None;
        self.rid = None;
        cx.notify();

        let task = gpui_tokio::Tokio::spawn_result(cx, async move {
            let mut client = LuoguClient::with_session(session)?;
            let result = client
                .submit_with_captcha(&pid, &code, language, false, None, captcha.as_deref())
                .await;
            anyhow::Ok((client.session().clone(), result))
        });

        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                this.submitting = false;
                match outcome {
                    Ok((session, Ok(rid))) => {
                        this.session = session;
                        this.rid = Some(rid);
                        this.waiting_for_record = true;
                        this.poll_record(cx);
                    }
                    Ok((session, Err(error))) => {
                        this.session = session;
                        if matches!(error, Error::CaptchaRequired { .. }) {
                            this.fetch_submit_captcha(cx);
                        } else {
                            this.submit_error = Some(error.to_string().into());
                        }
                    }
                    Err(error) => this.submit_error = Some(error.to_string().into()),
                }
                cx.notify();
            })
            .log_err();
        })
        .detach();
    }

    fn poll_record(&mut self, cx: &mut Context<Self>) {
        let Some(rid) = self.rid else {
            return;
        };
        let session = self.session.clone();
        let task = gpui_tokio::Tokio::spawn_result(cx, async move {
            let mut client = LuoguClient::with_session(session)?;
            let result = client.wait_for_record(rid, Duration::from_secs(120)).await;
            anyhow::Ok((client.session().clone(), result))
        });

        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                this.waiting_for_record = false;
                match outcome {
                    Ok((session, Ok(record))) => {
                        this.session = session;
                        this.record = Some(record);
                    }
                    Ok((session, Err(error))) => {
                        this.session = session;
                        this.submit_error = Some(error.to_string().into());
                    }
                    Err(error) => this.submit_error = Some(error.to_string().into()),
                }
                cx.notify();
            })
            .log_err();
        })
        .detach();
    }

    fn fetch_submit_captcha(&mut self, cx: &mut Context<Self>) {
        let session = self.session.clone();
        let task = gpui_tokio::Tokio::spawn_result(cx, async move {
            let mut client = LuoguClient::with_session(session)?;
            let result = client.fetch_submit_captcha().await;
            anyhow::Ok((client.session().clone(), result))
        });

        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                match outcome {
                    Ok((session, Ok(captcha))) => {
                        this.session = session;
                        this.captcha_image = Some(Arc::new(Image::from_bytes(
                            ImageFormat::Jpeg,
                            captcha.image,
                        )));
                    }
                    Ok((session, Err(error))) => {
                        this.session = session;
                        this.submit_error = Some(error.to_string().into());
                    }
                    Err(error) => this.submit_error = Some(error.to_string().into()),
                }
                cx.notify();
            })
            .log_err();
        })
        .detach();
    }

    fn render_submit(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors();
        let logged_in = self.session.uid().is_some();
        let languages = [
            (Language::Cpp17, "C++17"),
            (Language::Cpp20, "C++20"),
            (Language::Cpp14, "C++14"),
            (Language::C, "C"),
            (Language::Python3, "Python3"),
            (Language::PyPy3, "PyPy3"),
            (Language::Rust, "Rust"),
            (Language::Go, "Go"),
            (Language::Java21, "Java21"),
        ];
        let selected_language = self.language;
        let language_buttons = languages.into_iter().map(|(language, label)| {
            Button::new(("oj-language", language.id()), label)
                .color(if language == selected_language {
                    Color::Accent
                } else {
                    Color::Default
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.language = language;
                    cx.notify();
                }))
        });

        v_flex()
            .px_6()
            .py_3()
            .gap_2()
            .border_b_1()
            .border_color(colors.border)
            .bg(colors.element_background)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Label::new("提交代码")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        Label::new(format!("目标题目 {}", self.submit_pid))
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    )
                    .child(div().flex_1())
                    .child(Button::new("oj-submit-close", "收起").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.show_submit = false;
                            cx.notify();
                        },
                    )))
                    .when(!logged_in, |this| {
                        this.child(
                            Label::new("请先在左侧面板登录")
                                .size(LabelSize::Small)
                                .color(Color::Warning),
                        )
                    }),
            )
            .child(
                div()
                    .h(px(200.))
                    .w_full()
                    .p_1()
                    .border_1()
                    .border_color(colors.border)
                    .rounded_md()
                    .child(self.code_editor.clone()),
            )
            .child(h_flex().gap_1().flex_wrap().children(language_buttons))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new("oj-submit", "提交")
                            .loading(self.submitting)
                            .disabled(!logged_in || self.submitting || self.waiting_for_record)
                            .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                    )
                    .when(self.waiting_for_record, |this| {
                        this.child(
                            Label::new("评测中…")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        )
                    })
                    .when_some(self.rid, |this, rid| {
                        this.child(
                            Label::new(format!("记录 #{rid}"))
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        )
                    }),
            )
            .when_some(self.captcha_image.clone(), |this, image| {
                this.child(
                    v_flex()
                        .gap_1()
                        .child(
                            Label::new("提交需要图片验证码")
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        )
                        .child(img(image).max_h(px(60.)).max_w(px(240.)))
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(
                                    div()
                                        .w(px(160.))
                                        .h(px(30.))
                                        .p_1()
                                        .border_1()
                                        .border_color(colors.border)
                                        .rounded_md()
                                        .child(self.captcha_input.clone()),
                                )
                                .child(Button::new("oj-submit-captcha", "带验证码提交").on_click(
                                    cx.listener(|this, _, _, cx| this.submit_with_captcha(cx)),
                                )),
                        ),
                )
            })
            .when_some(self.submit_error.clone(), |this, error| {
                this.child(Label::new(error).size(LabelSize::Small).color(Color::Error))
            })
            .when_some(self.record.clone(), |this, record| {
                this.child(Self::render_verdict(&record, cx))
            })
            .into_any_element()
    }

    fn render_verdict(record: &Record, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors();
        let status = record.status();
        let color = if status.is_accepted() {
            Color::Success
        } else if status.is_pending() {
            Color::Muted
        } else {
            Color::Error
        };
        let compile_message = record
            .detail
            .as_ref()
            .and_then(|detail| detail.get("compileResult"))
            .filter(|compile| compile.get("success").and_then(Value::as_bool) == Some(false))
            .and_then(|compile| compile.get("message"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let judge_result = record.detail.as_ref().and_then(Self::render_judge_result);
        v_flex()
            .gap_1()
            .p_2()
            .rounded_md()
            .bg(colors.element_background)
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(
                        Label::new(status.label())
                            .size(LabelSize::Small)
                            .color(color),
                    )
                    .child(
                        Label::new(format!("{} 分", record.score))
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    )
                    .child(
                        Label::new(format!("{} ms", record.time))
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    )
                    .child(
                        Label::new(format!("{} KB", record.memory))
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    )
                    .child(
                        Label::new(Language::from_id(record.language).label())
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    ),
            )
            .when_some(compile_message, |this, message| {
                this.child(
                    Label::new(message)
                        .size(LabelSize::XSmall)
                        .color(Color::Error),
                )
            })
            .when_some(judge_result, |this, judge_result| this.child(judge_result))
            .into_any_element()
    }

    fn render_judge_result(detail: &Value) -> Option<AnyElement> {
        let judge = detail.get("judgeResult")?;
        let mut children: Vec<AnyElement> = Vec::new();
        let mut case_count = 0;
        if let Some(subtasks) = judge.get("subtasks").and_then(Value::as_array) {
            for subtask in subtasks {
                let id = subtask.get("id").and_then(Value::as_u64).unwrap_or(0);
                let score = subtask.get("score").and_then(Value::as_f64).unwrap_or(0.);
                let status = RecordStatus::from_id(
                    subtask.get("status").and_then(Value::as_i64).unwrap_or(0) as i32,
                );
                let time = subtask.get("time").and_then(Value::as_u64).unwrap_or(0);
                let memory = subtask.get("memory").and_then(Value::as_u64).unwrap_or(0);
                children.push(
                    h_flex()
                        .gap_3()
                        .items_center()
                        .mt_1()
                        .child(
                            Label::new(format!("子任务 #{id}"))
                                .size(LabelSize::XSmall)
                                .color(Color::Default),
                        )
                        .child(
                            Label::new(format!("{score:.0} 分"))
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        )
                        .child(
                            Label::new(status.label())
                                .size(LabelSize::XSmall)
                                .color(record_status_color(status)),
                        )
                        .child(
                            Label::new(format!("{time} ms · {memory} KB"))
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        )
                        .into_any_element(),
                );
                if let Some(cases) = subtask.get("testCases").and_then(Value::as_array) {
                    for case in cases {
                        case_count += 1;
                        children.push(Self::render_test_case(case));
                    }
                }
            }
        }
        if case_count == 0 {
            if let Some(cases) = judge.get("testCases").and_then(Value::as_array) {
                for case in cases {
                    children.push(Self::render_test_case(case));
                }
            }
        }
        if children.is_empty() {
            return None;
        }
        Some(
            v_flex()
                .gap_1()
                .mt_1()
                .max_h(px(240.))
                .id("oj-judge-detail-scroll")
                .overflow_y_scroll()
                .children(children)
                .into_any_element(),
        )
    }

    fn render_test_case(case: &Value) -> AnyElement {
        let id = case.get("id").and_then(Value::as_u64).unwrap_or(0);
        let status =
            RecordStatus::from_id(case.get("status").and_then(Value::as_i64).unwrap_or(0) as i32);
        let time = case.get("time").and_then(Value::as_u64).unwrap_or(0);
        let memory = case.get("memory").and_then(Value::as_u64).unwrap_or(0);
        let score = case.get("score").and_then(Value::as_f64).unwrap_or(0.);
        h_flex()
            .gap_3()
            .items_center()
            .pl_2()
            .child(
                Label::new(format!("测试点 #{id}"))
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
            .child(
                Label::new(status.label())
                    .size(LabelSize::XSmall)
                    .color(record_status_color(status)),
            )
            .child(
                Label::new(format!("{time} ms"))
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
            .child(
                Label::new(format!("{memory} KB"))
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
            .child(
                Label::new(format!("{score:.0} 分"))
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
            .when_some(
                case.get("description")
                    .and_then(Value::as_str)
                    .filter(|description| !description.is_empty() && *description != "ok accepted"),
                |this, description| {
                    this.child(
                        Label::new(description)
                            .size(LabelSize::XSmall)
                            .color(Color::Error),
                    )
                },
            )
            .into_any_element()
    }
}

impl Focusable for OjProblemView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<()> for OjProblemView {}

impl Item for OjProblemView {
    type Event = ();

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        format!("{} {}", self.problem.pid, self.problem.name).into()
    }

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<Icon> {
        Some(Icon::new(IconName::Book))
    }
}

impl Render for OjProblemView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let submit_card = if self.show_submit {
            Some(self.render_submit(cx))
        } else {
            None
        };
        let colors = cx.theme().colors();
        v_flex()
            .size_full()
            .bg(colors.editor_background)
            .track_focus(&self.focus_handle)
            .child(self.render_header(cx))
            .children(submit_card)
            .child(
                v_flex()
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .id("oj-problem-tab-scroll")
                    .overflow_y_scroll()
                    .px_6()
                    .pb_6()
                    .child(MarkdownElement::new(
                        self.markdown.clone(),
                        MarkdownStyle::themed(MarkdownFont::Preview, window, cx),
                    ))
                    .child(self.render_samples(cx)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::problem_id_from_file_name;

    #[test]
    fn extracts_problem_ids_from_file_names() {
        assert_eq!(
            problem_id_from_file_name("P1001.cpp").as_deref(),
            Some("P1001")
        );
        assert_eq!(
            problem_id_from_file_name("p1001.rs").as_deref(),
            Some("P1001")
        );
        assert_eq!(
            problem_id_from_file_name("1001.py").as_deref(),
            Some("P1001")
        );
        assert_eq!(
            problem_id_from_file_name("CF1A.cpp").as_deref(),
            Some("CF1A")
        );
        assert_eq!(
            problem_id_from_file_name("P1001 两数之和.cpp").as_deref(),
            Some("P1001")
        );
        assert_eq!(problem_id_from_file_name("solution.cpp"), None);
        assert_eq!(problem_id_from_file_name("main_v2.cpp"), None);
    }
}
