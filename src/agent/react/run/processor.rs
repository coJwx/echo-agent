//! SSE chunk processing — convert raw LLM streaming chunks into AgentEvent sequences.
//!
//! These are static methods (no `&self`) — pure functions that transform chunk data.

use crate::agent::AgentEvent;
use crate::llm::types::{ChatCompletionChunk, FunctionCall, ToolCall as LlmToolCall};
use serde_json::Value;
use std::collections::HashMap;

/// Process streaming response chunk, collect content and return events.
///
/// `in_reasoning` tracks whether reasoning_content is being output (Qwen3/DeepSeek thinking process).
/// ThinkStart is emitted when reasoning_content is first encountered,
/// ThinkEnd is emitted when content or tool_calls is first encountered after reasoning ends.
#[allow(clippy::type_complexity)]
pub(crate) fn process_stream_chunk(
    chunk: &ChatCompletionChunk,
    content_buffer: &mut String,
    reasoning_buffer: &mut String,
    tool_call_map: &mut HashMap<u32, (String, String, String)>,
    in_reasoning: &mut bool,
) -> Vec<AgentEvent> {
    let mut events = Vec::new();

    if let Some(choice) = chunk.choices.first() {
        // Handle reasoning_content (Qwen3/DeepSeek thinking process)
        if let Some(reasoning) = &choice.delta.reasoning_content
            && !reasoning.is_empty()
        {
            if !*in_reasoning {
                *in_reasoning = true;
                events.push(AgentEvent::ThinkStart);
            }
            let delta = if reasoning.starts_with(reasoning_buffer.as_str()) {
                &reasoning[reasoning_buffer.len()..]
            } else {
                reasoning.as_str()
            };
            if !delta.is_empty() {
                reasoning_buffer.push_str(delta);
                events.push(AgentEvent::Token(delta.to_string()));
            }
        }

        // When content is first encountered after reasoning ends, close the thinking block
        if let Some(content) = &choice.delta.content
            && !content.is_empty()
        {
            if *in_reasoning {
                *in_reasoning = false;
                events.push(AgentEvent::ThinkEnd {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                });
            }
            content_buffer.push_str(content);
            events.push(AgentEvent::Token(content.clone()));
        }

        if let Some(delta_calls) = &choice.delta.tool_calls {
            if *in_reasoning {
                *in_reasoning = false;
                events.push(AgentEvent::ThinkEnd {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                });
            }
            for dc in delta_calls {
                let entry = tool_call_map
                    .entry(dc.index)
                    .or_insert_with(|| (String::new(), String::new(), String::new()));
                if let Some(id) = &dc.id
                    && !id.is_empty()
                {
                    entry.0 = id.clone();
                }
                if let Some(f) = &dc.function {
                    if let Some(name) = &f.name
                        && !name.is_empty()
                    {
                        entry.1 = name.clone();
                    }
                    if let Some(args) = &f.arguments {
                        entry.2.push_str(args);
                    }
                }
            }
        }
    }

    events
}

/// Convert the collected tool_call_map into structured tool call lists.
pub(crate) fn build_tool_calls_from_map(
    tool_call_map: &HashMap<u32, (String, String, String)>,
) -> (Vec<LlmToolCall>, Vec<(String, String, Value)>) {
    let mut sorted_indices: Vec<u32> = tool_call_map.keys().cloned().collect();
    sorted_indices.sort();

    let mut msg_tool_calls: Vec<LlmToolCall> = Vec::new();
    let mut steps: Vec<(String, String, Value)> = Vec::new();
    let mut seen_calls = std::collections::HashSet::new();

    for idx in &sorted_indices {
        let (id, name, args_str) = &tool_call_map[idx];
        if !seen_calls.insert((name.clone(), args_str.clone())) {
            tracing::debug!(
                tool_name = %name,
                raw_args = %args_str,
                "Skipping duplicate streaming tool call"
            );
            continue;
        }
        let args: Value = match serde_json::from_str(args_str) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    tool_name = %name,
                    tool_call_id = %id,
                    raw_args = %args_str,
                    error = %e,
                    "Failed to parse streaming tool-call arguments as JSON; skipping this tool call"
                );
                // Skip this tool call — do NOT execute with broken/empty args.
                // The raw args are preserved in msg_tool_calls for the model to
                // see in the response, but no execution step is created.
                msg_tool_calls.push(LlmToolCall {
                    id: id.clone(),
                    call_type: "function".to_string(),
                    function: FunctionCall {
                        name: name.clone(),
                        arguments: args_str.clone(),
                    },
                });
                continue;
            }
        };

        msg_tool_calls.push(LlmToolCall {
            id: id.clone(),
            call_type: "function".to_string(),
            function: FunctionCall {
                name: name.clone(),
                arguments: args_str.clone(),
            },
        });
        steps.push((id.clone(), name.clone(), args));
    }

    (msg_tool_calls, steps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::types::{ChunkChoice, DeltaMessage};

    #[test]
    fn reasoning_content_emits_token_during_thinking_without_content_buffer() {
        let chunk = ChatCompletionChunk {
            id: "chunk-1".to_string(),
            choices: vec![ChunkChoice {
                delta: DeltaMessage {
                    reasoning_content: Some("thinking".to_string()),
                    ..Default::default()
                },
                finish_reason: None,
                index: 0,
            }],
            usage: None,
        };
        let mut content_buffer = String::new();
        let mut reasoning_buffer = String::new();
        let mut tool_call_map = HashMap::new();
        let mut in_reasoning = false;

        let events = process_stream_chunk(
            &chunk,
            &mut content_buffer,
            &mut reasoning_buffer,
            &mut tool_call_map,
            &mut in_reasoning,
        );

        assert!(matches!(events.first(), Some(AgentEvent::ThinkStart)));
        assert!(matches!(
            events.get(1),
            Some(AgentEvent::Token(token)) if token == "thinking"
        ));
        assert!(content_buffer.is_empty());
        assert_eq!(reasoning_buffer, "thinking");
    }

    #[test]
    fn cumulative_reasoning_content_only_emits_new_suffix() {
        let mut content_buffer = String::new();
        let mut reasoning_buffer = String::new();
        let mut tool_call_map = HashMap::new();
        let mut in_reasoning = false;

        let first = reasoning_chunk("think");
        let second = reasoning_chunk("thinking");

        let first_events = process_stream_chunk(
            &first,
            &mut content_buffer,
            &mut reasoning_buffer,
            &mut tool_call_map,
            &mut in_reasoning,
        );
        let second_events = process_stream_chunk(
            &second,
            &mut content_buffer,
            &mut reasoning_buffer,
            &mut tool_call_map,
            &mut in_reasoning,
        );

        assert!(matches!(
            first_events.as_slice(),
            [AgentEvent::ThinkStart, AgentEvent::Token(token)] if token == "think"
        ));
        assert!(matches!(
            second_events.as_slice(),
            [AgentEvent::Token(token)] if token == "ing"
        ));
        assert_eq!(reasoning_buffer, "thinking");
    }

    #[test]
    fn duplicate_tool_calls_with_same_name_and_args_are_collapsed() {
        let mut tool_call_map = HashMap::new();
        tool_call_map.insert(
            0,
            (
                "call-1".to_string(),
                "list_dir".to_string(),
                r#"{"path":"."}"#.to_string(),
            ),
        );
        tool_call_map.insert(
            1,
            (
                "call-2".to_string(),
                "list_dir".to_string(),
                r#"{"path":"."}"#.to_string(),
            ),
        );

        let (msg_tool_calls, steps) = build_tool_calls_from_map(&tool_call_map);

        assert_eq!(msg_tool_calls.len(), 1);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].0, "call-1");
        assert_eq!(steps[0].1, "list_dir");
    }

    #[test]
    fn streaming_tool_call_preserves_provider_id() {
        let mut tool_call_map = HashMap::new();
        tool_call_map.insert(
            0,
            (
                "call_provider_1".to_string(),
                "list_dir".to_string(),
                r#"{"path":"."}"#.to_string(),
            ),
        );

        let (msg_tool_calls, steps) = build_tool_calls_from_map(&tool_call_map);

        assert_eq!(msg_tool_calls[0].id, "call_provider_1");
        assert_eq!(steps[0].0, "call_provider_1");
        assert_eq!(steps[0].1, "list_dir");
    }

    fn reasoning_chunk(reasoning_content: &str) -> ChatCompletionChunk {
        ChatCompletionChunk {
            id: "chunk".to_string(),
            choices: vec![ChunkChoice {
                delta: DeltaMessage {
                    reasoning_content: Some(reasoning_content.to_string()),
                    ..Default::default()
                },
                finish_reason: None,
                index: 0,
            }],
            usage: None,
        }
    }
}
