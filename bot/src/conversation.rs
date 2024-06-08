use std::env;
use redis::{AsyncCommands, RedisResult};
use serde::{Deserialize, Serialize};
use chrono::Utc;

pub const DEFAULT_CACHE_DURATION: i64 = 60 * 10;
pub const DEFAULT_CHAR_LIMIT: usize = 10_000;

#[derive(Serialize, Deserialize, Debug)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    fn new(role: &str, content: &str) -> Self {
        Self { role: role.to_string(), content: content.to_string() }
    }
}

pub struct ConversationManager {
    client: redis::Client,
    cache_duration: i64,
    char_limit: usize,
    prefix: String,
}

impl ConversationManager {
    pub async fn default() -> Self {
        Self::new(DEFAULT_CACHE_DURATION, DEFAULT_CHAR_LIMIT).await
    }

    async fn new(cache_duration: i64, char_limit: usize) -> Self {
        let redis_url = env::var("REDIS_URL").expect("REDIS_URL must be set");
        let client = redis::Client::open(redis_url).unwrap();
        ConversationManager {
            client,
            cache_duration,
            char_limit,
            prefix: "history".to_string(),
        }
    }

    pub fn with_cache_duration(mut self, value: i64) -> Self {
        self.cache_duration = value;
        self
    }

    pub fn with_char_limit(mut self, value: usize) -> Self {
        self.char_limit = value;
        self
    }

    pub async fn store_message(&self, user_id: &str, role: &str, content: &str, timestamp: Option<i64>) -> RedisResult<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("{}:{}", self.prefix, user_id);
        let timestamp = timestamp.unwrap_or(Utc::now().timestamp_millis());

        let message_json = serde_json::to_string(&Message {
            role: role.to_string(),
            content: content.to_string(),
        }).unwrap();

        // Add message to sorted set with timestamp as the score
        conn.zadd(&key, message_json, timestamp).await?;
        conn.expire(&key, self.cache_duration).await?;

        Ok(())
    }

    pub async fn get_conversation(&self, user_id: &str, current_message_length: usize) -> RedisResult<Vec<Message>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("{}:{}", self.prefix, user_id);
        let mut total_length = current_message_length;
        let mut trimmed_conversation = Vec::new();

        let mut cursor = 0;
        loop {
            // Fetch messages in reverse order by their timestamp one by one
            let batch: Vec<String> = conn.zrevrange(&key, cursor, cursor).await?;
            if batch.is_empty() {
                break;
            }

            let message_json = &batch[0];
            let message: Message = serde_json::from_str(message_json).unwrap();
            let message_length = message_json.len();

            if total_length + message_length <= self.char_limit {
                total_length += message_length;
                trimmed_conversation.push(message);
            } else {
                // Remove the old message that exceeds the limit
                conn.zrem(&key, message_json).await?;
            }

            cursor += 1;
        }

        // Reverse the order to restore the original chronological order
        trimmed_conversation.reverse();

        Ok(trimmed_conversation)
    }

    pub async fn process_message<F, Fut>(&self, sender_id: &str, message: &str, func: F) -> Result<Option<String>, String>
        where
            F: Fn(Vec<Message>) -> Fut,
            Fut: std::future::Future<Output=Result<Option<String>, String>>,
    {
        let mut history = self.get_conversation(&sender_id, message.len()).await.unwrap_or(vec![]);
        history.push(Message::new("user", message));
        let timestamp = Some(Utc::now().timestamp_millis());
        let answer = match func(history).await {
            Ok(Some(answer)) => answer,
            resp => return resp,
        };

        for (role, message, ts) in vec![
            ("user", message, timestamp),
            ("assistant", &answer, None)
        ].into_iter() {
            if let Err(e) = self.store_message(sender_id, role, message, ts).await {
                log::error!("Failed store message:\n{e:?}");
            }
        }

        Ok(Some(answer))
    }
}