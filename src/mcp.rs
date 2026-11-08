//! MCP server over stdio: check_comment and check_batch.

use futures_util::{StreamExt, stream};
use replysieve::{Checker, Comment, Level, Verdict};
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{ErrorData, ServiceExt, tool, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Comments judged at once in check_batch.
const CONCURRENCY: usize = 8;

#[derive(Deserialize, JsonSchema)]
struct CheckComment {
    /// The comment text.
    text: String,
    /// Title of the post or product the comment is on.
    title: Option<String>,
    /// Description of the post or product.
    description: Option<String>,
    /// The comment this one replies to.
    parent: Option<String>,
    /// light, medium or heavy. Defaults to the server's level.
    level: Option<Level>,
    /// Recent texts from the same author or thread; an identical comment is a duplicate.
    #[serde(default)]
    recent: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
struct CheckBatch {
    /// The comments, each with text and optional title, description and parent.
    comments: Vec<Comment>,
    /// light, medium or heavy. Defaults to the server's level.
    level: Option<Level>,
    /// Recent texts from the same author or thread; an identical comment is a duplicate.
    #[serde(default)]
    recent: Vec<String>,
}

#[derive(Serialize, JsonSchema)]
struct Verdicts {
    /// One verdict per comment, in input order.
    verdicts: Vec<Verdict>,
}

struct Server {
    checker: Checker,
}

impl Server {
    fn at(&self, level: Option<Level>) -> Checker {
        match level {
            Some(l) => self.checker.clone().at_level(l),
            None => self.checker.clone(),
        }
    }
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
        let comment = Comment {
            text: p.text,
            title: p.title,
            description: p.description,
            parent: p.parent,
        };
        let v = self
            .at(p.level)
            .check(&comment, &p.recent)
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
        let checker = self.at(p.level);
        let recent = std::sync::Arc::new(p.recent);
        let results: Vec<_> = stream::iter(p.comments.into_iter().map(|c| {
            let (checker, recent) = (checker.clone(), recent.clone());
            async move { checker.check(&c, &recent).await }
        }))
        .buffered(CONCURRENCY)
        .collect()
        .await;
        let verdicts = results
            .into_iter()
            .collect::<Result<_, _>>()
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
