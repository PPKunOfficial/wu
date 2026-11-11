use oj_sdk::luogu::{
    extract_lentille_context, extract_meta_csrf_token, parse_problem_list_page, parse_problem_page,
    parse_record_list_page, parse_record_page, parse_training_list_page, parse_training_page,
    parse_user_page,
};
use oj_sdk::{Difficulty, Language, RecordStatus, Session};

#[test]
fn parses_problem_page() {
    let problem = parse_problem_page(include_str!("fixtures/problem_page.html"))
        .expect("problem page should parse");

    assert_eq!(problem.pid, "P1001");
    assert_eq!(problem.name, "A+B Problem");
    assert_eq!(problem.difficulty, 1);
    assert_eq!(problem.difficulty(), Difficulty::Introductory);
    assert_eq!(problem.accept_languages, vec![2, 12]);
    assert_eq!(problem.samples.len(), 1);
    assert_eq!(problem.samples[0].input, "20 30\n");
    assert_eq!(problem.samples[0].output, "50\n");
    assert_eq!(problem.limits.time, vec![1000, 1000]);
    assert_eq!(problem.limits.memory, vec![524288, 524288]);

    let content = problem.contenu.as_ref().expect("content");
    assert_eq!(content.locale.as_deref(), Some("zh-CN"));
    assert!(
        content
            .hint
            .as_deref()
            .is_some_and(|hint| hint.contains("10^9"))
    );
}

#[test]
fn parses_problem_list_page() {
    let page = parse_problem_list_page(include_str!("fixtures/problem_list_page.html"))
        .expect("problem list should parse");

    assert_eq!(page.per_page, 50);
    assert_eq!(page.count, 2);
    assert_eq!(page.result.len(), 2);
    assert_eq!(page.result[0].pid, "P1001");
    assert_eq!(page.result[0].difficulty(), Difficulty::Introductory);
    assert_eq!(page.result[1].pid, "B2001");
    assert_eq!(page.result[1].difficulty(), Difficulty::PopularMinus);
    assert!(page.result[1].flag.is_null());
}

#[test]
fn parses_record_page() {
    let record = parse_record_page(include_str!("fixtures/record_page.html"))
        .expect("record page should parse");

    assert_eq!(record.id, 309247588);
    assert_eq!(record.status(), RecordStatus::Accepted);
    assert!(record.is_finished());
    assert_eq!(record.language, Language::Cpp17.id());
    assert_eq!(Language::from_id(record.language).label(), "C++17");
    assert_eq!(
        record.source_code.as_deref(),
        Some("int main() { return 0; }")
    );
    assert_eq!(
        record.problem.as_ref().map(|problem| problem.pid.as_str()),
        Some("P1001")
    );
}

#[test]
fn parses_record_list_page() {
    let page = parse_record_list_page(include_str!("fixtures/record_list_page.html"))
        .expect("record list should parse");

    assert_eq!(page.per_page, 20);
    assert_eq!(page.count, 1);
    assert_eq!(page.result.len(), 1);
    assert_eq!(page.result[0].status(), RecordStatus::WrongAnswer);
    assert!(page.result[0].is_finished());
    assert!(page.result[0].source_code.is_none());
}

#[test]
fn parses_user_page() {
    let user = parse_user_page(include_str!("fixtures/user_page.html")).expect("user page");

    assert_eq!(user.uid, 311493);
    assert_eq!(user.name, "PPKun");
}

#[test]
fn extracts_lentille_context_regardless_of_attribute_order() {
    let html = r#"<script type="application/json" id="lentille-context">{"template":"problem.show"}</script>"#;
    let context = extract_lentille_context(html).expect("context");

    assert_eq!(context["template"], "problem.show");
}

#[test]
fn extracts_csrf_token_regardless_of_attribute_order() {
    let html = r#"<meta content="1794484228:token" name="csrf-token">"#;

    assert_eq!(
        extract_meta_csrf_token(html).as_deref(),
        Some("1794484228:token")
    );
}

#[test]
fn parses_cookie_strings() {
    let session = Session::from_cookie_string("Cookie: __client_id=abc; _uid=311493; C3VK=123")
        .expect("cookie string");

    assert_eq!(session.uid(), Some(311493));
    assert_eq!(
        session.to_cookie_string(),
        "C3VK=123; __client_id=abc; _uid=311493"
    );
}

#[test]
fn updates_cookies_from_set_cookie() {
    let mut session = Session::from_cookie_string("C3VK=123; _uid=311493").expect("cookie string");

    session.update_from_set_cookie("C3VK=456; Path=/; HttpOnly");

    assert_eq!(session.get("C3VK"), Some("456"));
    assert_eq!(session.uid(), Some(311493));
}

#[test]
fn rejects_malformed_cookie_strings() {
    assert!(Session::from_cookie_string("not-a-cookie").is_err());
    assert!(Session::from_cookie_string("=value").is_err());
}

#[test]
fn maps_ids_to_enums() {
    assert_eq!(Language::from_id(12), Language::Cpp17);
    assert_eq!(Language::Cpp17.id(), 12);
    assert_eq!(Language::from_id(999), Language::Unknown(999));
    assert_eq!(Difficulty::from_id(8), Difficulty::National);
    assert_eq!(Difficulty::from_id(99), Difficulty::Unknown(99));
    assert_eq!(RecordStatus::from_id(0), RecordStatus::Waiting);
    assert!(RecordStatus::from_id(0).is_pending());
    assert_eq!(RecordStatus::from_id(42), RecordStatus::Unknown(42));
}

fn training_html(template: &str, data: &str) -> String {
    format!(
        r#"<html><head></head><body><script id="lentille-context" type="application/json">{{"instance":"main","template":"{template}","status":200,"data":{data}}}</script></body></html>"#
    )
}

#[test]
fn parses_training_list_page() {
    let html = training_html(
        "training.list",
        r#"{"trainings":{"perPage":18,"count":2,"result":[
            {"id":100,"name":"新手入门","type":"public","provider":"洛谷","createTime":1700000000,"deadline":null,"problemCount":5,"markCount":0},
            {"id":101,"name":"进阶训练","type":"public","provider":"洛谷","createTime":1700000001,"deadline":null,"problemCount":8,"markCount":2}
        ]}}"#,
    );

    let page = parse_training_list_page(&html).expect("training list should parse");
    assert_eq!(page.per_page, 18);
    assert_eq!(page.count, 2);
    assert_eq!(page.result[0].id, 100);
    assert_eq!(page.result[0].name, "新手入门");
    assert_eq!(page.result[0].provider_name(), Some("洛谷"));
    assert_eq!(page.result[0].problem_count, 5);
    assert_eq!(page.result[1].id, 101);
    assert_eq!(page.result[1].mark_count, 2);
}

#[test]
fn parses_training_page() {
    let html = training_html(
        "training.show",
        r#"{"training":{"id":100,"name":"新手入门","type":"public","provider":"洛谷","createTime":1700000000,"deadline":null,"problemCount":1,"markCount":0,"marked":false,"description":"入门题目","problems":[
            {"pid":"P1001","type":"P","name":"A+B Problem","difficulty":1,"fullScore":100,"tags":[1],"totalSubmit":10,"totalAccepted":5,"flag":null,"provider":null,"submitted":false,"accepted":false}
        ]}}"#,
    );

    let training = parse_training_page(&html).expect("training should parse");
    assert_eq!(training.id(), 100);
    assert_eq!(training.name(), "新手入门");
    assert_eq!(training.marked, false);
    assert_eq!(training.description.as_deref(), Some("入门题目"));
    assert_eq!(training.problems.len(), 1);
    assert_eq!(training.problems[0].pid, "P1001");
}
