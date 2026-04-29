use crate::mcp::ArpServer;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::prompt;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
pub struct CodeReviewArgs {
    pub workspace: String,
    #[serde(default)]
    pub files: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ParallelImplementationArgs {
    pub workspace: String,
    #[serde(default)]
    pub subtask_count: Option<u32>,
}

#[rmcp::prompt_router(vis = "pub")]
impl ArpServer {
    #[prompt(name = "task/code-review", description = "Spawn a reviewer agent and send it a code review task via A2A SendMessage")]
    pub async fn code_review(
        &self,
        Parameters(args): Parameters<CodeReviewArgs>,
    ) -> Result<GetPromptResult, ErrorData> {
        let files = args.files.unwrap_or_else(|| ".".into());
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "1. Use agent/spawn to create a reviewer agent in workspace \"{}\"\n\
                 2. Use agent/message to send: \"Review the code changes in {}\"\n\
                 3. Use agent/task_status to poll until the task completes\n\
                 4. Report the review findings",
                args.workspace, files,
            ),
        )])
        .with_description(format!("Code review in workspace {}", args.workspace)))
    }

    #[prompt(name = "task/parallel-implementation", description = "Spawn multiple agents in a workspace, each assigned a subtask via A2A SendMessage")]
    pub async fn parallel_implementation(
        &self,
        Parameters(args): Parameters<ParallelImplementationArgs>,
    ) -> Result<GetPromptResult, ErrorData> {
        let count = args.subtask_count.unwrap_or(2);
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "1. Use agent/spawn {} times in workspace \"{}\" with distinct names (worker-1, worker-2, ...)\n\
                 2. Break the task into {} subtasks\n\
                 3. Use agent/task to assign each worker a subtask\n\
                 4. Use agent/task_status to poll each until TASK_STATE_COMPLETED\n\
                 5. Summarize results from all workers",
                count, args.workspace, count,
            ),
        )])
        .with_description(format!(
            "Parallel implementation with {} agents in workspace {}",
            count, args.workspace
        )))
    }
}
