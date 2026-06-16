import { useState, type KeyboardEvent } from "react";
import { Paperclip, SendHorizontal } from "lucide-react";

interface Props {
  disabled: boolean;
  onSend: (text: string) => void;
}

export default function MessageInput({ disabled, onSend }: Props) {
  const [text, setText] = useState("");

  const submit = () => {
    const v = text.trim();
    if (!v || disabled) return;
    onSend(v);
    setText("");
  };

  const onKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    // Enter 发送、Shift+Enter 换行 (macOS 同时支持 Cmd+Enter)
    if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      submit();
    }
  };

  return (
    <div className="px-3 pb-3 pt-2 lg:px-5 lg:pb-4">
      <div className="mx-auto flex w-full max-w-[940px] items-end gap-2 rounded-2xl border border-[#343434] bg-[#2a2a2a] px-3 py-2.5 lg:px-4 lg:py-3 shadow-xl shadow-black/25 transition focus-within:border-[#555]">
        <textarea
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={onKey}
          rows={1}
          disabled={disabled}
          placeholder={
            disabled
              ? "请先选择会话或等待生成完成…"
              : "输入消息  (Enter 发送 · Shift+Enter 换行)"
          }
          className="max-h-40 flex-1 resize-none bg-transparent text-[15px] leading-[1.45] text-ink-primary placeholder:text-[#8f8f8f] focus:outline-none"
          style={{ minHeight: "2.5rem" }}
        />
        <button
          type="button"
          className="hidden sm:flex h-8 w-8 items-center justify-center rounded-lg text-ink-secondary transition hover:bg-white/10 hover:text-ink-primary"
          disabled={disabled}
          title="附件"
          aria-label="附件"
        >
          <Paperclip className="h-4 w-4" strokeWidth={2} />
        </button>
        <button
          type="button"
          onClick={submit}
          disabled={disabled || !text.trim()}
          className="flex h-8 w-8 items-center justify-center rounded-full bg-[#d2d2d2] text-[#111] transition hover:bg-white disabled:cursor-not-allowed disabled:opacity-40"
          title="发送"
          aria-label="发送"
        >
          <SendHorizontal className="h-4 w-4" strokeWidth={2} />
        </button>
      </div>
    </div>
  );
}
