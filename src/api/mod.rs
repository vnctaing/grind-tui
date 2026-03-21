use std::collections::HashMap;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, REFERER};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Authentication failed")]
    AuthFailed,
    #[error("Rate limited")]
    RateLimited,
    #[error("Problem not found")]
    NotFound,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub session_token: String,
    pub csrf_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Problem {
    pub id: u32,
    pub title: String,
    pub slug: String,
    pub difficulty: String,
    pub tags: Vec<String>,
    pub acceptance_rate: f64,
    pub paid_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblemDetail {
    pub id: u32,
    pub title: String,
    pub slug: String,
    pub difficulty: String,
    pub description: String,
    pub examples: Vec<String>,
    pub constraints: Vec<String>,
    pub starter_code: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitResult {
    pub status: String,
    pub runtime_ms: Option<u32>,
    pub memory_mb: Option<f64>,
    pub passed: u32,
    pub total: u32,
    pub error_detail: Option<String>,
}

// Raw API response types
#[derive(Deserialize)]
struct ProblemsResponse {
    stat_status_pairs: Vec<StatStatusPair>,
}

#[derive(Deserialize)]
struct StatStatusPair {
    stat: ProblemStat,
    difficulty: ProblemDifficulty,
    paid_only: bool,
}

#[allow(non_snake_case)]
#[derive(Deserialize)]
struct ProblemStat {
    question_id: u32,
    question__title: String,
    question__title_slug: String,
    total_acs: f64,
    total_submitted: f64,
}

#[derive(Deserialize)]
struct ProblemDifficulty {
    level: u32,
}

#[derive(Deserialize)]
struct GraphqlResponse<T> {
    data: T,
}

#[derive(Deserialize)]
struct QuestionData {
    question: QuestionDetail,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct QuestionDetail {
    #[serde(rename = "questionId")]
    question_id: String,
    title: String,
    #[serde(rename = "titleSlug")]
    title_slug: String,
    difficulty: String,
    content: Option<String>,
    #[serde(rename = "topicTags")]
    topic_tags: Vec<TopicTag>,
    #[serde(rename = "codeSnippets")]
    code_snippets: Option<Vec<CodeSnippet>>,
    #[serde(rename = "exampleTestcases")]
    example_testcases: Option<String>,
}

#[derive(Deserialize)]
struct TopicTag {
    name: String,
}

#[derive(Deserialize)]
struct CodeSnippet {
    #[serde(rename = "langSlug")]
    lang_slug: String,
    code: String,
}

#[derive(Deserialize)]
struct SubmitResponse {
    submission_id: Option<u64>,
    interpret_id: Option<String>,
}

// CheckResponse is parsed as raw Value to handle LeetCode's inconsistent field types

pub struct LeetCodeClient {
    client: reqwest::Client,
}

impl LeetCodeClient {
    pub fn new(session: Session) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            "Cookie",
            HeaderValue::from_str(&format!(
                "LEETCODE_SESSION={}; csrftoken={}",
                session.session_token, session.csrf_token
            ))
            .unwrap_or(HeaderValue::from_static("")),
        );
        headers.insert(
            "X-CSRFToken",
            HeaderValue::from_str(&session.csrf_token)
                .unwrap_or(HeaderValue::from_static("")),
        );
        headers.insert(
            reqwest::header::USER_AGENT,
            HeaderValue::from_static(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36",
            ),
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .expect("Failed to build HTTP client");

        Self { client }
    }

    pub async fn get_all_problems(&self) -> Result<Vec<Problem>, ApiError> {
        let resp = self
            .client
            .get("https://leetcode.com/api/problems/all/")
            .send()
            .await?;

        if resp.status() == 403 {
            return Err(ApiError::AuthFailed);
        }
        if resp.status() == 429 {
            return Err(ApiError::RateLimited);
        }

        let data: ProblemsResponse = resp
            .json()
            .await
            .map_err(|e| ApiError::Parse(e.to_string()))?;

        let mut problems: Vec<Problem> = data
            .stat_status_pairs
            .into_iter()
            .map(|p| {
                let acceptance = if p.stat.total_submitted > 0.0 {
                    p.stat.total_acs / p.stat.total_submitted * 100.0
                } else {
                    0.0
                };
                Problem {
                    id: p.stat.question_id,
                    title: p.stat.question__title,
                    slug: p.stat.question__title_slug,
                    difficulty: match p.difficulty.level {
                        1 => "Easy".to_string(),
                        2 => "Medium".to_string(),
                        3 => "Hard".to_string(),
                        _ => "Unknown".to_string(),
                    },
                    tags: vec![],
                    acceptance_rate: acceptance,
                    paid_only: p.paid_only,
                }
            })
            .collect();

        problems.sort_by_key(|p| p.id);
        Ok(problems)
    }

    pub async fn get_problem(&self, slug: &str) -> Result<ProblemDetail, ApiError> {
        let query = r#"
            query questionData($titleSlug: String!) {
                question(titleSlug: $titleSlug) {
                    questionId
                    title
                    titleSlug
                    difficulty
                    content
                    topicTags { name }
                    codeSnippets { langSlug code }
                    exampleTestcases
                }
            }
        "#;

        let body = serde_json::json!({
            "query": query,
            "variables": { "titleSlug": slug },
            "operationName": "questionData"
        });

        let resp = self
            .client
            .post("https://leetcode.com/graphql")
            .header(CONTENT_TYPE, "application/json")
            .header(REFERER, format!("https://leetcode.com/problems/{}/", slug))
            .json(&body)
            .send()
            .await?;

        if resp.status() == 403 {
            return Err(ApiError::AuthFailed);
        }

        let gql: GraphqlResponse<QuestionData> = resp
            .json()
            .await
            .map_err(|e| ApiError::Parse(e.to_string()))?;

        let q = gql.data.question;
        let description = strip_html(&q.content.unwrap_or_default());

        let mut starter_code = HashMap::new();
        if let Some(snippets) = q.code_snippets {
            for s in snippets {
                starter_code.insert(s.lang_slug, s.code);
            }
        }

        let id = q.question_id.parse::<u32>().unwrap_or(0);

        Ok(ProblemDetail {
            id,
            title: q.title,
            slug: q.title_slug,
            difficulty: q.difficulty,
            description,
            examples: q
                .example_testcases
                .unwrap_or_default()
                .lines()
                .map(str::to_string)
                .collect(),
            constraints: vec![],
            starter_code,
        })
    }

    pub async fn submit(
        &self,
        slug: &str,
        code: &str,
        lang: &str,
        question_id: &str,
    ) -> Result<SubmitResult, ApiError> {
        let url = format!("https://leetcode.com/problems/{}/submit/", slug);
        let body = serde_json::json!({
            "lang": lang,
            "question_id": question_id,
            "typed_code": code,
        });

        let resp = self
            .client
            .post(&url)
            .header(CONTENT_TYPE, "application/json")
            .header(REFERER, format!("https://leetcode.com/problems/{}/", slug))
            .json(&body)
            .send()
            .await?;

        if resp.status() == 403 {
            return Err(ApiError::AuthFailed);
        }

        let submit: SubmitResponse = resp
            .json()
            .await
            .map_err(|e| ApiError::Parse(e.to_string()))?;

        let submission_id = submit
            .submission_id
            .ok_or_else(|| ApiError::Parse("No submission_id in response".to_string()))?;

        self.poll_result(&submission_id.to_string()).await
    }

    pub async fn run_tests(
        &self,
        slug: &str,
        code: &str,
        lang: &str,
        test_input: &str,
        question_id: &str,
    ) -> Result<SubmitResult, ApiError> {
        let url = format!(
            "https://leetcode.com/problems/{}/interpret_solution/",
            slug
        );
        let body = serde_json::json!({
            "lang": lang,
            "question_id": question_id,
            "typed_code": code,
            "data_input": test_input,
        });

        let resp = self
            .client
            .post(&url)
            .header(CONTENT_TYPE, "application/json")
            .header(REFERER, format!("https://leetcode.com/problems/{}/", slug))
            .json(&body)
            .send()
            .await?;

        if resp.status() == 403 {
            return Err(ApiError::AuthFailed);
        }

        let submit: SubmitResponse = resp
            .json()
            .await
            .map_err(|e| ApiError::Parse(e.to_string()))?;

        let submission_id = submit
            .interpret_id
            .or_else(|| submit.submission_id.map(|id| id.to_string()))
            .ok_or_else(|| ApiError::Parse("No submission_id in response".to_string()))?;

        self.poll_result(&submission_id).await
    }

    async fn poll_result(&self, submission_id: &str) -> Result<SubmitResult, ApiError> {
        let url = format!(
            "https://leetcode.com/submissions/detail/{}/check/",
            submission_id
        );

        for _ in 0..30 {
            tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;

            let resp = self.client.get(&url).send().await?;
            let check: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| ApiError::Parse(e.to_string()))?;

            let state = check["state"].as_str().unwrap_or("");
            if state == "STARTED" || state == "PENDING" {
                continue;
            }

            let str_field = |key: &str| -> Option<String> {
                check[key].as_str().map(str::to_string)
            };
            let u32_field = |key: &str| -> u32 {
                check[key].as_u64().unwrap_or(0) as u32
            };

            let runtime_ms = str_field("status_runtime")
                .as_deref()
                .and_then(|s| s.split_whitespace().next())
                .and_then(|n| n.parse().ok());
            let memory_mb = str_field("status_memory")
                .as_deref()
                .and_then(|s| s.split_whitespace().next())
                .and_then(|n| n.parse().ok());

            // code_output may be a string or array
            let code_output_str = match &check["code_output"] {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Array(arr) => Some(
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
                _ => None,
            };

            // Run-tests response: per-test comparison via code_answer + expected_code_answer + compare_result
            let run_test_detail = {
                let got_arr = check["code_answer"].as_array();
                let exp_arr = check["expected_code_answer"].as_array();
                let compare = check["compare_result"].as_str().unwrap_or("");
                match (got_arr, exp_arr) {
                    (Some(got), Some(exp)) if !compare.is_empty() => {
                        let lines: Vec<String> = compare.chars().enumerate()
                            .filter_map(|(i, c)| {
                                let got_val = got.get(i).and_then(|v| v.as_str()).unwrap_or("?");
                                let exp_val = exp.get(i).and_then(|v| v.as_str()).unwrap_or("?");
                                if c == '0' {
                                    Some(format!("✗ Test {}\nGot:      {}\nExpected: {}", i + 1, got_val, exp_val))
                                } else {
                                    Some(format!("✓ Test {}: {}", i + 1, got_val))
                                }
                            })
                            .collect();
                        if lines.is_empty() { None } else { Some(lines.join("\n")) }
                    }
                    _ => None,
                }
            };

            let error_detail = str_field("full_runtime_error")
                .or_else(|| str_field("runtime_error"))
                .or_else(|| str_field("full_compile_error"))
                .or_else(|| str_field("compile_error"))
                .or_else(|| run_test_detail)
                .or_else(|| {
                    match (str_field("last_testcase"), str_field("expected_output"), code_output_str) {
                        (Some(tc), Some(exp), Some(got)) => Some(format!(
                            "Input: {}\nExpected: {}\nGot: {}",
                            tc.trim(), exp.trim(), got.trim()
                        )),
                        _ => None,
                    }
                })
                .map(|s| strip_tags(&s));

            return Ok(SubmitResult {
                status: str_field("status_msg").unwrap_or_else(|| "Unknown".to_string()),
                runtime_ms,
                memory_mb,
                passed: u32_field("total_correct"),
                total: u32_field("total_testcases"),
                error_detail,
            });
        }

        Err(ApiError::Parse("Timed out waiting for result".to_string()))
    }
}

fn strip_tags(s: &str) -> String {
    // Remove all <tag> and </tag> occurrences
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&nbsp;", " ")
        .replace("&#39;", "'")
}

fn strip_html(html: &str) -> String {
    html.replace("<p>", "")
        .replace("</p>", "\n")
        .replace("<strong>", "**")
        .replace("</strong>", "**")
        .replace("<em>", "_")
        .replace("</em>", "_")
        .replace("<code>", "`")
        .replace("</code>", "`")
        .replace("<pre>", "\n```\n")
        .replace("</pre>", "\n```\n")
        .replace("<ul>", "")
        .replace("</ul>", "")
        .replace("<li>", "  • ")
        .replace("</li>", "\n")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&nbsp;", " ")
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
}

pub fn load_session() -> Option<Session> {
    let path = crate::state::config_dir().join("session.json");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

pub fn save_session(session: &Session) {
    let dir = crate::state::config_dir();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(s) = serde_json::to_string_pretty(session) {
        let _ = std::fs::write(dir.join("session.json"), s);
    }
}
