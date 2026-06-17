// 与 Rust 侧 events::StreamPayload / state::SessionMeta 保持同步

export interface SessionMeta {
  id: string;
  title: string;
  model: string;
  system_prompt: string;
  work_dir?: string | null;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface CreateSessionInput {
  title: string;
  model: string;
  system_prompt: string;
  work_dir?: string | null;
  temperature?: number | null;
  max_tokens?: number | null;
}

export interface ProviderModel {
  name: string;
  provider?: string | null;
  provider_name?: string | null;
  base_url?: string | null;
  api_key: string;
  model?: string | null;
}

export interface ProviderGroup {
  name: string;
  display_name: string;
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

export interface UpdateSessionModelInput {
  model: string;
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
  segments?: ChatMessageSegment[];
  thinkTokens?: { prompt: number; completion: number };
  status: "pending" | "streaming" | "done" | "error";
  elapsedMs?: number;
  error?: string;
}

export type ChatMessageSegment =
  | { kind: "text"; content: string }
  | {
      kind: "thinking";
      content: string;
      tokens?: { prompt: number; completion: number };
    }
  | { kind: "tool_call"; call: ToolCallTrace };

export interface ChatTurn {
  id: string;
  role: "user" | "assistant" | "system";
  segments: ChatMessageSegment[];
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

export interface AccessTokenInfo {
  token: string;
  created_at_ms: number;
  expires_at_ms: number;
  local_url: string;
  network_url?: string | null;
  qr_svg: string;
}

export interface DebugChatTrace {
  events: StreamPayload[];
  history: ChatTurn[];
  elapsed_ms: number;
  ok: boolean;
  error: string | null;
}
