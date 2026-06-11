import { FormEvent, useEffect, useMemo, useState } from "react";
import {
  Bot,
  BrainCircuit,
  CheckCircle2,
  CircleOff,
  Cloud,
  Eye,
  EyeOff,
  Moon,
  Pencil,
  Plus,
  Server,
  Sparkles,
  Trash2,
  type LucideIcon,
} from "lucide-react";
import { api } from "../../api";
import type {
  ProviderConfig,
  ProviderModel,
  ProviderModelInput,
} from "../../types";

const providerLabels: Record<string, string> = {
  openai: "OpenAI",
  anthropic: "Anthropic",
  google: "Google",
  gemini: "Google",
  azure: "Azure OpenAI",
  azure_openai: "Azure OpenAI",
  dashscope: "阿里云百炼",
  qwen: "阿里云百炼",
  zhipu: "智谱 AI",
  glm: "智谱 AI",
  deepseek: "DeepSeek",
  moonshot: "Moonshot",
  kimi: "Kimi",
  ollama: "Ollama",
  custom: "自定义服务商",
};

const providerIcons: Record<string, LucideIcon> = {
  openai: Sparkles,
  anthropic: BrainCircuit,
  google: Bot,
  gemini: Bot,
  azure: Cloud,
  azure_openai: Cloud,
  dashscope: Cloud,
  qwen: Cloud,
  zhipu: BrainCircuit,
  glm: BrainCircuit,
  deepseek: Server,
  moonshot: Moon,
  kimi: Moon,
  ollama: Server,
  custom: Plus,
};

const defaultApiKeyByProvider: Record<string, string> = {
  openai: "${OPENAI_API_KEY}",
  anthropic: "${ANTHROPIC_API_KEY}",
  deepseek: "${DEEPSEEK_API_KEY}",
  dashscope: "${DASHSCOPE_API_KEY}",
  qwen: "${DASHSCOPE_API_KEY}",
  moonshot: "${MOONSHOT_API_KEY}",
  kimi: "${MOONSHOT_API_KEY}",
  zhipu: "${ZHIPU_API_KEY}",
  glm: "${ZHIPU_API_KEY}",
  ollama: "ollama",
  gemini: "${GEMINI_API_KEY}",
  google: "${GEMINI_API_KEY}",
};

const defaultProviderKeys = [
  "openai",
  "anthropic",
  "gemini",
  "zhipu",
  "deepseek",
  "moonshot",
  "dashscope",
  // "azure_openai",
  // "ollama"
];

const emptyForm: ProviderModelInput = {
  name: "",
  provider: "openai",
  base_url: "",
  api_key: "${OPENAI_API_KEY}",
  model: "",
};

type ProviderRow = {
  key: string;
  label: string;
  models: ProviderModel[];
  connected: boolean;
};

export default function ProviderConfigView() {
  const [config, setConfig] = useState<ProviderConfig | null>(null);
  const [form, setForm] = useState<ProviderModelInput>(emptyForm);
  const [selectedProvider, setSelectedProvider] = useState("openai");
  const [selectedModelName, setSelectedModelName] = useState("");
  const [providerEnabled, setProviderEnabled] = useState(false);
  const [showApiKey, setShowApiKey] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .getProviderConfig()
      .then((next) => {
        if (!cancelled) {
          setConfig(next);
          const first = buildProviderRows(next)[0];
          if (first) selectProvider(first.key, next);
        }
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const providerRows = useMemo(
    () => (config ? buildProviderRows(config) : []),
    [config],
  );
  const currentProviderLabel =
    providerRows.find((provider) => provider.key === selectedProvider)?.label ??
    providerLabel(selectedProvider);
  const currentModels =
    providerRows.find((provider) => provider.key === selectedProvider)?.models ??
    [];
  const addingModel = selectedModelName === "";
  const existingNames = useMemo(
    () => new Set(config?.models.map((model) => model.name) ?? []),
    [config],
  );

  function selectProvider(provider: string, source = config) {
    setSelectedProvider(provider);
    setMessage(null);
    setError(null);
    const models =
      buildProviderRows(source).find((row) => row.key === provider)?.models ?? [];
    const firstModel = models[0];
    setProviderEnabled(models.length > 0);
    if (firstModel) {
      setSelectedModelName(firstModel.name);
      setForm(modelToForm(firstModel, provider));
      return;
    }
    setSelectedModelName("");
    setForm(blankFormForProvider(provider));
  }

  function selectModel(modelName: string) {
    const model = currentModels.find((item) => item.name === modelName);
    setSelectedModelName(modelName);
    setForm(model ? modelToForm(model, selectedProvider) : blankFormForProvider(selectedProvider));
  }

  function startAddProvider() {
    setSelectedProvider("custom");
    setSelectedModelName("");
    setMessage(null);
    setError(null);
    setProviderEnabled(false);
    setForm(blankFormForProvider("custom"));
  }

  function startAddModel() {
    setSelectedModelName("");
    setMessage(null);
    setError(null);
    setForm(blankFormForProvider(selectedProvider));
  }

  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    setSaving(true);
    setError(null);
    setMessage(null);
    try {
      const provider = form.provider === "custom" ? null : form.provider;
      const modelName = form.name.trim();
      const next = await api.saveProviderModel({
        name: modelName,
        provider,
        base_url: form.base_url?.trim() || null,
        api_key: form.api_key.trim(),
        model: form.model?.trim() || null,
      });
      setConfig(next);
      setSelectedProvider(form.provider ?? "custom");
      setSelectedModelName(modelName);
      setMessage(existingNames.has(modelName) ? "模型已更新" : "模型已新增");
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="flex-1 min-h-0 overflow-hidden bg-bg-base">
      <div className="grid h-full min-h-0 grid-cols-[360px_minmax(0,1fr)]">
        <aside className="flex min-h-0 flex-col border-r border-[#2a2a2a] bg-[#171717]">
          <div className="px-4 pb-3 pt-5">
            <h1 className="text-[18px] font-semibold text-ink-primary">
              模型提供商
            </h1>
          </div>

          <div className="min-h-0 flex-1 space-y-2 overflow-auto px-3 pb-4">
            {loading ? (
              <div className="rounded-lg border border-border-subtle bg-bg-card/70 p-4 text-[14px] text-ink-secondary">
                正在加载 Provider 配置...
              </div>
            ) : (
              providerRows.map((provider) => (
                <button
                  key={provider.key}
                  className={
                    "group flex min-h-[58px] w-full items-center gap-3 rounded-md border px-3 text-left transition " +
                    (selectedProvider === provider.key
                      ? "border-brand bg-brand-soft"
                      : "border-transparent bg-transparent hover:bg-bg-hover")
                  }
                  onClick={() => selectProvider(provider.key)}
                >
                  <ProviderIcon provider={provider.key} />
                  <span className="min-w-0 flex-1 truncate text-[15px] font-medium text-ink-primary">
                    {provider.label}
                  </span>
                  <ConnectionStatus connected={provider.connected} />
                </button>
              ))
            )}
          </div>
        </aside>

        <main className="flex min-h-0 flex-col overflow-auto px-10 py-8">
          <header className="mb-10 flex items-start justify-between gap-4">
            <h2 className="text-[26px] font-semibold text-ink-primary">
              模型配置
            </h2>
            <button
              type="button"
              className="flex items-center gap-2 rounded-lg bg-brand px-4 py-2 text-[14px] font-medium text-white shadow-lg shadow-brand/20 transition hover:bg-brand-hover"
              onClick={startAddProvider}
            >
              <Plus className="h-4 w-4" strokeWidth={2} />
              <span>新增供应商</span>
            </button>
          </header>

          {(error || message) && (
            <div
              className={
                "mb-4 max-w-[1180px] rounded-md border px-3 py-2 text-[13px] " +
                (error
                  ? "border-red-500/40 bg-red-500/10 text-red-200"
                  : "border-emerald-500/40 bg-emerald-500/10 text-emerald-200")
              }
            >
              {error ?? message}
            </div>
          )}

          <form
            className="flex w-full max-w-[1180px] flex-col rounded-xl border border-border-subtle bg-bg-panel p-6 shadow-2xl shadow-black/20"
            onSubmit={handleSubmit}
          >
            <div className="mb-8 flex items-center justify-between gap-4">
              <div className="flex min-w-0 items-center gap-3">
                <h3 className="truncate text-[24px] font-semibold text-ink-primary">
                  {currentProviderLabel}
                </h3>
                <button
                  type="button"
                  className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-ink-secondary transition hover:bg-white/5 hover:text-ink-primary"
                  aria-label="编辑服务商名称"
                >
                  <Pencil className="h-4 w-4" strokeWidth={2} />
                </button>
              </div>
              <button
                type="button"
                className={
                  "relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition " +
                  (providerEnabled ? "bg-brand" : "bg-bg-hover")
                }
                onClick={() => setProviderEnabled((enabled) => !enabled)}
                aria-pressed={providerEnabled}
                aria-label={providerEnabled ? "停用服务商" : "启用服务商"}
              >
                <span
                  className={
                    "h-5 w-5 rounded-full bg-white shadow transition " +
                    (providerEnabled ? "translate-x-5" : "translate-x-0.5")
                  }
                />
              </button>
            </div>

            <Field label="API Base URL">
              <input
                className="input-base h-11 w-full"
                value={form.base_url ?? ""}
                onChange={(event) =>
                  setForm((current) => ({
                    ...current,
                    base_url: event.target.value,
                  }))
                }
                placeholder="留空时使用服务商默认地址"
              />
            </Field>

            <Field label="API Key">
              <div className="relative">
                <input
                  className="input-base h-11 w-full pr-12 font-mono"
                  type={showApiKey ? "text" : "password"}
                  value={form.api_key}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      api_key: event.target.value,
                    }))
                  }
                  placeholder="${OPENAI_API_KEY}"
                  required
                />
                <button
                  type="button"
                  className="absolute right-3 top-1/2 flex h-7 w-7 -translate-y-1/2 items-center justify-center rounded text-ink-secondary transition hover:bg-white/5 hover:text-ink-primary"
                  onClick={() => setShowApiKey((current) => !current)}
                  aria-label={showApiKey ? "隐藏 API Key" : "显示 API Key"}
                >
                  {showApiKey ? (
                    <EyeOff className="h-4 w-4" strokeWidth={2} />
                  ) : (
                    <Eye className="h-4 w-4" strokeWidth={2} />
                  )}
                </button>
              </div>
            </Field>

            <div className="mt-5">
              <span className="mb-3 block text-[14px] font-medium text-ink-secondary">
                模型选择
              </span>
              <div className="max-h-[300px] overflow-auto rounded-md border border-border-subtle bg-bg-card p-2">
                {currentModels.length > 0 ? (
                  currentModels.map((model) => (
                    <button
                      key={model.name}
                      type="button"
                      className={
                        "flex h-[58px] w-full items-center gap-3 rounded px-3 text-left transition " +
                        (selectedModelName === model.name
                          ? "bg-brand-soft"
                          : "hover:bg-bg-hover")
                      }
                      onClick={() => selectModel(model.name)}
                    >
                      <span
                        className={
                          "flex h-4 w-4 shrink-0 items-center justify-center rounded-full border " +
                          (selectedModelName === model.name
                            ? "border-brand"
                            : "border-ink-secondary")
                        }
                      >
                        {selectedModelName === model.name && (
                          <span className="h-2 w-2 rounded-full bg-brand" />
                        )}
                      </span>
                      <span className="min-w-0 flex-1 truncate text-[15px] font-medium text-ink-primary">
                        {model.name}
                      </span>
                      <span
                        className="flex h-8 w-8 shrink-0 items-center justify-center text-ink-secondary"
                        aria-hidden="true"
                      >
                        <Trash2 className="h-4 w-4" strokeWidth={2} />
                      </span>
                    </button>
                  ))
                ) : (
                  <div className="px-3 py-8 text-[14px] text-ink-secondary">
                    暂未配置 model
                  </div>
                )}
              </div>
            </div>

            <div className="mt-3 flex gap-3">
              <input
                className="input-base h-11 min-w-0 flex-1"
                value={selectedModelName ? "" : form.name}
                onChange={(event) =>
                  setForm((current) => ({
                    ...current,
                    name: event.target.value,
                    model: event.target.value,
                  }))
                }
                onFocus={startAddModel}
                placeholder="输入模型名称，如 Qwen/Qwen3.5-397B-A17B"
              />
              <button
                className="flex h-11 w-14 shrink-0 items-center justify-center rounded-md bg-brand text-white transition hover:bg-brand-hover disabled:opacity-60"
                disabled={saving || !addingModel || !form.name.trim()}
                aria-label={saving ? "保存中" : "新增模型"}
              >
                <Plus className="h-5 w-5" strokeWidth={2} />
              </button>
            </div>

            <div className="mt-6 border-t border-border-subtle pt-5">
              <button
                className="rounded-md bg-brand px-5 py-2 text-[14px] font-medium text-white shadow-lg shadow-brand/20 transition hover:bg-brand-hover disabled:opacity-60"
                disabled={saving || !form.name.trim()}
              >
                {saving ? "保存中..." : "保存配置"}
              </button>
            </div>
          </form>
        </main>
      </div>
    </div>
  );
}

function buildProviderRows(config: ProviderConfig | null): ProviderRow[] {
  const providerMap = new Map<string, ProviderModel[]>();
  const displayNameMap = new Map<string, string>();
  if (config) {
    for (const group of config.providers) {
      providerMap.set(group.name, group.models);
      displayNameMap.set(group.name, group.display_name || group.name);
    }
  }

  const keys = new Set<string>(config?.supported_providers ?? defaultProviderKeys);
  if (config) {
    for (const group of config.providers) {
      keys.add(group.name);
    }
  }
  keys.delete("custom");
  keys.add("custom");

  return Array.from(keys)
    .map((key) => ({
      key,
      label: displayNameMap.get(key) ?? providerLabel(key),
      models: providerMap.get(key) ?? [],
      connected: (providerMap.get(key)?.length ?? 0) > 0,
    }))
    .sort((a, b) => Number(b.connected) - Number(a.connected));
}

function blankFormForProvider(provider: string): ProviderModelInput {
  return {
    ...emptyForm,
    provider,
    api_key: defaultApiKeyByProvider[provider] ?? "",
    base_url: "",
    model: "",
    name: "",
  };
}

function modelToForm(model: ProviderModel, provider: string): ProviderModelInput {
  return {
    name: model.name,
    provider,
    base_url: model.base_url ?? "",
    api_key: model.api_key,
    model: model.model ?? "",
  };
}

function ProviderIcon({ provider }: { provider: string }) {
  const Icon = providerIcons[provider] ?? Server;
  return (
    <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-border-subtle bg-[#141b28] text-xs font-semibold text-brand">
      <Icon className="h-4 w-4" strokeWidth={2} />
    </span>
  );
}

function ConnectionStatus({ connected }: { connected: boolean }) {
  const Icon = connected ? CheckCircle2 : CircleOff;
  return (
    <span className="flex shrink-0 items-center gap-2 text-[12px] text-ink-secondary">
      <Icon
        className={connected ? "h-3.5 w-3.5 text-emerald-400" : "h-3.5 w-3.5 text-slate-500"}
        strokeWidth={2}
      />
      {connected ? "已连接" : "未连接"}
    </span>
  );
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <label className="block">
      <span className="mb-1.5 block text-[12px] text-ink-secondary">{label}</span>
      {children}
    </label>
  );
}

function providerLabel(provider: string) {
  return providerLabels[provider] ?? provider;
}
