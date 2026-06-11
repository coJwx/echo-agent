use echo_app_core::error::AppResult;
use echo_app_core::provider::{self as core_provider, ProviderConfig, ProviderModelInput};

#[tauri::command]
pub async fn provider_config_get() -> AppResult<ProviderConfig> {
    core_provider::provider_config_get().await
}

#[tauri::command]
pub async fn provider_model_save(input: ProviderModelInput) -> AppResult<ProviderConfig> {
    core_provider::provider_model_save(input).await
}
