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
                "description": "Run a shell command and return a concise summary of its output. Use this to keep verbose command output out of context. Saves tokens by having a local model read the output and report the key points.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The shell command to run"
                        },
                        "context_tokens": {
                            "type": "integer",
                            "description": "Current conversation context size in tokens. Used for savings estimation."
                        }
                    },
                    "required": ["command", "context_tokens"]
                }
            },
            {
                "name": "local_pass_fail",
                "description": "Run a shell command and report pass or fail. Use this for builds, test suites, and linters where you only need to know if it succeeded and what went wrong if it didn't.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The shell command to run"
                        },
                        "context_tokens": {
                            "type": "integer",
                            "description": "Current conversation context size in tokens. Used for savings estimation."
                        }
                    },
                    "required": ["command", "context_tokens"]
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
                        },
                        "context_tokens": {
                            "type": "integer",
                            "description": "Current conversation context size in tokens. Used for savings estimation."
                        }
                    },
                    "required": ["code", "context_tokens"]
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
                        },
                        "context_tokens": {
                            "type": "integer",
                            "description": "Current conversation context size in tokens. Used for savings estimation."
                        }
                    },
                    "required": ["question", "context_tokens"]
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
                        },
                        "context_tokens": {
                            "type": "integer",
                            "description": "Current conversation context size in tokens. Used for savings estimation."
                        }
                    },
                    "required": ["url", "question", "context_tokens"]
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
                        },
                        "context_tokens": {
                            "type": "integer",
                            "description": "Current conversation context size in tokens. Used for savings estimation."
                        }
                    },
                    "required": ["diff", "context_tokens"]
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
                        },
                        "context_tokens": {
                            "type": "integer",
                            "description": "Current conversation context size in tokens. Used for savings estimation."
                        }
                    },
                    "required": ["task", "context_tokens"]
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
    let context_tokens = arguments.get("context_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    match tool_name {
        "local_summarize" => {
            let command = arguments.get("command").and_then(|v| v.as_str()).unwrap_or("");
            handlers::handle_summarize(client, logger, command, context_tokens).await
        }
        "local_pass_fail" => {
            let command = arguments.get("command").and_then(|v| v.as_str()).unwrap_or("");
            handlers::handle_pass_fail(client, logger, command, context_tokens).await
        }
        "local_explain" => {
            let code = arguments.get("code").and_then(|v| v.as_str()).unwrap_or("");
            handlers::handle_explain(client, logger, code, context_tokens).await
        }
        "local_ask" => {
            let question = arguments.get("question").and_then(|v| v.as_str()).unwrap_or("");
            let context = arguments.get("context").and_then(|v| v.as_str());
            handlers::handle_ask(client, logger, question, context, context_tokens).await
        }
        "local_web_fetch" => {
            let url = arguments.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let question = arguments.get("question").and_then(|v| v.as_str()).unwrap_or("");
            handlers::handle_web_fetch(client, logger, url, question, context_tokens).await
        }
        "local_review" => {
            let diff = arguments.get("diff").and_then(|v| v.as_str()).unwrap_or("");
            handlers::handle_review(client, logger, diff, context_tokens).await
        }
        "local_draft" => {
            let task = arguments.get("task").and_then(|v| v.as_str()).unwrap_or("");
            let context = arguments.get("context").and_then(|v| v.as_str());
            handlers::handle_draft(client, logger, task, context, context_tokens).await
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
    fn test_tool_definitions_has_eight_tools() {
        let defs = tool_definitions();
        let tools = defs["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 8);
    }

    #[test]
    fn test_tool_definitions_names() {
        let defs = tool_definitions();
        let names: Vec<&str> = defs["tools"].as_array().unwrap()
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        assert!(names.contains(&"local_summarize"));
        assert!(names.contains(&"local_pass_fail"));
        assert!(names.contains(&"local_explain"));
        assert!(names.contains(&"local_ask"));
        assert!(names.contains(&"local_web_fetch"));
        assert!(names.contains(&"local_review"));
        assert!(names.contains(&"local_draft"));
        assert!(names.contains(&"local_status"));
    }

    #[test]
    fn test_tool_definitions_has_pass_fail() {
        let defs = tool_definitions();
        let names: Vec<&str> = defs["tools"].as_array().unwrap()
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        assert!(names.contains(&"local_pass_fail"));
    }

    #[test]
    fn test_all_tools_have_input_schema() {
        let defs = tool_definitions();
        for tool in defs["tools"].as_array().unwrap() {
            assert!(tool.get("inputSchema").is_some(), "Tool {} missing inputSchema", tool["name"]);
        }
    }

    #[test]
    fn test_all_tools_except_status_require_context_tokens() {
        let defs = tool_definitions();
        for tool in defs["tools"].as_array().unwrap() {
            let name = tool["name"].as_str().unwrap();
            let required = tool["inputSchema"]["required"].as_array().unwrap();
            let required_strs: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
            if name == "local_status" {
                assert!(!required_strs.contains(&"context_tokens"),
                    "local_status should NOT require context_tokens");
            } else {
                assert!(required_strs.contains(&"context_tokens"),
                    "Tool {} should require context_tokens", name);
            }
        }
    }

    #[test]
    fn test_all_tools_except_status_have_context_tokens_property() {
        let defs = tool_definitions();
        for tool in defs["tools"].as_array().unwrap() {
            let name = tool["name"].as_str().unwrap();
            let props = tool["inputSchema"]["properties"].as_object().unwrap();
            if name == "local_status" {
                assert!(!props.contains_key("context_tokens"),
                    "local_status should NOT have context_tokens property");
            } else {
                assert!(props.contains_key("context_tokens"),
                    "Tool {} should have context_tokens property", name);
                assert_eq!(props["context_tokens"]["type"], "integer",
                    "Tool {} context_tokens should be integer", name);
            }
        }
    }
}
