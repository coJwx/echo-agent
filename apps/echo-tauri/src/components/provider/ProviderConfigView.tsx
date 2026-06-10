import { FormEvent, useEffect, useMemo, useState } from "react";
import {
  Bot,
  BrainCircuit,
  CheckCircle2,
  CircleOff,
  Cloud,
  Moon,
  Plus,
  Server,
  Sparkles,
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
  "google",
  "azure_openai",
  "dashscope",
  "zhipu",
  "deepseek",
  "ollama",
  "custom",
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
  const currentModels =
    providerRows.find((provider) => provider.key === selectedProvider)?.models ??
    [];
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

  function updateProvider(provider: string) {
    setSelectedProvider(provider);
    setSelectedModelName("");
    setForm(blankFormForProvider(provider));
  }

  return (
    <div className="flex-1 min-h-0 overflow-hidden bg-[radial-gradient(circle_at_20%_0%,rgba(72,88,255,0.14),transparent_32%),#0d1118]">
      <div className="flex h-full flex-col p-4">
        <header className="mb-4 flex items-center justify-between">
          <div>
            <h1 className="text-lg font-semibold text-ink-primary">
              Provider 配置
            </h1>
            <p className="mt-0.5 text-[12px] text-ink-secondary">
              {config?.path ?? "正在读取 echo-agent-models.yaml..."}
            </p>
          </div>
          <button
            type="button"
            className="flex items-center gap-2 rounded-lg bg-brand px-3 py-2 text-[14px] font-medium text-white shadow-lg shadow-brand/25 transition hover:bg-brand-hover"
            onClick={startAddProvider}
          >
            <Plus className="h-4 w-4" strokeWidth={2} />
            <span>添加服务商</span>
          </button>
        </header>

        {(error || message) && (
          <div
            className={
              "mb-3 rounded-lg border px-3 py-2 text-[13px] " +
              (error
                ? "border-red-500/40 bg-red-500/10 text-red-200"
                : "border-emerald-500/40 bg-emerald-500/10 text-emerald-200")
            }
          >
            {error ?? message}
          </div>
        )}

        <main className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_390px] overflow-hidden rounded-xl border border-border-strong bg-bg-panel/80 shadow-2xl shadow-black/25 backdrop-blur">
          <section className="min-w-0 overflow-auto p-3">
            <div className="mb-3 flex items-center justify-between">
              <div>
                <h2 className="text-[15px] font-semibold text-ink-primary">
                  API 服务商配置
                </h2>
                <p className="mt-0.5 text-[12px] text-ink-secondary">
                  {providerRows.filter((item) => item.connected).length} 个已连接，{config?.models.length ?? 0} 个模型
                </p>
              </div>
              <span className="rounded-full border border-border-subtle bg-bg-card px-2.5 py-1 text-[12px] text-ink-secondary">
                {loading ? "加载中" : `${providerRows.length} providers`}
              </span>
            </div>

            {loading ? (
              <div className="rounded-lg border border-border-subtle bg-bg-card/70 p-4 text-[14px] text-ink-secondary">
                正在加载 Provider 配置...
              </div>
            ) : (
              <div className="space-y-2">
                {providerRows.map((provider) => (
                  <button
                    key={provider.key}
                    className={
                      "group w-full rounded-lg border p-3 text-left transition " +
                      (selectedProvider === provider.key
                        ? "border-brand bg-brand-soft shadow-[0_0_0_1px_rgba(108,92,231,0.25)_inset]"
                        : "border-border-subtle bg-bg-card/70 hover:border-border-strong hover:bg-bg-hover/70")
                    }
                    onClick={() => selectProvider(provider.key)}
                  >
                    <div className="flex items-center gap-3">
                      <ProviderIcon provider={provider.key} />
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          <span className="truncate font-medium text-ink-primary">
                            {provider.label}
                          </span>
                          {provider.connected && provider.key === "openai" && (
                            <span className="rounded bg-emerald-500/15 px-1.5 py-0.5 text-[10px] text-emerald-300">
                              默认
                            </span>
                          )}
                        </div>
                        <div className="mt-2 flex flex-wrap gap-1.5">
                          {provider.models.length > 0 ? (
                            provider.models.map((model) => (
                              <span
                                key={model.name}
                                className="rounded bg-white/5 px-2 py-1 text-[12px] text-ink-secondary"
                              >
                                {model.name}
                              </span>
                            ))
                          ) : (
                            <span className="text-[12px] text-ink-secondary">
                              暂未配置 model
                            </span>
                          )}
                        </div>
                      </div>
                      <ConnectionStatus connected={provider.connected} />
                    </div>
                  </button>
                ))}
              </div>
            )}
          </section>

          <aside className="flex min-h-0 flex-col border-l border-border-strong bg-[#111620]/90">
            <div className="border-b border-border-subtle px-4 py-3">
              <h2 className="text-[15px] font-semibold text-ink-primary">
                编辑服务商
              </h2>
              <p className="mt-0.5 text-[12px] text-ink-secondary">
                {providerLabel(selectedProvider)} · {currentModels.length || 0} models
              </p>
            </div>

            <form
              className="flex min-h-0 flex-1 flex-col gap-3 overflow-auto px-4 py-3"
              onSubmit={handleSubmit}
            >
              <Field label="服务商名称">
                <select
                  className="input-base h-10 w-full"
                  value={form.provider ?? "custom"}
                  onChange={(event) => updateProvider(event.target.value)}
                >
                  {(config?.supported_providers.length
                    ? config.supported_providers
                    : defaultProviderKeys
                  ).map((provider) => (
                    <option key={provider} value={provider}>
                      {providerLabel(provider)}
                    </option>
                  ))}
                </select>
              </Field>

              {currentModels.length > 0 && (
                <Field label="已配置 Model">
                  <select
                    className="input-base h-10 w-full"
                    value={selectedModelName}
                    onChange={(event) => selectModel(event.target.value)}
                  >
                    {currentModels.map((model) => (
                      <option key={model.name} value={model.name}>
                        {model.name}
                      </option>
                    ))}
                  </select>
                </Field>
              )}

              <Field label="Model Key">
                <input
                  className="input-base h-10 w-full"
                  value={form.name}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      name: event.target.value,
                    }))
                  }
                  placeholder="gpt-4o"
                  required
                />
              </Field>

              <Field label="API 地址">
                <input
                  className="input-base h-10 w-full"
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
                    className="input-base h-10 w-full pr-10 font-mono"
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
                  <span className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-xs text-ink-muted">
                    {form.api_key.startsWith("${") ? "ENV" : "•••"}
                  </span>
                </div>
              </Field>

              <Field label="实际模型名">
                <input
                  className="input-base h-10 w-full"
                  value={form.model ?? ""}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      model: event.target.value,
                    }))
                  }
                  placeholder="默认使用 Model Key"
                />
              </Field>

              <Field label="超时时间（秒）">
                <input
                  className="input-base h-10 w-full"
                  value="60"
                  readOnly
                />
              </Field>

              <div className="mt-1 flex items-center justify-between">
                <span className="text-[13px] text-ink-secondary">启用状态</span>
                <span className="relative inline-flex h-5 w-9 items-center rounded-full bg-brand">
                  <span className="absolute right-0.5 h-4 w-4 rounded-full bg-white shadow" />
                </span>
              </div>

              <div className="mt-auto flex gap-3 pt-4">
                <button
                  type="button"
                  className="flex-1 rounded-lg bg-bg-hover px-3 py-2 text-[14px] text-ink-secondary transition hover:text-ink-primary"
                  onClick={startAddModel}
                >
                  新增模型
                </button>
                <button
                  className="flex-1 rounded-lg bg-brand px-3 py-2 text-[14px] font-medium text-white shadow-lg shadow-brand/20 transition hover:bg-brand-hover disabled:opacity-60"
                  disabled={saving}
                >
                  {saving ? "保存中..." : "保存"}
                </button>
              </div>
            </form>
          </aside>
        </main>
      </div>
    </div>
  );
}

function buildProviderRows(config: ProviderConfig | null): ProviderRow[] {
  const providerMap = new Map<string, ProviderModel[]>();
  if (config) {
    for (const group of config.providers) {
      providerMap.set(group.name, group.models);
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
      label: providerLabel(key),
      models: providerMap.get(key) ?? [],
      connected: (providerMap.get(key)?.length ?? 0) > 0,
    }))
    .sort((a, b) => Number(b.connected) - Number(a.connected) || a.label.localeCompare(b.label));
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
