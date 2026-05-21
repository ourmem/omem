use std::env;

#[derive(Debug, Clone)]
pub struct OmemConfig {
    pub port: u16,
    pub log_level: String,
    pub s3_bucket: String,
    pub oss_bucket: String,
    pub embed_provider: String,
    pub llm_provider: String,
    pub llm_api_key: String,
    pub llm_model: String,
    pub llm_base_url: String,
    pub embed_api_key: String,
    pub embed_base_url: String,
    pub embed_model: String,
    pub embed_dim: usize,
    pub embed_timeout_secs: u64,
    /// Per-user sharing rate limit (operations per minute). 0 = disabled.
    pub share_rate_per_min: u32,
}

impl Default for OmemConfig {
    fn default() -> Self {
        Self {
            port: 8080,
            log_level: "info".to_string(),
            s3_bucket: String::new(),
            oss_bucket: String::new(),
            embed_provider: "noop".to_string(),
            llm_provider: String::new(),
            llm_api_key: String::new(),
            llm_model: "gpt-4o-mini".to_string(),
            llm_base_url: "https://api.openai.com".to_string(),
            embed_api_key: String::new(),
            embed_base_url: String::new(),
            embed_model: String::new(),
            embed_dim: 1024,
            embed_timeout_secs: 10,
            share_rate_per_min: 0,
        }
    }
}

impl OmemConfig {
    pub fn from_env() -> Self {
        let defaults = Self::default();
        Self {
            port: env::var("OMEM_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(defaults.port),
            log_level: env::var("OMEM_LOG_LEVEL").unwrap_or(defaults.log_level),
            s3_bucket: env::var("OMEM_S3_BUCKET").unwrap_or(defaults.s3_bucket),
            oss_bucket: env::var("OMEM_OSS_BUCKET").unwrap_or(defaults.oss_bucket),
            embed_provider: env::var("OMEM_EMBED_PROVIDER").unwrap_or(defaults.embed_provider),
            llm_provider: env::var("OMEM_LLM_PROVIDER").unwrap_or(defaults.llm_provider),
            llm_api_key: env::var("OMEM_LLM_API_KEY").unwrap_or(defaults.llm_api_key),
            llm_model: env::var("OMEM_LLM_MODEL").unwrap_or(defaults.llm_model),
            llm_base_url: env::var("OMEM_LLM_BASE_URL").unwrap_or(defaults.llm_base_url),
            embed_api_key: env::var("OMEM_EMBED_API_KEY").unwrap_or(defaults.embed_api_key),
            embed_base_url: env::var("OMEM_EMBED_BASE_URL").unwrap_or(defaults.embed_base_url),
            embed_model: env::var("OMEM_EMBED_MODEL").unwrap_or(defaults.embed_model),
            embed_dim: env::var("OMEM_EMBED_DIM")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(defaults.embed_dim),
            embed_timeout_secs: env::var("OMEM_EMBED_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(defaults.embed_timeout_secs),
            share_rate_per_min: env::var("OMEM_SHARE_RATE_PER_MIN")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(defaults.share_rate_per_min),
        }
    }

    pub fn store_uri(&self) -> String {
        if !self.oss_bucket.is_empty() {
            format!("oss://{}/omem", self.oss_bucket)
        } else if !self.s3_bucket.is_empty() {
            format!("s3://{}/omem", self.s3_bucket)
        } else {
            "./omem-data".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_are_valid() {
        let config = OmemConfig::default();
        assert_eq!(config.port, 8080);
        assert_eq!(config.embed_provider, "noop");
        assert_eq!(config.llm_model, "gpt-4o-mini");
        assert_eq!(config.log_level, "info");
    }
}
