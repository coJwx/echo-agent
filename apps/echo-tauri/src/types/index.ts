// 与 Rust 侧 events::StreamPayload / state::SessionMeta 保持同步

export interface SessionMeta {
  id: string;
  title: string;
  model: string;
  system_prompt: string;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface CreateSessionInput {
  title: string;
  model: string;
  system_prompt: string;
  temperature?: number | null;
  max_tokens?: number | null;
}

export interface ProviderModel {
  name: string;
  provider?: string | null;
  base_url?: string | null;
  api_key: string;
  model?: string | null;
}

export interface ProviderGroup {
  name: string;
  models: ProviderModel[];
}

export interface ProviderConfig {
  path: string;
  supported_providers: string[];
  providers: ProviderGroup[];
  models: ProviderModel[];
}

export interface ProviderModelInput {
  name: string;
  provider?: string | null;
  base_url?: string | null;
  api_key: string;
  model?: string | null;
}

// snake_case 与 Rust #[serde(rename_all = "snake_case")] 对齐
export type StreamPayload =
  | { kind: "token"; delta: string }
  | { kind: "think_start" }
  | { kind: "think_end"; prompt_tokens: number; completion_tokens: number }
  | { kind: "tool_call"; tool_call_id: string; name: string; args: unknown }
  | { kind: "tool_result"; tool_call_id: string; name: string; output: string }
  | { kind: "tool_error"; tool_call_id: string; name: string; error: string }
  | { kind: "tool_stream"; tool_call_id: string; name: string; payload: unknown }
  | { kind: "guard_triggered"; guard: string; blocked: boolean }
  | { kind: "memory_recalled"; count: number }
  | {
      kind: "context_compressed";
      before_count: number;
      after_count: number;
      before_tokens: number;
      after_tokens: number;
    }
  | { kind: "chart"; spec: unknown }
  | { kind: "error"; source: string; message: string }
  | {
      kind: "safety_notice";
      action: string;
      reason: string;
      risk: string;
      permission: string;
    }
  | {
      kind: "parameter_error";
      tool: string;
      parameter: string;
      expected: string;
      got: string;
    }
  | { kind: "final_answer"; text: string }
  | { kind: "cancelled" }
  | {
      kind: "done";
      elapsed_ms: number;
      ok: boolean;
      error: string | null;
    };

// 前端正在活跃使用的消息
export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  thinkingContent?: string;
  thinkingActive?: boolean;
  toolCalls?: ToolCallTrace[];
  thinkTokens?: { prompt: number; completion: number };
  status: "pending" | "streaming" | "done" | "error";
  elapsedMs?: number;
  error?: string;
}

// 后端 agent_history 命令返回的"精简版"消息
// 字段对齐 Rust 侧 HistoryMessage，只保留前端渲染所需的最小集
export interface HistoryMessage {
  id: string; // 由 (role, idx) 拼接出的稳定 id
  role: "user" | "assistant" | "system" | "tool" | "custom";
  content: string; // 文本主体
  tool_call_id?: string | null;
  name?: string | null;
  thinkingContent?: string;
  thinking_content?: string;
  toolCalls?: ToolCallTrace[];
  tool_calls?: ToolCallTrace[];
  thinkTokens?: { prompt: number; completion: number };
  think_tokens?: { prompt: number; completion: number };
  status: "done" | "streaming" | "error" | "pending";
  elapsedMs?: number;
  elapsed_ms?: number;
}

export interface ToolCallTrace {
  id?: string;
  toolCallId?: string;
  tool_call_id?: string;
  name: string;
  args: unknown;
  result?: string;
  error?: string;
  startedAt?: number;
}

export interface DebugChatTrace {
  events: StreamPayload[];
  history: HistoryMessage[];
  elapsed_ms: number;
  ok: boolean;
  error: string | null;
}
