import { useEffect, useRef } from "react";
import { Check, Cpu, LoaderCircle, Sparkles, TriangleAlert } from "lucide-react";
import ReactMarkdown from "react-markdown";
import type { ChatMessage } from "../../types";
import type { ReactNode } from "react";

interface Props {
  messages: ChatMessage[];
  placeholder: boolean;
}

export default function MessageList({ messages, placeholder }: Props) {
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [messages]);

  if (placeholder) {
    return (
      <div className="flex flex-1 items-center justify-center text-[14px] text-ink-secondary">
        <div className="rounded-lg border border-[#2f2f2f] bg-[#1c1c1c] px-5 py-4">
          请在左侧选择一个会话，或新建一个。
        </div>
      </div>
    );
  }
  if (messages.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center text-[14px] text-ink-secondary">
        <div className="rounded-lg border border-[#2f2f2f] bg-[#1c1c1c] px-5 py-4">
          发送第一条消息开始对话。
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto px-3 lg:px-5 py-4">
      <div className="mx-auto flex w-full max-w-[940px] flex-col gap-3">
      {messages.map((m) => (
        <MessageBubble key={m.id} m={m} />
      ))}
      <div ref={bottomRef} />
      </div>
    </div>
  );
}

function MessageBubble({ m }: { m: ChatMessage }) {
  const isUser = m.role === "user";
  const isSystem = m.role === "system";
  const toolCalls = m.toolCalls ?? [];
  const hasToolCalls = toolCalls.length > 0;
  const hasContent = m.content.trim().length > 0;
  const hasThinking =
    Boolean(m.thinkingActive) ||
    Boolean(m.thinkingContent?.trim()) ||
    Boolean(m.thinkTokens);
  const hasError = m.status === "error" && Boolean(m.error);

  if (!isSystem && !hasToolCalls && !hasContent && !hasThinking && !hasError && m.status !== "streaming") {
    return null;
  }

  if (isUser) {
    return (
      <div className="flex justify-end">
        <div className="max-w-[85%] lg:max-w-[72%] rounded-2xl bg-[#303030] px-3.5 py-2.5 text-[15px] leading-[1.55] text-white shadow-sm">
          <ReactMarkdown>{m.content}</ReactMarkdown>
        </div>
      </div>
    );
  }

  return (
    <div className="flex justify-start">
      <div
        className={
          "w-full overflow-hidden rounded-xl border " +
          (isSystem
            ? "border-[#2f2f2f] bg-[#1b1b1b] text-ink-secondary"
            : "border-[#2c2c2c] bg-[#181818] text-ink-primary shadow-sm")
        }
      >
        {isSystem ? null : <div className="flex items-center justify-between border-b border-[#242424] px-3 py-2">
          <div className="flex items-center gap-2">
            <span
              className={
                "flex h-5 w-5 items-center justify-center rounded-md text-xs text-white " +
                (isSystem ? "bg-ink-muted" : "bg-[#303030]")
              }
            >
              <Sparkles className="h-3.5 w-3.5" strokeWidth={2} />
            </span>
            <span className="text-[14px] font-semibold">
              {cardTitle(m, hasToolCalls)}
            </span>
          </div>
          {m.status === "streaming" && (
            <span className="text-[12px] text-brand">正在生成...</span>
          )}
        </div>}
        <div className="px-3 py-2.5">
          {isSystem ? (
            <details className="whitespace-pre-wrap break-words leading-[1.55]">
              <summary className="mb-1 cursor-pointer font-medium text-ink-secondary flex">
                <span className="text-accent-green">系统提示</span>
              </summary>
              <div className="text-[15px] text-ink-primary">{m.content}</div>
            </details>
          ) : m.segments?.length ? (
            <SegmentedAssistantView message={m} />
          ) : (
            <>
              {hasThinking && (
                <ThinkingView
                  content={m.thinkingContent ?? ""}
                  active={Boolean(m.thinkingActive)}
                  tokens={m.thinkTokens}
                />
              )}
              {hasToolCalls && <ToolCallsView calls={toolCalls} />}
              {(hasContent || m.status === "streaming") && (
                <div className="readable-prose prose prose-invert max-w-none whitespace-pre-wrap break-words">
                  <ReactMarkdown>{m.content || "…"}</ReactMarkdown>
                </div>
              )}
              {m.status === "error" && m.error && (
                <div className="mt-2 rounded border border-accent-red/30 bg-accent-red/10 px-2 py-1 text-[12px] text-accent-red">
                  {m.error}
                </div>
              )}
              {m.status === "done" && m.elapsedMs != null && (
                <div className="mt-2 text-[12px] text-ink-secondary">
                  {(m.elapsedMs / 1000).toFixed(2)}s
                  {m.thinkTokens &&
                    ` · ${m.thinkTokens.prompt}+${m.thinkTokens.completion} tok`}
                </div>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function SegmentedAssistantView({ message }: { message: ChatMessage }) {
  const segments = message.segments ?? [];
  const nodes: ReactNode[] = [];

  for (let i = 0; i < segments.length; i++) {
    const segment = segments[i];
    if (segment.kind === "tool_call") {
      const run = [];
      let j = i;
      while (j < segments.length && segments[j].kind === "tool_call") {
        const toolSegment = segments[j];
        if (toolSegment.kind === "tool_call") {
          run.push(toolSegment.call);
        }
        j++;
      }

      if (run.length > 2) {
        nodes.push(<ToolCallsView key={`tool-run-${i}`} calls={run} />);
      } else {
        for (let k = 0; k < run.length; k++) {
          nodes.push(<ToolCallItem key={`tool-${i + k}`} call={run[k]} />);
        }
      }

      i = j - 1;
      continue;
    }

    if (segment.kind === "thinking") {
      nodes.push(
        <ThinkingView
          key={`thinking-${i}`}
          content={segment.content}
          active={Boolean(message.thinkingActive) && i === segments.length - 1}
          tokens={segment.tokens}
        />,
      );
      continue;
    }

    nodes.push(
      <div
        key={`text-${i}`}
        className="readable-prose prose prose-invert max-w-none whitespace-pre-wrap break-words"
      >
        <ReactMarkdown>{segment.content}</ReactMarkdown>
      </div>,
    );
  }

  return (
    <>
      {nodes}
      {message.status === "streaming" && nodes.length === 0 && (
        <div className="readable-prose prose prose-invert max-w-none whitespace-pre-wrap break-words">
          <ReactMarkdown>…</ReactMarkdown>
        </div>
      )}
      {message.status === "error" && message.error && (
        <div className="mt-2 rounded border border-accent-red/30 bg-accent-red/10 px-2 py-1 text-[12px] text-accent-red">
          {message.error}
        </div>
      )}
    </>
  );
}

function ThinkingView({
  content,
  active,
  tokens,
}: {
  content: string;
  active: boolean;
  tokens?: ChatMessage["thinkTokens"];
}) {
  const hasContent = content.trim().length > 0;
  return (
    <details className="mb-2 rounded-lg border border-[#303030] bg-[#202020] text-[12px] text-ink-secondary">
      <summary className="flex cursor-pointer items-center gap-2 px-2.5 py-1.5">
        <Cpu className="h-3.5 w-3.5 animate-pulse text-ink-secondary" strokeWidth={2} />
        <span className="text-ink-primary">思考过程</span>
        {tokens && (
          <span className="text-[11px]">
            {tokens.prompt}+{tokens.completion} tok
          </span>
        )}
      </summary>
      {(hasContent || active) && (
        <div className="whitespace-pre-wrap break-words px-2.5 pb-2.5 text-[13px] leading-[1.5] text-ink-primary">
          <ReactMarkdown>{hasContent ? content : "..."}</ReactMarkdown>
        </div>
      )}
    </details>
  );
}

function ToolCallsView({ calls }: { calls: NonNullable<ChatMessage["toolCalls"]> }) {
  return (
    <details className="mb-2 rounded-lg border border-[#2f2f2f] bg-[#1d1d1d] text-[12px]" open>
      <summary className="flex cursor-pointer items-center gap-2 border-b border-[#2a2a2a] px-2.5 py-1.5 text-ink-primary">
        <span>调用工具</span>
        <span className="text-[11px]">{calls.length}</span>
      </summary>
      <div className="space-y-1.5 px-2.5 pb-2.5 pt-2">
        {calls.map((call, i) => (
          <ToolCallItem key={i} call={call} />
        ))}
      </div>
    </details>
  );
}

function ToolCallItem({ call }: { call: NonNullable<ChatMessage["toolCalls"]>[number] }) {
  const state = call.error ? "error" : call.result ? "ok" : "running";
  const badge =
    state === "error"
      ? "bg-accent-red/10 text-accent-red border-accent-red/25"
      : state === "ok"
        ? "bg-[#202820] text-accent-green border-[#2a3f2d]"
        : "bg-accent-amber/10 text-accent-amber border-accent-amber/25";
  return (
    <details className={"mb-2 rounded-lg border bg-bg-card/60 text-[12px] " + badge}>
      <summary className="flex cursor-pointer items-center gap-2 px-2.5 py-1.5">
        <ToolStateIcon state={state} />
        <span className="font-medium">{call.name}</span>
        <span className="ml-auto rounded bg-white/5 px-2 py-0.5 text-[11px]">
          JSON
        </span>
      </summary>
      <div className="space-y-1 px-2.5 pb-2.5 pt-1 font-mono text-[12px] leading-[1.45]">
        <div>
          <span className="text-ink-secondary">args:</span>{" "}
          <code className="break-all">{JSON.stringify(call.args)}</code>
        </div>
        {call.result && (
          <div>
            <span className="text-ink-secondary">result:</span>{" "}
            <code className="break-all whitespace-pre-wrap">
              {call.result.length > 400
                ? call.result.slice(0, 400) + "..."
                : call.result}
            </code>
          </div>
        )}
        {call.error && (
          <div>
            <span className="text-ink-secondary">error:</span>{" "}
            <code className="break-all">{call.error}</code>
          </div>
        )}
      </div>
    </details>
  );
}

function ToolStateIcon({ state }: { state: "error" | "ok" | "running" }) {
  if (state === "running") {
    return <LoaderCircle className="h-3.5 w-3.5 animate-spin" strokeWidth={2} />;
  }
  if (state === "ok") {
    return <Check className="h-3.5 w-3.5" strokeWidth={2} />;
  }
  return <TriangleAlert className="h-3.5 w-3.5" strokeWidth={2} />;
}

function cardTitle(m: ChatMessage, hasToolCalls: boolean) {
  if (hasToolCalls && !m.content.trim()) return "工具调用";
  if (hasToolCalls) return "AI";
  return "AI";
}
