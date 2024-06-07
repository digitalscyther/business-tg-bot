mod dialogue;
mod user;
mod db;

use std::env;
use std::fs::read_to_string;
use sqlx::{Pool, Postgres};

use tgbot::{
    api::Client,
    handler::{LongPoll, UpdateHandler},
    types::{SendMessage, Update},
};
use tgbot::types::{Chat, UpdateType};
use crate::user::{Openai, User};

const MAX_PROMPT_SIZE: usize = 4_000;

struct Handler {
    client: Client,
}

impl UpdateHandler for Handler {
    async fn handle(&self, update: Update) {
        let pool = db::create_pool().await;
        let method = match update.update_type {
            UpdateType::BusinessConnection(connection) => {
                Some(SendMessage::new(connection.user_chat_id, match connection.is_enabled {
                    true => {
                        db::insert_or_update_user(&pool, connection.user_chat_id, &connection.id).await.unwrap();
                        "created\nnow /help for info"
                    }
                    false => {
                        db::delete_user_by_id(&pool, connection.user_chat_id).await.unwrap();
                        "deleted"
                    }
                }))
            }
            UpdateType::BusinessMessage(message) => {
                let business_id = message.business_connection_id.clone().unwrap();
                match db::load_user_from_business_id(&pool, &business_id).await {
                    Ok(user) => {
                        if let Some(uid) = message.sender.get_user_id() {
                            let uid: i64 = uid.into();
                            if uid == user.get_id() {
                                return;
                            }
                        }
                        Some(
                            SendMessage::new(
                                message.chat.get_id(),
                                match message.get_text() {
                                    Some(text) => match get_answer(&pool, &user, text.clone().data).await {
                                        Ok(Some(message)) => message,
                                        Ok(None) => {return;},
                                        Err(e) => e
                                    },
                                    None => "Only text".to_string()
                                },
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
                match db::load_user_from_chat_id(&pool, chat_id.into()).await {
                    Ok(mut user) => Some(SendMessage::new(chat_id, setup(
                        &pool,
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
            format!("Current API key: {:?}", config.get_api_key())
        }
        ["/model", new_model] => {
            config.set_model(new_model.to_string())?;
            "Option updated".to_string()
        }
        ["/model"] => {
            format!("Current model: {:?}", config.get_model())
        }
        ["/prompt"] => {
            format!("Current prompt: {:?}", config.get_prompt())
        }
        ["/prompt", ..] => {
            let new_prompt = command.replacen("/prompt ", "", 1);
            match new_prompt.len() > MAX_PROMPT_SIZE {
                true => "Max prompt size is 4.000 symbols".to_string(),
                false => {
                    config.set_prompt(new_prompt)?;   // TODO set prompt to None
                    "Option updated".to_string()
                },
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

async fn get_answer(pool: &Pool<Postgres>, user: &User, message: String) -> Result<Option<String>, String> {
    if user.get_openai_spent_tokens() > user.get_config().get_max_total_tokens_spent() {
        return Ok(None);    // TODO send notification to owner
    }

    if message.len() > user.get_config().get_max_message_length() as usize {
        return Err("too long message".to_string());
    }

    let config = user.get_config();
    match dialogue::get_response(&config, message).await {
        Ok(response) => {
            if let Err(e) = db::add_spends(pool, user.get_id(), response.tokens_spent as i32).await {
                log::error!("Failed update tokens spent:{e:?}");
            }
            return Ok(Some(response.message))
        }
        Err(err) => {
            log::error!("Failed get at response:\n{err:?}");
            Err("I don't know what to answer".to_string())
        }
    }
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    db::migrate(&db::create_pool().await).await.expect("failed migrations");

    let token = env::var("TG_TOKEN").expect("TG_TOKEN is not set");
    let client = Client::new(token).expect("Failed to create API");
    LongPoll::new(client.clone(), Handler { client }).run().await;
}