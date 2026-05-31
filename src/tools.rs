use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::json;
use thiserror::Error;

use crate::ask::ToolBox;
use crate::config::{Config, Sandbox};
use crate::model::{FunctionDef, ToolCall, ToolDef, ToolKind};

const TRUNCATION_MARK: &str = "\n…[truncated]";
const WEB_SEARCH_STUB: &str = "web_search is not configured in this build.";

pub struct Tools {
    root: PathBuf,
    output_cap: usize,
    sandbox: Sandbox,
    http: reqwest::Client,
}

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("invalid tool arguments: {0}")]
    BadArgs(String),
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("path not found or unreadable: {0}")]
    NotFound(String),
    #[error("path escapes the allowed root: {0}")]
    OutsideRoot(String),
    #[error("only http(s) URLs may be fetched")]
    Scheme,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("non-UTF-8 path: {0}")]
    NonUtf8Path(String),
    #[error("sandbox error: {0}")]
    Sandbox(String),
    #[error("http error: {0}")]
    Http(String),
}

#[derive(Deserialize)]
struct PathArgs {
    path: String,
}

#[derive(Deserialize)]
struct UrlArgs {
    url: String,
}

impl Tools {
    pub fn new(
        root: impl AsRef<Path>,
        output_cap: usize,
        sandbox: Sandbox,
    ) -> Result<Self, ToolError> {
        let root = root.as_ref().canonicalize()?;
        let http = reqwest::Client::builder()
            .build()
            .map_err(|error| ToolError::Http(error.to_string()))?;
        Ok(Self {
            root,
            output_cap,
            sandbox,
            http,
        })
    }

    pub fn from_config(config: &Config) -> Result<Self, ToolError> {
        let root = std::env::current_dir()?;
        Self::new(root, config.budget.tool_output_cap, config.sandbox)
    }

    async fn dispatch(&self, call: &ToolCall) -> Result<String, ToolError> {
        match call.function.name.as_str() {
            "read_file" => self.read_file(&args::<PathArgs>(call)?.path).await,
            "list_dir" => self.list_dir(&args::<PathArgs>(call)?.path).await,
            "fetch_url" => self.fetch_url(&args::<UrlArgs>(call)?.url).await,
            "web_search" => Ok(WEB_SEARCH_STUB.to_string()),
            other => Err(ToolError::UnknownTool(other.to_string())),
        }
    }

    async fn read_file(&self, path: &str) -> Result<String, ToolError> {
        let target = confine(&self.root, path).await?;
        let content = match self.sandbox {
            Sandbox::Enabled => run_sandboxed(&self.root, &["cat", path_str(&target)?]).await?,
            Sandbox::Disabled => tokio::fs::read_to_string(&target).await?,
        };
        Ok(self.cap(content))
    }

    async fn list_dir(&self, path: &str) -> Result<String, ToolError> {
        let target = confine(&self.root, path).await?;
        let listing = match self.sandbox {
            Sandbox::Enabled => {
                run_sandboxed(&self.root, &["ls", "-1A", path_str(&target)?]).await?
            }
            Sandbox::Disabled => native_listing(&target).await?,
        };
        Ok(self.cap(listing))
    }

    async fn fetch_url(&self, url: &str) -> Result<String, ToolError> {
        if !is_http(url) {
            return Err(ToolError::Scheme);
        }
        let body = self
            .http
            .get(url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|error| ToolError::Http(error.to_string()))?
            .text()
            .await
            .map_err(|error| ToolError::Http(error.to_string()))?;
        Ok(self.cap(body))
    }

    fn cap(&self, text: String) -> String {
        truncate(text, self.output_cap)
    }
}

impl ToolBox for Tools {
    fn schemas(&self) -> Vec<ToolDef> {
        vec![
            tool(
                "read_file",
                "Read the contents of a file within the working directory.",
                json!({
                    "type": "object",
                    "properties": {"path": {"type": "string", "description": "Path relative to the working directory"}},
                    "required": ["path"]
                }),
            ),
            tool(
                "list_dir",
                "List the entries of a directory within the working directory.",
                json!({
                    "type": "object",
                    "properties": {"path": {"type": "string", "description": "Directory path relative to the working directory"}},
                    "required": ["path"]
                }),
            ),
            tool(
                "fetch_url",
                "Fetch the text of an http(s) URL.",
                json!({
                    "type": "object",
                    "properties": {"url": {"type": "string", "description": "An http or https URL"}},
                    "required": ["url"]
                }),
            ),
            tool(
                "web_search",
                "Search the web for a query.",
                json!({
                    "type": "object",
                    "properties": {"query": {"type": "string", "description": "Search query"}},
                    "required": ["query"]
                }),
            ),
        ]
    }

    async fn invoke(&self, call: &ToolCall) -> String {
        match self.dispatch(call).await {
            Ok(output) => output,
            Err(error) => format!("error: {error}"),
        }
    }
}

fn args<T: DeserializeOwned>(call: &ToolCall) -> Result<T, ToolError> {
    serde_json::from_str(&call.function.arguments)
        .map_err(|error| ToolError::BadArgs(error.to_string()))
}

fn tool(name: &str, description: &str, parameters: serde_json::Value) -> ToolDef {
    ToolDef {
        kind: ToolKind::Function,
        function: FunctionDef {
            name: name.to_string(),
            description: description.to_string(),
            parameters,
        },
    }
}

async fn confine(root: &Path, requested: &str) -> Result<PathBuf, ToolError> {
    let canonical = tokio::fs::canonicalize(root.join(requested))
        .await
        .map_err(|_| ToolError::NotFound(requested.to_string()))?;
    canonical
        .starts_with(root)
        .then_some(canonical)
        .ok_or_else(|| ToolError::OutsideRoot(requested.to_string()))
}

async fn native_listing(dir: &Path) -> Result<String, ToolError> {
    let mut read_dir = tokio::fs::read_dir(dir).await?;
    let mut entries = Vec::new();
    while let Some(entry) = read_dir.next_entry().await? {
        let suffix = match entry.file_type().await.map(|kind| kind.is_dir()) {
            Ok(true) => "/",
            _ => "",
        };
        entries.push(format!("{}{suffix}", entry.file_name().to_string_lossy()));
    }
    entries.sort();
    Ok(entries.join("\n"))
}

async fn run_sandboxed(root: &Path, argv: &[&str]) -> Result<String, ToolError> {
    let root = path_str(root)?;
    let output = tokio::process::Command::new("bwrap")
        .args(bwrap_args(root, argv))
        .output()
        .await
        .map_err(|error| ToolError::Sandbox(format!("could not run bwrap: {error}")))?;
    if !output.status.success() {
        return Err(ToolError::Sandbox(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn bwrap_args(root: &str, argv: &[&str]) -> Vec<String> {
    let mut args: Vec<String> = [
        "--ro-bind",
        root,
        root,
        "--ro-bind",
        "/usr",
        "/usr",
        "--ro-bind-try",
        "/bin",
        "/bin",
        "--ro-bind-try",
        "/lib",
        "/lib",
        "--ro-bind-try",
        "/lib64",
        "/lib64",
        "--ro-bind-try",
        "/etc",
        "/etc",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--unshare-all",
        "--die-with-parent",
        "--",
    ]
    .iter()
    .map(|part| part.to_string())
    .collect();
    args.extend(argv.iter().map(|part| part.to_string()));
    args
}

fn path_str(path: &Path) -> Result<&str, ToolError> {
    path.to_str()
        .ok_or_else(|| ToolError::NonUtf8Path(path.display().to_string()))
}

fn is_http(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

fn truncate(mut text: String, cap: usize) -> String {
    if text.len() <= cap {
        return text;
    }
    let mut end = cap;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text.push_str(TRUNCATION_MARK);
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FunctionCall, ToolKind};
    use std::fs;
    use tempfile::tempdir;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn tools_rooted_at(root: &Path) -> Tools {
        Tools::new(root, 64, Sandbox::Disabled).unwrap()
    }

    fn call(name: &str, arguments: &str) -> ToolCall {
        ToolCall {
            id: "c1".into(),
            kind: ToolKind::Function,
            function: FunctionCall {
                name: name.into(),
                arguments: arguments.into(),
            },
        }
    }

    #[tokio::test]
    async fn reads_a_file_inside_the_root() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("note.txt"), "hello").unwrap();
        let tools = tools_rooted_at(dir.path());
        assert_eq!(
            tools
                .invoke(&call("read_file", r#"{"path":"note.txt"}"#))
                .await,
            "hello"
        );
    }

    #[tokio::test]
    async fn truncates_oversized_output() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("big.txt"), "x".repeat(100)).unwrap();
        let tools = tools_rooted_at(dir.path());
        let out = tools
            .invoke(&call("read_file", r#"{"path":"big.txt"}"#))
            .await;
        assert!(out.ends_with(TRUNCATION_MARK));
        assert_eq!(out.len(), 64 + TRUNCATION_MARK.len());
    }

    #[tokio::test]
    async fn lists_directory_entries_sorted() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("b.txt"), "").unwrap();
        fs::write(dir.path().join("a.txt"), "").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        let tools = tools_rooted_at(dir.path());
        assert_eq!(
            tools.invoke(&call("list_dir", r#"{"path":"."}"#)).await,
            "a.txt\nb.txt\nsub/"
        );
    }

    #[tokio::test]
    async fn rejects_path_escaping_the_root() {
        let dir = tempdir().unwrap();
        let tools = tools_rooted_at(dir.path());
        let out = tools
            .invoke(&call("read_file", r#"{"path":"../../etc/passwd"}"#))
            .await;
        assert!(out.starts_with("error:"));
    }

    #[tokio::test]
    async fn missing_file_returns_error_string() {
        let dir = tempdir().unwrap();
        let tools = tools_rooted_at(dir.path());
        let out = tools
            .invoke(&call("read_file", r#"{"path":"nope.txt"}"#))
            .await;
        assert!(out.contains("not found"));
    }

    #[tokio::test]
    async fn fetches_url_body() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("page body"))
            .mount(&server)
            .await;
        let dir = tempdir().unwrap();
        let tools = tools_rooted_at(dir.path());
        let out = tools
            .invoke(&call(
                "fetch_url",
                &format!(r#"{{"url":"{}"}}"#, server.uri()),
            ))
            .await;
        assert_eq!(out, "page body");
    }

    #[tokio::test]
    async fn rejects_non_http_scheme() {
        let dir = tempdir().unwrap();
        let tools = tools_rooted_at(dir.path());
        let out = tools
            .invoke(&call("fetch_url", r#"{"url":"file:///etc/passwd"}"#))
            .await;
        assert!(out.contains("http(s)"));
    }

    #[tokio::test]
    async fn web_search_is_stubbed() {
        let dir = tempdir().unwrap();
        let tools = tools_rooted_at(dir.path());
        assert_eq!(
            tools.invoke(&call("web_search", r#"{"query":"x"}"#)).await,
            WEB_SEARCH_STUB
        );
    }

    #[tokio::test]
    async fn unknown_tool_and_bad_args_report_errors() {
        let dir = tempdir().unwrap();
        let tools = tools_rooted_at(dir.path());
        assert!(
            tools
                .invoke(&call("frobnicate", "{}"))
                .await
                .contains("unknown tool")
        );
        assert!(
            tools
                .invoke(&call("read_file", "not json"))
                .await
                .contains("invalid tool arguments")
        );
    }

    #[test]
    fn schemas_cover_all_four_tools() {
        let dir = tempdir().unwrap();
        let names: Vec<String> = tools_rooted_at(dir.path())
            .schemas()
            .into_iter()
            .map(|schema| schema.function.name)
            .collect();
        assert_eq!(names, ["read_file", "list_dir", "fetch_url", "web_search"]);
    }

    #[test]
    fn bwrap_args_bind_root_readonly_and_isolate() {
        let built = bwrap_args("/srv/work", &["cat", "/srv/work/f"]);
        assert_eq!(&built[0..3], &["--ro-bind", "/srv/work", "/srv/work"]);
        assert!(built.contains(&"--unshare-all".to_string()));
        assert_eq!(&built[built.len() - 2..], &["cat", "/srv/work/f"]);
    }
}
