use async_openai::Client;
use async_openai::config::OpenAIConfig;
use async_openai::error::OpenAIError;
use async_openai::types::{ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs};
use crate::user::OpenaiConfig;

pub struct ChatResponse {
    pub message: String,
    pub tokens_spent: u32,
}

#[derive(Debug)]
pub enum OpenaiResponseError {
    Openai(OpenAIError),
    Message(String),
}

pub fn get_client(api_key: &str) -> Client<OpenAIConfig> {
    let openai_config = OpenAIConfig::new().with_api_key(api_key);
    Client::with_config(openai_config)
}

pub async fn get_response(config: &OpenaiConfig, message: String) -> Result<ChatResponse, OpenaiResponseError> {
    let client = get_client(
        &config.get_api_key()
            .ok_or_else(|| OpenaiResponseError::Message("I don't know what to answer".to_string()))?
    );

    let mut chat_messages = vec![];
    if let Some(prompt) = config.get_prompt() {
        chat_messages.push(
            ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessageArgs::default()
                .content(prompt)
                .build()
                .unwrap())
        )
    }
    chat_messages.push(
        ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessageArgs::default()
            .content(message)
            .build()
            .unwrap())
    );

    let request = CreateChatCompletionRequestArgs::default()
        .max_tokens(config.get_max_tokens())
        .model(config.get_model())
        .messages(chat_messages)
        .build()
        .unwrap();

    let response = client.chat()
        .create(request)
        .await.map_err(|e| OpenaiResponseError::Openai(e))?;

    Ok(
        ChatResponse {
            message: match response.choices.first() {
                Some(choice) => choice.clone().message.content.unwrap(),
                None => { return Err(OpenaiResponseError::Message("No answer".to_string())); }
            },
            tokens_spent: match response.usage {
                Some(u) => u.total_tokens,
                _ => 0
            },
        }
    )
}

pub async fn is_api_key_valid(api_key: &str) -> Result<bool, String> {
    let client = get_client(api_key);
    match client.models().list().await {
        Ok(_) => Ok(true),
        Err(OpenAIError::ApiError(err)) => match err.message.starts_with("Incorrect API key provided") {
            true => Ok(false),
            _ => Err(format!("{err:?}"))
        }
        Err(err) => Err(format!("{err:?}"))
    }
}