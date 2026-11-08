//! MCP server over stdio: check_comment and check_batch.

use replysieve::{Checker, Comment, Level, Verdict};
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{ErrorData, ServiceExt, tool, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One comment to judge. Also the body of the HTTP server's POST /v1/check.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct CheckComment {
    /// The comment text.
    pub text: String,
    /// Title of the post or product the comment is on.
    pub title: Option<String>,
    /// Description of the post or product.
    pub description: Option<String>,
    /// The comment this one replies to.
    pub parent: Option<String>,
    /// light, medium or heavy. Defaults to the server's level.
    pub level: Option<Level>,
    /// Recent texts from the same author or thread; an identical comment is a duplicate.
    #[serde(default)]
    pub recent: Vec<String>,
}

impl CheckComment {
    pub fn comment(&self) -> Comment {
        Comment {
            text: self.text.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            parent: self.parent.clone(),
        }
    }
}

/// Many comments to judge. Also the body of the HTTP server's POST /v1/check/batch.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct CheckBatch {
    /// The comments, each with text and optional title, description and parent.
    pub comments: Vec<Comment>,
    /// light, medium or heavy. Defaults to the server's level.
    pub level: Option<Level>,
    /// Recent texts from the same author or thread; an identical comment is a duplicate.
    #[serde(default)]
    pub recent: Vec<String>,
}

/// One verdict per comment. Also the HTTP server's POST /v1/check/batch response.
#[derive(Serialize, JsonSchema)]
pub(crate) struct Verdicts {
    /// One verdict per comment, in input order.
    pub verdicts: Vec<Verdict>,
}

/// `checker` at `level`, or unchanged when no level was asked for.
pub(crate) fn at(checker: &Checker, level: Option<Level>) -> Checker {
    match level {
        Some(l) => checker.clone().at_level(l),
        None => checker.clone(),
    }
}

struct Server {
    checker: Checker,
}

fn failed(e: replysieve::Error) -> ErrorData {
    ErrorData::invalid_request(e.to_string(), None)
}

#[tool_router(server_handler)]
impl Server {
    #[tool(
        name = "check_comment",
        description = "Judge one comment: approve, reject or needs_review, with a reason, confidence, level and model."
    )]
    async fn check_comment(
        &self,
        Parameters(p): Parameters<CheckComment>,
    ) -> Result<Json<Verdict>, ErrorData> {
        let v = at(&self.checker, p.level)
            .check(&p.comment(), &p.recent)
            .await
            .map_err(failed)?;
        Ok(Json(v))
    }

    #[tool(
        name = "check_batch",
        description = "Judge many comments at once. Returns one verdict per comment, in input order."
    )]
    async fn check_batch(
        &self,
        Parameters(p): Parameters<CheckBatch>,
    ) -> Result<Json<Verdicts>, ErrorData> {
        let verdicts = at(&self.checker, p.level)
            .check_batch(&p.comments, &p.recent)
            .await
            .map_err(failed)?;
        Ok(Json(Verdicts { verdicts }))
    }
}

/// Serve MCP on stdin and stdout until the client disconnects.
pub async fn serve(checker: Checker) -> anyhow::Result<()> {
    let running = Server { checker }.serve(rmcp::transport::stdio()).await?;
    running.waiting().await?;
    Ok(())
}
