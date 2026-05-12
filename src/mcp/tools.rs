// src/mcp/tools.rs

use super::handlers;
use super::llm_client::McpLlmClient;
use super::logger::ActivityLogger;
use super::protocol::ToolResult;

/// MCP tool definitions returned by tools/list.
pub fn tool_definitions() -> serde_json::Value {
    serde_json::json!({
        "tools": [
            {
                "name": "local_summarize",
                "description": "Run a shell command and return a concise summary of its output. Use this instead of Bash for noisy commands (builds, test suites) where you only need pass/fail and key details. Saves tokens by keeping verbose output out of context.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The shell command to run"
                        }
                    },
                    "required": ["command"]
                }
            },
            {
                "name": "local_explain",
                "description": "Explain what a piece of code does in plain English. Use this for 'what does this function do?' questions — cheaper than reading and analyzing the code yourself.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "code": {
                            "type": "string",
                            "description": "The code to explain"
                        }
                    },
                    "required": ["code"]
                }
            },
            {
                "name": "local_ask",
                "description": "Ask the local model a question, optionally with context. Use this for simple questions that don't require frontier reasoning.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "question": {
                            "type": "string",
                            "description": "The question to ask"
                        },
                        "context": {
                            "type": "string",
                            "description": "Optional context to inform the answer"
                        }
                    },
                    "required": ["question"]
                }
            },
            {
                "name": "local_web_fetch",
                "description": "Fetch a web page and answer a specific question about its content. Use this instead of WebFetch to keep large HTML pages out of your context — the local model reads the page and returns only the answer.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "The URL to fetch"
                        },
                        "question": {
                            "type": "string",
                            "description": "What to extract from the page"
                        }
                    },
                    "required": ["url", "question"]
                }
            },
            {
                "name": "local_review",
                "description": "Summarize a code diff. Returns a concise description of what changed and why. Experimental — quality may vary.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "diff": {
                            "type": "string",
                            "description": "The diff to summarize"
                        }
                    },
                    "required": ["diff"]
                }
            },
            {
                "name": "local_draft",
                "description": "Generate code or text using the local model. Use for boilerplate, templates, and straightforward generation tasks. Experimental — quality may vary for complex code.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "task": {
                            "type": "string",
                            "description": "What to generate"
                        },
                        "context": {
                            "type": "string",
                            "description": "Optional context (existing code, requirements, etc.)"
                        }
                    },
                    "required": ["task"]
                }
            },
            {
                "name": "local_status",
                "description": "Check the status of the local model. Returns model name, health, and endpoint info.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
            }
        ]
    })
}

/// Dispatch a tool call to the appropriate handler.
pub async fn dispatch(
    tool_name: &str,
    arguments: &serde_json::Value,
    client: &McpLlmClient,
    logger: &ActivityLogger,
) -> ToolResult {
    match tool_name {
        "local_summarize" => {
            let command = arguments.get("command").and_then(|v| v.as_str()).unwrap_or("");
            handlers::handle_summarize(client, logger, command).await
        }
        "local_explain" => {
            let code = arguments.get("code").and_then(|v| v.as_str()).unwrap_or("");
            handlers::handle_explain(client, logger, code).await
        }
        "local_ask" => {
            let question = arguments.get("question").and_then(|v| v.as_str()).unwrap_or("");
            let context = arguments.get("context").and_then(|v| v.as_str());
            handlers::handle_ask(client, logger, question, context).await
        }
        "local_web_fetch" => {
            let url = arguments.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let question = arguments.get("question").and_then(|v| v.as_str()).unwrap_or("");
            handlers::handle_web_fetch(client, logger, url, question).await
        }
        "local_review" => {
            let diff = arguments.get("diff").and_then(|v| v.as_str()).unwrap_or("");
            handlers::handle_review(client, logger, diff).await
        }
        "local_draft" => {
            let task = arguments.get("task").and_then(|v| v.as_str()).unwrap_or("");
            let context = arguments.get("context").and_then(|v| v.as_str());
            handlers::handle_draft(client, logger, task, context).await
        }
        "local_status" => {
            handlers::handle_status(client).await
        }
        _ => ToolResult::error(format!("Unknown tool: {}", tool_name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_definitions_has_seven_tools() {
        let defs = tool_definitions();
        let tools = defs["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 7);
    }

    #[test]
    fn test_tool_definitions_names() {
        let defs = tool_definitions();
        let names: Vec<&str> = defs["tools"].as_array().unwrap()
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        assert!(names.contains(&"local_summarize"));
        assert!(names.contains(&"local_explain"));
        assert!(names.contains(&"local_ask"));
        assert!(names.contains(&"local_web_fetch"));
        assert!(names.contains(&"local_review"));
        assert!(names.contains(&"local_draft"));
        assert!(names.contains(&"local_status"));
    }

    #[test]
    fn test_all_tools_have_input_schema() {
        let defs = tool_definitions();
        for tool in defs["tools"].as_array().unwrap() {
            assert!(tool.get("inputSchema").is_some(), "Tool {} missing inputSchema", tool["name"]);
        }
    }
}
