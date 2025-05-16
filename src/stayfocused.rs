use arrrg::CommandLine;

use claudius::{
    Anthropic, ContentBlock, JsonSchema, KnownModel, MessageContentBlock, MessageCreateParams,
    MessageParam, MessageParamContent, MessageRole, Model, StopReason, SystemPrompt, TextBlock,
    ToolChoice, ToolParam, ToolResultBlock, ToolUnionParam, ToolUseBlock,
};

#[derive(
    Clone, Debug, Eq, PartialEq, arrrg_derive::CommandLine, serde::Deserialize, serde::Serialize,
)]
pub struct StayFocusedOptions {
    #[arrrg(optional, "Which histfile to tail for context.")]
    pub histfile: String,
    #[arrrg(optional, "How many lines to tail and maintain from the histfile.")]
    pub tail: usize,
}

impl Default for StayFocusedOptions {
    fn default() -> Self {
        Self {
            histfile: ".histfile".to_string(),
            tail: 10,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct History {
    pub tail: Vec<String>,
    pub last_index: usize,
    pub primary_objective: Option<String>,
    pub side_quests: Option<Vec<String>>,
    pub options: StayFocusedOptions,
}

impl History {
    pub fn as_content_block(&self) -> MessageContentBlock {
        let mut objectives = String::new();
        if let Some(po) = self.primary_objective.as_ref() {
            objectives += "Primary objective: ";
            objectives += po;
            objectives.push('\n');
        }
        if let Some(side_quests) = &self.side_quests {
            if !side_quests.is_empty() {
                objectives += "Side quests:\n";
                for quest in side_quests {
                    objectives += "- ";
                    objectives += quest;
                    objectives.push('\n');
                }
            }
        }
        let histfile = "<histfile>
"
        .to_string()
            + &self.tail.join("\n")
            + "
</histfile>
";
        MessageContentBlock::Text(TextBlock::new(objectives + &histfile))
    }
}

#[derive(Clone, Debug, claudius_derive::JsonSchema, serde::Deserialize, serde::Serialize)]
struct SetPrimaryTaskArgs {
    task: String,
}

#[derive(Clone, Debug, claudius_derive::JsonSchema, serde::Deserialize, serde::Serialize)]
struct SetSideQuestsArgs {
    side_quests: Vec<String>,
}

async fn process_tool_call(tool_use: &ToolUseBlock, history: &mut History) -> String {
    eprintln!("{}", serde_json::to_string_pretty(&tool_use.input).unwrap());
    match tool_use.name.as_str() {
        "set_primary_task" => {
            if let Ok(args) = serde_json::from_value::<SetPrimaryTaskArgs>(tool_use.input.clone()) {
                history.primary_objective = Some(args.task.clone());
                format!("Primary task set to: {}", args.task)
            } else {
                "Error: Invalid arguments for set_primary_task".to_string()
            }
        }
        "set_side_quests" => {
            if let Ok(args) = serde_json::from_value::<SetSideQuestsArgs>(tool_use.input.clone()) {
                history.side_quests = Some(args.side_quests.clone());
                format!("Side quests set: {:?}", args.side_quests)
            } else {
                "Error: Invalid arguments for set_side_quests".to_string()
            }
        }
        _ => {
            format!("Error: Unknown tool '{}'", tool_use.name)
        }
    }
}

pub async fn main() {
    let state_path = std::env::var("STAYFOCUSED_STATE").unwrap_or_else(|_| {
        eprintln!("You should set STAYFOCUSED_STATE in your environment.");
        std::process::exit(13);
    });
    let (options, free) =
        StayFocusedOptions::from_command_line_relaxed("USAGE: stayfocused HISTFILE ...");
    if !free.is_empty() {
        eprintln!("command takes no positional arguments");
        std::process::exit(13);
    }
    let actions = std::fs::read_to_string(&options.histfile).unwrap_or_else(|err| {
        panic!(
            "should be able to read {} but the best laid plans of mice ... {err}",
            options.histfile
        )
    });
    let mut actions = actions
        .split_terminator('\n')
        .rev()
        .take(options.tail)
        .collect::<Vec<_>>();
    actions.reverse();
    let history = std::fs::read_to_string(&state_path).unwrap_or_else(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            let default = History {
                tail: vec![],
                last_index: 0,
                primary_objective: None,
                side_quests: None,
                options: options.clone(),
            };
            serde_json::to_string(&default).expect("all types serializable, no error")
        } else {
            panic!("could not read history file from {state_path}: {err}")
        }
    });
    let mut history: History = serde_json::from_str(&history).expect("history should be JSON");
    history.tail.extend(actions.into_iter().map(String::from));
    history.tail = history
        .tail
        .split_off(history.tail.len().saturating_sub(history.options.tail));

    let client = Anthropic::new(None).expect("Should be able to instantiate an Anthropic client");
    let message = MessageParam::new(
        MessageParamContent::String(
            "<histfile>\n".to_string() + &history.tail.join("\n") + "\n</histfile>",
        ),
        MessageRole::User,
    );
    let mut messages = vec![message];

    for _ in 0..3 {
        let params = MessageCreateParams {
            max_tokens: 1000,
            messages: messages.clone(),
            model: Model::Known(KnownModel::Claude37SonnetLatest),
            system: Some(SystemPrompt::String(
                include_str!("stayfocused.md").to_string(),
            )),
            stream: false,
            thinking: None,
            tool_choice: Some(ToolChoice::Any {
                disable_parallel_tool_use: Some(false),
            }),
            tools: Some(vec![
                ToolUnionParam::CustomTool(ToolParam {
                    name: "nop".to_string(),
                    cache_control: None,
                    description: Some("Do nothing.".to_string()),
                    input_schema: SetSideQuestsArgs::json_schema(),
                }),
                ToolUnionParam::CustomTool(ToolParam {
                    name: "set_primary_task".to_string(),
                    cache_control: None,
                    description: Some("Set the user's primary task.".to_string()),
                    input_schema: SetPrimaryTaskArgs::json_schema(),
                }),
                ToolUnionParam::CustomTool(ToolParam {
                    name: "set_side_quests".to_string(),
                    cache_control: None,
                    description: Some("Set the user's side quests.".to_string()),
                    input_schema: SetSideQuestsArgs::json_schema(),
                }),
            ]),
            metadata: None,
            stop_sequences: None,
            temperature: None,
            top_k: None,
            top_p: None,
        };

        let response = client.send(params).await.unwrap();
        println!("{response:?}");

        // Add assistant response to messages
        let assistant_content: Vec<MessageContentBlock> = response
            .content
            .iter()
            .map(|cb| match cb {
                ContentBlock::Text(text) => MessageContentBlock::Text(text.clone()),
                ContentBlock::ToolUse(tool_use) => MessageContentBlock::ToolUse(tool_use.clone()),
                ContentBlock::ServerToolUse(_) => panic!("Unexpected ServerToolUse"),
                ContentBlock::Thinking(thinking) => {
                    MessageContentBlock::Text(TextBlock::new(thinking.thinking.clone()))
                }
                ContentBlock::RedactedThinking(_) => {
                    MessageContentBlock::Text(TextBlock::new("[Thinking was redacted]".to_string()))
                }
            })
            .collect();

        messages.push(MessageParam::new(
            MessageParamContent::Array(assistant_content),
            MessageRole::Assistant,
        ));

        // Check stop reason
        if response.stop_reason != Some(StopReason::ToolUse) {
            println!("Final response: {response:#?}");
            break;
        }

        // Process tool calls
        let mut tool_results = Vec::new();
        for content_block in &response.content {
            if let ContentBlock::ToolUse(tool_use) = content_block {
                let result = process_tool_call(tool_use, &mut history).await;
                tool_results.push(MessageContentBlock::ToolResult(ToolResultBlock {
                    tool_use_id: tool_use.id.clone(),
                    content: Some(claudius::ToolResultBlockContent::String(result)),
                    is_error: None,
                    cache_control: None,
                }));
            }
        }

        // Add tool results to messages
        if !tool_results.is_empty() {
            messages.push(MessageParam::new(
                MessageParamContent::Array(tool_results),
                MessageRole::User,
            ));
        }
    }

    // Save updated history
    let history_json = serde_json::to_string(&history).expect("history should serialize");
    std::fs::write(&state_path, history_json).expect("should be able to write state file");
}
