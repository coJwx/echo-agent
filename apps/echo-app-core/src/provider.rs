use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_yaml_ng::{Mapping, Value};

use crate::error::{AppError, AppResult};
use echo_core::utils::paths::root_agent_dir;
use echo_integration::providers::{ProviderFactory, config::provider_base_url};

const API_KEY_MASK: &str = "******";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderModel {
    pub name: String,
    pub provider: Option<String>,
    pub provider_name: Option<String>,
    pub base_url: Option<String>,
    pub api_key: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderGroup {
    pub name: String,
    pub display_name: String,
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

pub async fn provider_config_get() -> AppResult<ProviderConfig> {
    let path = models_config_path();
    read_provider_config_from_path(&path)
}

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
        .and_then(|root| root.get(Value::String("providers".to_string())))
        .and_then(Value::as_mapping)
    {
        for (provider_key, value) in mapping {
            let Some(provider_id) = provider_key.as_str() else {
                continue;
            };
            let Some(provider_entry) = value.as_mapping() else {
                continue;
            };
            let base_url = string_field(provider_entry, "baseUrl")
                .or_else(|| string_field(provider_entry, "base_url"));
            let provider_name =
                string_field(provider_entry, "name").unwrap_or_else(|| provider_id.to_string());

            if let Some(model_entries) = provider_entry
                .get(Value::String("models".to_string()))
                .and_then(Value::as_sequence)
            {
                for model_entry in model_entries {
                    let Some(model_entry) = model_entry.as_mapping() else {
                        continue;
                    };
                    let Some(model_id) = string_field(model_entry, "id") else {
                        continue;
                    };
                    models.push(ProviderModel {
                        name: model_id,
                        provider: Some(provider_id.to_string()),
                        provider_name: Some(provider_name.clone()),
                        base_url: base_url.clone(),
                        api_key: API_KEY_MASK.to_string(),
                        model: string_field(model_entry, "name"),
                    });
                }
            }
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
        .map(|(name, models)| {
            let display_name = models
                .first()
                .and_then(|model| model.provider_name.clone())
                .unwrap_or_else(|| name.clone());
            ProviderGroup {
                name,
                display_name,
                models,
            }
        })
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

    let requested_api_key = input.api_key.trim();

    let provider = input
        .provider
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("custom");
    let base_url = input
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .or_else(|| provider_base_url(provider).map(provider_base_from_endpoint));

    let Some(base_url) = base_url else {
        return Err(AppError::Config(
            "请选择 provider 或填写 base_url".to_string(),
        ));
    };

    let mut root = read_yaml_root(path)?;
    if !root.is_mapping() {
        root = Value::Mapping(Mapping::new());
    }
    let root_map = root.as_mapping_mut().expect("root mapping");
    let providers_key = Value::String("providers".to_string());
    if !root_map.contains_key(&providers_key) {
        root_map.insert(providers_key.clone(), Value::Mapping(Mapping::new()));
    }

    let providers = root_map
        .get_mut(&providers_key)
        .and_then(Value::as_mapping_mut)
        .ok_or_else(|| AppError::Config("providers 必须是 YAML map".to_string()))?;

    let provider_key = Value::String(provider.to_string());
    let existing_api_key = providers
        .get(&provider_key)
        .and_then(Value::as_mapping)
        .and_then(|provider| {
            string_field(provider, "apiKey").or_else(|| string_field(provider, "api_key"))
        });
    let api_key = if requested_api_key == API_KEY_MASK {
        existing_api_key
            .as_deref()
            .ok_or_else(|| AppError::Config("api_key 不能为空".to_string()))?
    } else if requested_api_key.is_empty() {
        return Err(AppError::Config("api_key 不能为空".to_string()));
    } else {
        requested_api_key
    };

    if !providers.contains_key(&provider_key) {
        let mut provider_entry = Mapping::new();
        provider_entry.insert(
            Value::String("name".to_string()),
            Value::String(provider.to_string()),
        );
        provider_entry.insert(
            Value::String("baseUrl".to_string()),
            Value::String(base_url.clone()),
        );
        provider_entry.insert(
            Value::String("apiKey".to_string()),
            Value::String(api_key.to_string()),
        );
        provider_entry.insert(
            Value::String("api".to_string()),
            Value::String("openai-completions".to_string()),
        );
        provider_entry.insert(
            Value::String("auth".to_string()),
            Value::String("apiKey".to_string()),
        );
        provider_entry.insert(
            Value::String("models".to_string()),
            Value::Sequence(Vec::new()),
        );
        providers.insert(provider_key.clone(), Value::Mapping(provider_entry));
    }

    let provider_entry = providers
        .get_mut(&provider_key)
        .and_then(Value::as_mapping_mut)
        .ok_or_else(|| AppError::Config("provider 必须是 YAML map".to_string()))?;

    provider_entry.insert(
        Value::String("baseUrl".to_string()),
        Value::String(base_url.clone()),
    );
    provider_entry.insert(
        Value::String("apiKey".to_string()),
        Value::String(api_key.to_string()),
    );
    provider_entry
        .entry(Value::String("api".to_string()))
        .or_insert_with(|| Value::String("openai-completions".to_string()));
    provider_entry
        .entry(Value::String("auth".to_string()))
        .or_insert_with(|| Value::String("apiKey".to_string()));
    provider_entry
        .entry(Value::String("models".to_string()))
        .or_insert_with(|| Value::Sequence(Vec::new()));

    let mut entry = Mapping::new();
    entry.insert(
        Value::String("id".to_string()),
        Value::String(name.to_string()),
    );
    entry.insert(
        Value::String("name".to_string()),
        Value::String(
            input
                .model
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .unwrap_or(name)
                .to_string(),
        ),
    );
    if let Some(models) = provider_entry
        .get_mut(Value::String("models".to_string()))
        .and_then(Value::as_sequence_mut)
    {
        models.retain(|model| {
            model
                .as_mapping()
                .and_then(|mapping| string_field(mapping, "id"))
                .is_none_or(|id| id != name)
        });
        models.push(Value::Mapping(entry));
    } else {
        return Err(AppError::Config("provider.models 必须是 YAML list".to_string()));
    }

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
    root_agent_dir().join("models.yaml")
}

fn supported_provider_names() -> Vec<String> {
    let mut providers = ProviderFactory::supported_providers()
        .iter()
        .filter_map(|provider| canonical_provider_key(provider))
        .map(str::to_string)
        .collect::<Vec<_>>();
    providers.push("custom".to_string());
    providers.sort();
    providers.dedup();
    providers
}

fn provider_base_from_endpoint(endpoint: &str) -> String {
    endpoint
        .trim_end_matches('/')
        .strip_suffix("/chat/completions")
        .unwrap_or(endpoint.trim_end_matches('/'))
        .to_string()
}

fn canonical_provider_key(provider: &str) -> Option<&'static str> {
    match provider {
        "openai" => Some("openai"),
        "anthropic" => Some("anthropic"),
        "deepseek" => Some("deepseek"),
        "dashscope" | "qwen" | "aliyun" => Some("dashscope"),
        "moonshot" | "kimi" => Some("moonshot"),
        "zhipu" | "glm" => Some("zhipu"),
        "ollama" => Some("ollama"),
        "gemini" | "google" => Some("gemini"),
        "azure" | "azure_openai" => Some("azure_openai"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_provider_groups_from_models_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("models.yaml");
        std::fs::write(
            &path,
            r#"
providers:
  zhipu:
    name: 智谱
    baseUrl: https://open.bigmodel.cn/api/paas/v4
    apiKey: ${ZHIPU_API_KEY}
    api: openai-completions
    auth: apiKey
    models:
      - id: glm-5.1
        name: glm-5.1
  dashscope:
    name: 通义千问
    baseUrl: https://dashscope.aliyuncs.com/compatible-mode/v1
    apiKey: ${DASHSCOPE_API_KEY}
    api: openai-completions
    auth: apiKey
    models:
      - id: qwen-plus
        name: qwen-plus
"#,
        )
        .unwrap();

        let config = read_provider_config_from_path(&path).unwrap();

        assert_eq!(config.models.len(), 2);
        assert_eq!(config.providers.len(), 2);
        assert_eq!(config.providers[0].name, "dashscope");
        assert_eq!(config.providers[0].display_name, "通义千问");
        assert_eq!(config.providers[0].models[0].name, "qwen-plus");
        assert_eq!(config.providers[0].models[0].api_key, API_KEY_MASK);
        assert_eq!(config.providers[1].name, "zhipu");
        assert_eq!(config.providers[1].display_name, "智谱");
        assert_eq!(
            config.providers[1].models[0].model.as_deref(),
            Some("glm-5.1")
        );
    }

    #[test]
    fn saves_new_model_to_models_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("models.yaml");
        std::fs::write(&path, "providers: {}\n").unwrap();

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
            Some("https://api.openai.com/v1")
        );
        assert_eq!(model.api_key, API_KEY_MASK);

        let yaml = std::fs::read_to_string(&path).unwrap();
        assert!(yaml.contains("providers:"));
        assert!(yaml.contains("baseUrl: https://api.openai.com/v1"));
        assert!(yaml.contains("apiKey: ${OPENAI_API_KEY}"));
        assert!(yaml.contains("api: openai-completions"));
        assert!(yaml.contains("auth: apiKey"));
        assert!(yaml.contains("id: gpt-4o-mini"));
    }

    #[test]
    fn saves_masked_api_key_without_overwriting_existing_secret() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("models.yaml");
        std::fs::write(
            &path,
            r#"
providers:
  hsapi:
    name: 火山引擎
    baseUrl: https://ark.cn-beijing.volces.com/api/coding/v3
    apiKey: ark-secret
    api: openai-completions
    auth: apiKey
    models:
      - id: glm-5.1
        name: glm-5.1
"#,
        )
        .unwrap();

        save_model_to_path(
            &path,
            ProviderModelInput {
                name: "glm-5.1".to_string(),
                provider: Some("hsapi".to_string()),
                base_url: Some("https://ark.cn-beijing.volces.com/api/coding/v3".to_string()),
                api_key: API_KEY_MASK.to_string(),
                model: Some("glm-5.1".to_string()),
            },
        )
        .unwrap();

        let yaml = std::fs::read_to_string(&path).unwrap();
        assert!(yaml.contains("apiKey: ark-secret"));

        let config = read_provider_config_from_path(&path).unwrap();
        assert_eq!(config.providers[0].name, "hsapi");
        assert_eq!(config.providers[0].display_name, "火山引擎");
        assert_eq!(config.providers[0].models[0].api_key, API_KEY_MASK);
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
