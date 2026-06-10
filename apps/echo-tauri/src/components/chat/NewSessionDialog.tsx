import { useState } from "react";
import { api } from "../../api";
import type { SessionMeta } from "../../types";

interface Props {
  onCancel: () => void;
  onCreated: (meta: SessionMeta) => void;
}

// Tools' descriptions, parameter schemas, and TypedTool impls are injected
// automatically by echo_agent via `ToolManager::get_openai_tools()` into the
// `tools` field of each LLM call. The model uses native function calling,
// so the system prompt does NOT need to list tools manually.
const DEFAULT_PROMPT = [
  "You are Echo, a helpful AI assistant running inside a Tauri desktop app on Windows.",
  "The working directory is the echo-agent workspace root.",
  "Be concise and accurate. If a tool returns empty, the directory or match set may simply be empty; do not assume the tool is broken.",
].join("\n");

export default function NewSessionDialog({ onCancel, onCreated }: Props) {
  const [title, setTitle] = useState("新对话");
  const [model, setModel] = useState("glm-5.1");
  const [systemPrompt, setSystemPrompt] = useState(DEFAULT_PROMPT);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const create = async () => {
    setBusy(true);
    setErr(null);
    try {
      const meta = await api.createSession({
        title: title.trim() || "新对话",
        model: model.trim(),
        system_prompt: systemPrompt,
      });
      onCreated(meta);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4"
      onClick={onCancel}
    >
      <div
        className="card w-full max-w-md p-6"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="text-base font-semibold text-ink-primary mb-4">
          新建对话
        </h3>

        <Field label="名称">
          <input
            className="input-base w-full"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="例如：架构讨论"
            autoFocus
          />
        </Field>

        <Field
          label="模型"
          hint="必须在 echo-agent-models.yaml 中存在；默认 glm-5.1"
        >
          <input
            className="input-base w-full font-mono"
            value={model}
            onChange={(e) => setModel(e.target.value)}
          />
        </Field>

        <Field label="System Prompt">
          <textarea
            className="input-base w-full h-24 resize-none"
            value={systemPrompt}
            onChange={(e) => setSystemPrompt(e.target.value)}
          />
        </Field>

        {err && (
          <div className="text-xs text-accent-red bg-accent-red/10 border border-accent-red/30 rounded px-2 py-2 mb-3">
            {err}
          </div>
        )}

        <div className="flex justify-end gap-2">
          <button onClick={onCancel} className="btn-ghost" disabled={busy}>
            取消
          </button>
          <button
            onClick={create}
            className="btn-primary disabled:opacity-50"
            disabled={busy}
          >
            {busy ? "创建中…" : "创建"}
          </button>
        </div>
      </div>
    </div>
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="mb-4">
      <label className="block text-xs text-ink-secondary mb-1.5">
        {label}
        {hint && <span className="text-ink-muted ml-2">· {hint}</span>}
      </label>
      {children}
    </div>
  );
}
