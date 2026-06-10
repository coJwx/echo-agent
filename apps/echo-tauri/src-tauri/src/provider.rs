use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_yaml_ng::{Mapping, Value};

use crate::error::{AppError, AppResult};
use echo_integration::providers::{ProviderFactory, config::provider_base_url};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderModel {
    pub name: String,
    pub provider: Option<String>,
    pub base_url: Option<String>,
    pub api_key: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderGroup {
    pub name: String,
    pub models: Vec<ProviderModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderConfig {
    pub path: String,
    pub supported_providers: Vec<String>,
    pub providers: Vec<ProviderGroup>,
    pub models: Vec<ProviderModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderModelInput {
    pub name: String,
    pub provider: Option<String>,
    pub base_url: Option<String>,
    pub api_key: String,
    pub model: Option<String>,
}

#[tauri::command]
pub async fn provider_config_get() -> AppResult<ProviderConfig> {
    let path = models_config_path();
    read_provider_config_from_path(&path)
}

#[tauri::command]
pub async fn provider_model_save(input: ProviderModelInput) -> AppResult<ProviderConfig> {
    let path = models_config_path();
    save_model_to_path(&path, input)?;
    read_provider_config_from_path(&path)
}

fn read_provider_config_from_path(path: &Path) -> AppResult<ProviderConfig> {
    let root = read_yaml_root(path)?;
    let mut models = Vec::new();

    if let Some(mapping) = root
        .as_mapping()
        .and_then(|root| root.get(Value::String("models".to_string())))
        .and_then(Value::as_mapping)
    {
        for (key, value) in mapping {
            let Some(name) = key.as_str() else {
                continue;
            };
            let Some(entry) = value.as_mapping() else {
                continue;
            };
            let base_url = string_field(entry, "base_url");
            let provider = string_field(entry, "provider").or_else(|| infer_provider(&base_url));
            models.push(ProviderModel {
                name: name.to_string(),
                provider,
                base_url,
                api_key: string_field(entry, "api_key").unwrap_or_default(),
                model: string_field(entry, "model"),
            });
        }
    }

    models.sort_by(|a, b| a.name.cmp(&b.name));

    let mut grouped: BTreeMap<String, Vec<ProviderModel>> = BTreeMap::new();
    for model in &models {
        grouped
            .entry(
                model
                    .provider
                    .clone()
                    .unwrap_or_else(|| "custom".to_string()),
            )
            .or_default()
            .push(model.clone());
    }

    let providers = grouped
        .into_iter()
        .map(|(name, models)| ProviderGroup { name, models })
        .collect();

    Ok(ProviderConfig {
        path: path.display().to_string(),
        supported_providers: supported_provider_names(),
        providers,
        models,
    })
}

fn save_model_to_path(path: &Path, input: ProviderModelInput) -> AppResult<()> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::Config("model key 不能为空".to_string()));
    }

    let api_key = input.api_key.trim();
    if api_key.is_empty() {
        return Err(AppError::Config("api_key 不能为空".to_string()));
    }

    let provider = input
        .provider
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let base_url = input
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .or_else(|| provider.and_then(provider_base_url).map(str::to_string));

    if provider.is_none() && base_url.is_none() {
        return Err(AppError::Config(
            "请选择 provider 或填写 base_url".to_string(),
        ));
    }

    let mut root = read_yaml_root(path)?;
    if !root.is_mapping() {
        root = Value::Mapping(Mapping::new());
    }
    let root_map = root.as_mapping_mut().expect("root mapping");
    let models_key = Value::String("models".to_string());
    if !root_map.contains_key(&models_key) {
        root_map.insert(models_key.clone(), Value::Mapping(Mapping::new()));
    }

    let models = root_map
        .get_mut(&models_key)
        .and_then(Value::as_mapping_mut)
        .ok_or_else(|| AppError::Config("models 必须是 YAML map".to_string()))?;

    let mut entry = Mapping::new();
    if let Some(provider) = provider {
        entry.insert(
            Value::String("provider".to_string()),
            Value::String(provider.to_string()),
        );
    }
    if let Some(base_url) = base_url {
        entry.insert(
            Value::String("base_url".to_string()),
            Value::String(base_url),
        );
    }
    entry.insert(
        Value::String("api_key".to_string()),
        Value::String(api_key.to_string()),
    );
    if let Some(model) = input
        .model
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        entry.insert(
            Value::String("model".to_string()),
            Value::String(model.to_string()),
        );
    }

    models.insert(Value::String(name.to_string()), Value::Mapping(entry));

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AppError::PersistenceWrite(e.to_string()))?;
    }
    let content = serde_yaml_ng::to_string(&root)
        .map_err(|e| AppError::Config(format!("序列化模型配置失败: {e}")))?;
    std::fs::write(path, content).map_err(|e| AppError::PersistenceWrite(e.to_string()))
}

fn read_yaml_root(path: &Path) -> AppResult<Value> {
    if !path.exists() {
        return Ok(Value::Mapping(Mapping::new()));
    }
    let content =
        std::fs::read_to_string(path).map_err(|e| AppError::PersistenceRead(e.to_string()))?;
    if content.trim().is_empty() {
        return Ok(Value::Mapping(Mapping::new()));
    }
    serde_yaml_ng::from_str(&content)
        .map_err(|e| AppError::Config(format!("解析模型配置失败: {e}")))
}

fn string_field(mapping: &Mapping, key: &str) -> Option<String> {
    mapping
        .get(Value::String(key.to_string()))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn models_config_path() -> PathBuf {
    if let Ok(path) = std::env::var("ECHO_AGENT_MODELS_CONFIG")
        && !path.trim().is_empty()
    {
        return PathBuf::from(path);
    }

    let cwd_path = PathBuf::from("echo-agent-models.yaml");
    if cwd_path.is_file() {
        return cwd_path;
    }

    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        return PathBuf::from(home).join(".echo-agent").join("models.yaml");
    }

    cwd_path
}

fn supported_provider_names() -> Vec<String> {
    let mut providers = ProviderFactory::supported_providers()
        .iter()
        .map(|provider| (*provider).to_string())
        .collect::<Vec<_>>();
    providers.push("custom".to_string());
    providers.sort();
    providers.dedup();
    providers
}

fn infer_provider(base_url: &Option<String>) -> Option<String> {
    let lower = base_url.as_ref()?.to_ascii_lowercase();
    let provider = if lower.contains("openai.com") {
        "openai"
    } else if lower.contains("anthropic.com") {
        "anthropic"
    } else if lower.contains("deepseek.com") {
        "deepseek"
    } else if lower.contains("dashscope.aliyuncs.com") {
        "dashscope"
    } else if lower.contains("moonshot.cn") {
        "moonshot"
    } else if lower.contains("bigmodel.cn") {
        "zhipu"
    } else if lower.contains("localhost:11434") || lower.contains("ollama") {
        "ollama"
    } else if lower.contains("generativelanguage.googleapis.com") {
        "gemini"
    } else {
        "custom"
    };
    Some(provider.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_provider_groups_from_models_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("echo-agent-models.yaml");
        std::fs::write(
            &path,
            r#"
models:
  glm-5.1:
    provider: zhipu
    api_key: ${ZHIPU_API_KEY}
    model: glm-5.1
  qwen-plus:
    provider: dashscope
    api_key: ${DASHSCOPE_API_KEY}
"#,
        )
        .unwrap();

        let config = read_provider_config_from_path(&path).unwrap();

        assert_eq!(config.models.len(), 2);
        assert_eq!(config.providers.len(), 2);
        assert_eq!(config.providers[0].name, "dashscope");
        assert_eq!(config.providers[0].models[0].name, "qwen-plus");
        assert_eq!(config.providers[1].name, "zhipu");
        assert_eq!(
            config.providers[1].models[0].model.as_deref(),
            Some("glm-5.1")
        );
    }

    #[test]
    fn saves_new_model_to_models_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("echo-agent-models.yaml");
        std::fs::write(&path, "models: {}\n").unwrap();

        save_model_to_path(
            &path,
            ProviderModelInput {
                name: "gpt-4o-mini".to_string(),
                provider: Some("openai".to_string()),
                base_url: None,
                api_key: "${OPENAI_API_KEY}".to_string(),
                model: Some("gpt-4o-mini".to_string()),
            },
        )
        .unwrap();

        let config = read_provider_config_from_path(&path).unwrap();
        let model = config
            .models
            .iter()
            .find(|m| m.name == "gpt-4o-mini")
            .unwrap();
        assert_eq!(model.provider.as_deref(), Some("openai"));
        assert_eq!(
            model.base_url.as_deref(),
            Some("https://api.openai.com/v1/chat/completions")
        );
        assert_eq!(model.api_key, "${OPENAI_API_KEY}");
    }

    #[test]
    fn provider_options_are_sourced_from_integration_factory() {
        assert!(ProviderFactory::supported_providers().contains(&"openai"));
        assert_eq!(
            provider_base_url("openai"),
            Some("https://api.openai.com/v1/chat/completions")
        );
    }
}
