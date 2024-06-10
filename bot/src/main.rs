mod dialogue;
mod user;
mod db;
mod conversation;

use rand::Rng;
use std::env;
use std::fs::read_to_string;
use sqlx::{Pool, Postgres};

use tgbot::{
    api::Client,
    handler::{LongPoll, UpdateHandler},
    types::{SendMessage, Update},
};
use tgbot::types::{Chat, ChatAction, SendChatAction, UpdateType};
use tokio::time::{sleep, Duration};
use crate::user::{Openai, User};

const MAX_PROMPT_SIZE: usize = 4_000;

struct Handler {
    client: Client,
    pool: Pool<Postgres>,
}

impl UpdateHandler for Handler {
    async fn handle(&self, update: Update) {
        let method = match update.update_type {
            UpdateType::BusinessConnection(connection) => {
                Some(SendMessage::new(connection.user_chat_id, match connection.is_enabled {
                    true => {
                        db::insert_or_update_user(&self.pool, connection.user_chat_id, &connection.id).await.unwrap();
                        "created\nnow /help for info"
                    }
                    false => {
                        db::delete_user_by_id(&self.pool, connection.user_chat_id).await.unwrap();
                        "deleted"
                    }
                }))
            }
            UpdateType::BusinessMessage(message) => {
                let business_id = message.business_connection_id.clone().unwrap();
                match db::load_user_from_business_id(&self.pool, &business_id).await {
                    Ok(user) => {
                        let sender_id = message.sender.get_user_id();
                        if let Some(sender_id) = sender_id {
                            let sender_id: i64 = sender_id.into();
                            if sender_id == user.get_id() {
                                return;
                            }
                        } else { return; }
                        Some(
                            SendMessage::new(
                                message.chat.get_id(),
                                {
                                    let response = match message.get_text() {
                                        Some(text) => match get_answer(
                                            &self.pool, &user, &sender_id.unwrap().to_string(), &text.clone().data,
                                            || async {
                                                let _ = self.client.execute(
                                                    SendChatAction::new(
                                                        message.chat.get_id(),
                                                        ChatAction::Typing,
                                                    ).with_business_connection_id(
                                                        &business_id
                                                    )
                                                ).await;
                                            }
                                        ).await {
                                            Ok(Some(message)) => message,
                                            Ok(None) => { return; }
                                            Err(e) => e
                                        },
                                        None => "Only text".to_string()
                                    };
                                    match user.get_config().get_footer() {
                                        Some(footer) => format!("{}\n\n{}", response, footer),
                                        None => response,
                                    }
                                }
                            ).with_business_connection_id(business_id)
                        )
                    }
                    Err(_) => {
                        log::error!("Not found user with business_id={}", business_id);
                        None
                    }
                }
            }
            UpdateType::Message(message) => {
                let chat_id = match &message.chat {
                    Chat::Private(chat) => chat.id,
                    _ => return,
                };
                match db::load_user_from_chat_id(&self.pool, chat_id.into()).await {
                    Ok(mut user) => Some(SendMessage::new(chat_id, setup(
                        &self.pool,
                        &mut user,
                        match message.get_text() {
                            Some(text) => text.clone().data,
                            None => "Only text".to_string()
                        }).await.unwrap_or_else(|e| e.to_string()),
                    )),
                    Err(_) => {
                        let contact = env::var("CONTACT").unwrap_or("@DigitalScyther".to_string());
                        Some(SendMessage::new(chat_id, format!("only for business\ncontact {contact}")))
                    }
                }
            }
            _ => {
                log::info!("Skipped unexpected type message");
                None
            }
        };

        if let Some(method) = method {
            self.client.execute(method).await.unwrap();
        }
    }
}

async fn setup(pool: &Pool<Postgres>, user: &mut User, command: String) -> Result<String, String> {
    let mut config = user.get_config();
    let parts: Vec<&str> = command.split_whitespace().collect();

    let response = match parts.as_slice() {
        ["/api_key", new_api_key] => {
            config.set_api_key(new_api_key.to_string()).await?;
            "Option updated".to_string()
        }
        ["/api_key"] => {
            format!("Current API key: {:?}", config.get_api_key().unwrap_or("---".to_string()))
        }
        ["/history_timeout", new_cache_duration] => {
            let cache_duration: i64 = new_cache_duration.parse().map_err(|_| "Invalid history_timeout")?;
            config.set_cache_duration(cache_duration)?;
            "Option updated".to_string()
        }
        ["/history_timeout"] => {
            format!("Current history timeout: {:?} seconds", config.get_cache_duration())
        }
        ["/history_length", new_char_limit] => {
            let char_limit: usize = new_char_limit.parse().map_err(|_| "Invalid history_length")?;
            config.set_char_limit(char_limit)?;
            "Option updated".to_string()
        }
        ["/history_length"] => {
            format!("Current history length: {:?} symbols", config.get_char_limit())
        }
        ["/answer_pause", new_answer_pause] => {
            config.set_answer_pause(new_answer_pause)?;
            "Option updated".to_string()
        }
        ["/answer_pause"] => {
            let answer_pause = config.get_answer_pause();
            format!("Current answer pause: {}", match answer_pause.0 == answer_pause.1 {
                true => format!("{} seconds", answer_pause.0),
                false => format!("from {} to {} seconds", answer_pause.0, answer_pause.1),
            })
        }
        ["/answer_footer"] => {
            format!("Current footer: {:?}", config.get_footer().unwrap_or("---".to_string()))
        }
        ["/answer_footer", ..] => {
            let new_answer_footer = command.replacen("/answer_footer ", "", 1);
            config.set_footer(match new_answer_footer.trim().to_lowercase() == "[empty]" {
                true => None,
                false => Some(new_answer_footer.to_string())
            })?;
            "Option updated".to_string()
        }
        ["/model", new_model] => {
            config.set_model(new_model.to_string())?;
            "Option updated".to_string()
        }
        ["/model"] => {
            format!("Current model: {:?}", config.get_model())
        }
        ["/prompt"] => {
            format!("Current prompt: {:?}", config.get_prompt().unwrap_or("---"))
        }
        ["/prompt", ..] => {
            let new_prompt = command.replacen("/prompt ", "", 1);
            match new_prompt.len() > MAX_PROMPT_SIZE {
                true => "Max prompt size is 4.000 symbols".to_string(),
                false => {
                    config.set_prompt(new_prompt)?;   // TODO set prompt to None
                    "Option updated".to_string()
                }
            }
        }
        ["/max_message_length", new_length] => {
            let length: i32 = new_length.parse().map_err(|_| "Invalid length")?;
            config.set_max_message_length(length)?;
            "Option updated".to_string()
        }
        ["/max_message_length"] => {
            format!("Current max message length: {}", config.get_max_message_length())
        }
        ["/max_total_tokens_spent", new_tokens] => {
            let tokens: i64 = new_tokens.parse().map_err(|_| "Invalid token amount")?;
            config.set_max_total_tokens_spent(tokens);
            "Option updated".to_string()
        }
        ["/max_total_tokens_spent"] => {
            format!("Current max total tokens spent: {}", config.get_max_total_tokens_spent())
        }
        ["/max_tokens", new_tokens] => {
            let tokens: u16 = new_tokens.parse().map_err(|_| "Invalid token amount")?;
            config.set_max_tokens(tokens);
            "Option updated".to_string()
        }
        ["/max_tokens"] => {
            format!("Current max tokens: {}", config.get_max_tokens())
        }
        ["/help"] => read_to_string("./files/help_text.txt").unwrap_or_else(|e| {
            log::error!("Failed get help command text:\n{e:?}");
            "failed get help".to_string()
        }),
        _ => "Unknown command".to_string()
    };

    let openai: Openai = Openai::default()
        .with_config(config.clone())
        .with_spent_tokens(user.get_openai_spent_tokens());

    if let Err(e) = db::update_openai_by_id(
        pool,
        user.get_id(),
        openai,
    ).await {
        let err_str: String = format!("Failed update user openai:\n{e:?}");
        return Err(err_str);
    }

    Ok(response.to_string())
}

async fn get_answer<F, Fut>(
    pool: &Pool<Postgres>,
    user: &User,
    sender_id: &str,
    message: &str,
    call_typing: F
) -> Result<Option<String>, String>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    if user.get_openai_spent_tokens() > user.get_config().get_max_total_tokens_spent() {
        return Ok(None);    // TODO send notification to owner
    }

    if message.len() > user.get_config().get_max_message_length() as usize {
        return Err("too long message".to_string());
    }

    let config = user.get_config();

    Ok(config.get_manager().await.process_message(
        sender_id,
        message,
        |messages| async {
            match dialogue::get_response(&config, messages).await {
                Ok(response) => {
                    if let Err(e) = db::add_spends(pool, user.get_id(), response.tokens_spent as i32).await {
                        log::error!("Failed update tokens spent:{e:?}");
                    }
                    let (from, to) = config.get_answer_pause();
                    let random_seconds = rand::thread_rng().gen_range(from..=to) as u64;
                    let duration = Duration::from_secs(random_seconds);
                    call_typing().await;
                    sleep(duration).await;
                    return Ok(Some(response.message));
                }
                Err(err) => {
                    log::error!("Failed get at response:\n{err:?}");
                    Err("I don't know what to answer".to_string())
                }
            }
        },
    ).await.unwrap_or_else(|e| Some(e)))
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let pool = db::create_pool().await;
    db::migrate(&pool).await.expect("failed migrations");

    let token = env::var("TG_TOKEN").expect("TG_TOKEN is not set");
    let client = Client::new(token).expect("Failed to create API");

    log::info!("Bot starting...");
    LongPoll::new(client.clone(), Handler { client, pool }).run().await;
}