use std::string::ToString;
use derivative::Derivative;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::{dialogue};
use crate::conversation::{ConversationManager, DEFAULT_CACHE_DURATION, DEFAULT_CHAR_LIMIT};


const DEFAULT_MODEL: &str = "gpt-3.5-turbo";


#[derive(Debug, Default, Serialize, Deserialize)]
pub struct User {
    id: i64,
    business_id: String,
    openai: Openai,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Openai {
    config: OpenaiConfig,
    spent_tokens: i64,
}

#[derive(Debug, Clone, Derivative, Serialize, Deserialize)]
#[derivative(Default)]
pub struct OpenaiConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    api_key: Option<String>,
    #[derivative(Default(value = "DEFAULT_MODEL.to_string()"))]
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt: Option<String>,
    #[derivative(Default(value = "4_000"))]
    max_message_length: i32,
    #[derivative(Default(value = "1_000_000"))]
    max_total_tokens_spent: i64,
    #[derivative(Default(value = "300"))]
    max_tokens: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    conversation: Option<Conversation>,
}


#[derive(Debug, Clone, Derivative, Serialize, Deserialize)]
struct Conversation {
    cache_duration: Option<i64>,
    char_limit: Option<usize>,
}

impl User {
    pub fn new(id: i64, business_id: String, openai: Openai) -> Self {
        Self { id, business_id, openai}
    }

    pub fn get_config(&self) -> OpenaiConfig {
        self.openai.config.clone()
    }

    pub fn get_openai_spent_tokens(&self) -> i64 {
        self.openai.spent_tokens
    }

    pub fn get_id(&self) -> i64 {
        self.id
    }
}

impl From<Value> for Openai {
    fn from(value: Value) -> Self {
        serde_json::from_value(value).unwrap()
    }
}

impl From<Openai> for Value {
    fn from(value: Openai) -> Self {
        serde_json::to_value(value).unwrap()
    }
}

impl OpenaiConfig {
    pub fn get_api_key(&self) -> Option<String> {
        Some(match self.api_key.clone()? {
            key if key.len() > 5 => format!("...{}", &key[key.len() - 5..]),
            key => key,
        })
    }

    pub fn get_real_api_key(&self) -> Option<String> {
        self.api_key.clone()
    }

    pub fn get_model(&self) -> &str {
        &self.model
    }

    pub fn get_prompt(&self) -> Option<&str> {
        self.prompt.as_deref()
    }

    pub fn get_max_message_length(&self) -> i32 {
        self.max_message_length
    }

    pub fn get_max_total_tokens_spent(&self) -> i64 {
        self.max_total_tokens_spent
    }

    pub fn get_max_tokens(&self) -> u16 {
        self.max_tokens
    }

    pub async fn set_api_key(&mut self, api_key: String) -> Result<(), &'static str> {
        match Self::validate_api_key(&api_key).await {
            Ok(true) => {
                self.api_key = Some(api_key);
                Ok(())
            }
            Ok(false) => Err("Invalid API key"),
            Err(e) => {
                log::error!("Failed check API key:\n{e:?}");
                Err("Failed check API key")
            }
        }
    }

    pub fn set_model(&mut self, model: String) -> Result<(), &'static str> {
        match model.as_str() {
            "gpt-3.5-turbo" | "gpt-4-turbo" | "gpt-4o" => {
                self.model = model;
                Ok(())
            }
            _ => Err("Invalid model. Allowed values are: gpt-3.5-turbo, gpt-4-turbo, gpt-4o"),
        }
    }

    pub fn set_prompt(&mut self, prompt: String) -> Result<(), &'static str> {
        if prompt.len() <= 4000 {
            self.prompt = Some(prompt);
            Ok(())
        } else {
            Err("Prompt is too long. Maximum length is 4000 characters")
        }
    }

    pub fn set_max_message_length(&mut self, length: i32) -> Result<(), &'static str> {
        if length <= 4000 {
            self.max_message_length = length;
            Ok(())
        } else {
            Err("Max message length is too long. Maximum is 4000")
        }
    }

    pub fn set_max_total_tokens_spent(&mut self, tokens: i64) {
        self.max_total_tokens_spent = tokens;
    }

    pub fn set_max_tokens(&mut self, tokens: u16) {
        self.max_tokens = tokens;
    }

    async fn validate_api_key(api_key: &str) -> Result<bool, String> {
        dialogue::is_api_key_valid(api_key).await
    }

    pub fn set_cache_duration(&mut self, value: i64) -> Result<(), &'static str> {
        if value >= 3_600 {
            return Err("Maximum duration is 3,600 seconds")
        }

        let conversation = self.conversation.get_or_insert_with(
            || Conversation { cache_duration: None, char_limit: None }
        );
        conversation.cache_duration = Some(value);
        Ok(())
    }

    pub fn set_char_limit(&mut self, value: usize) -> Result<(), &'static str> {
        if value >= 10_000 {
            return Err("Maximum limit is 10,000 symbols")
        }
        let conversation = self.conversation.get_or_insert_with(
            || Conversation { cache_duration: None, char_limit: None }
        );
        conversation.char_limit = Some(value);
        Ok(())
    }

    pub fn get_cache_duration(&self) -> i64 {
        if let Some(conversation) = &self.conversation {
            if let Some(value) = conversation.cache_duration {
                return value;
            }
        };
        DEFAULT_CACHE_DURATION
    }
    pub fn get_char_limit(&self) -> usize {
        if let Some(conversation) = &self.conversation {
            if let Some(value) = conversation.char_limit {
                return value;
            }
        };
        DEFAULT_CHAR_LIMIT
    }

    pub async fn get_manager(&self) -> ConversationManager {
        let mut manager = ConversationManager::default().await;
        if let Some(conversation) = self.conversation.clone() {
            if let Some(cache_duration) = conversation.cache_duration {
                manager = manager.with_cache_duration(cache_duration)
            }
            if let Some(char_limit) = conversation.char_limit {
                manager = manager.with_char_limit(char_limit)
            }
        }
        manager
    }
}

impl Openai {
    pub fn with_config(mut self, config: OpenaiConfig) -> Self {
        self.config = config;
        self
    }
    pub fn with_spent_tokens(mut self, spent_tokens: i64) -> Self {
        self.spent_tokens = spent_tokens;
        self
    }
}
