use std::string::ToString;
use derivative::Derivative;
use serde::{Deserialize, Serialize};
use serde_json::Value;


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

const DEFAULT_MODEL: &str = "gpt-3.5-turbo";

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
}

impl User {
    pub fn new(id: i64, business_id: String, openai: Openai) -> Self {
        Self { id, business_id, openai }
    }

    pub fn get_config(&mut self) -> OpenaiConfig {
        self.openai.config.clone()
    }

    pub fn get_openai_spent_tokens(&mut self) -> i64 {
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
    pub fn get_api_key(&self) -> &str {
        self.api_key.as_deref().unwrap_or("---")
    }

    pub fn get_model(&self) -> &str {
        &self.model
    }

    pub fn get_prompt(&self) -> &str {
        self.prompt.as_deref().unwrap_or("---")
    }

    pub fn get_max_message_length(&self) -> i32 {
        self.max_message_length
    }

    pub fn get_max_total_tokens_spent(&self) -> i64 {
        self.max_total_tokens_spent
    }

    // Setters with validation
    pub fn set_api_key(&mut self, api_key: String) -> Result<(), &'static str> {
        if Self::validate_api_key(&api_key) {
            self.api_key = Some(api_key);
            Ok(())
        } else {
            Err("Invalid API key")
        }
    }

    pub fn set_model(&mut self, model: String) -> Result<(), &'static str> {
        match model.as_str() {
            "gpt-3.5-turbo" | "gpt-4-turbo" | "gpt-4o" => {
                self.model = model;
                Ok(())
            },
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

    // Helper function for API key validation
    fn validate_api_key(api_key: &str) -> bool {
        // Implement your API key validation logic here
        true
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
