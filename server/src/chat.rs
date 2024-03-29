use crate::lua::SerializableValue;
use crate::object::types::Message;
use crate::world::{Id, WorldRef};
use actix::{Actor, AsyncContext, Handler, Message as ActixMessage, StreamHandler};
use actix_web::web;
use actix_web_actors::ws;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);
pub struct ChatSocket {
  app_data: web::Data<AppState>,
  self_id: Option<Id>,
}

impl Actor for ChatSocket {
  type Context = ws::WebsocketContext<Self>;

  fn started(&mut self, ctx: &mut Self::Context) {
    log::info!("ChatSocket stared");
    self.start_ping(ctx);
  }

  fn stopped(&mut self, ctx: &mut Self::Context) {
    if let Some(id) = self.self_id {
      // we use try_write here because the world could be gone if we're tearing down
      self.app_data.world_ref.try_write(|world| {
        world.remove_chat_connection(id, ctx.address());
        world.send_message(Message {
          target: self.id(),
          original_user: Some(self.id()),
          immediate_sender: self.id(),
          name: "disconnected".to_string(),
          payload: SerializableValue::Nil,
        })
      });
      log::info!("ChatSocket stopped for id {}", id);
    }
  }
}

impl ChatSocket {
  pub fn new(data: web::Data<AppState>) -> ChatSocket {
    ChatSocket {
      app_data: data,
      self_id: None,
    }
  }

  fn handle_message(
    &mut self,
    text: &str,
    ctx: &mut ws::WebsocketContext<Self>,
  ) -> Result<(), serde_json::error::Error> {
    let message: ToServerMessage = serde_json::from_str(&text)?;
    log::info!("Got message {:?}", message);
    match message {
      ToServerMessage::Login {
        username,
        user_type,
      } => self.handle_login(&username, &user_type, ctx),
      ToServerMessage::Command { text } => {
        let mut payload = HashMap::new();
        payload.insert(
          "message".to_string(),
          SerializableValue::String(text.to_string()),
        );
        self.handle_user_command("command", SerializableValue::Dict(payload))
      }
      ToServerMessage::SendMessage { name, payload } => {
        self.handle_user_command(&name, serde_json::from_value(payload)?)
      }
      ToServerMessage::ReloadCode {} => self.handle_reload(ctx),
      ToServerMessage::SaveFile { name, content } => {
        // TODO: this needs way nicer syntax
        let mut payload = HashMap::new();
        payload.insert("name".to_string(), SerializableValue::String(name));
        payload.insert("content".to_string(), SerializableValue::String(content));

        self.handle_user_command("save_file", SerializableValue::Dict(payload))
      }
    }

    Ok(())
  }

  fn handle_login(
    &mut self,
    username: &str,
    user_type: &str,
    ctx: &mut ws::WebsocketContext<Self>,
  ) {
    let world_ref = self.app_data.world_ref.clone();
    world_ref.write(|world| {
      let id = world
        .get_state_mut()
        .get_or_create_user(username, user_type);

      if let Some(existing_id) = self.self_id {
        world.remove_chat_connection(existing_id, ctx.address());
      }

      world.register_chat_connect(id, ctx.address());
      self.self_id = Some(id);
    });
    self.handle_user_command("connected", SerializableValue::Nil);
  }

  fn handle_user_command(&self, name: &str, payload: SerializableValue) {
    if self.self_id.is_none() {
      log::warn!("Got command when had no id")
    } else {
      self.app_data.world_ref.write(|world| {
        world.send_message(Message {
          target: self.id(),
          original_user: Some(self.id()),
          immediate_sender: self.id(),
          name: name.to_string(),
          payload: payload,
        })
      })
    }
  }

  fn handle_reload(&self, ctx: &mut ws::WebsocketContext<Self>) {
    let message = match self
      .app_data
      .world_ref
      .write(|world| world.pull_and_reload_code())
    {
      Err(e) => format!("Failed to reload: {}", e),
      Ok(message) => format!("Reloaded code: {}", message),
    };
    self
      .send_to_client(
        &ToClientMessage::Tell {
          content: ChatRowContent::new(&message),
        },
        ctx,
      )
      .unwrap();
  }

  fn id(&self) -> Id {
    self.self_id.unwrap()
  }

  fn send_to_client(
    &self,
    message: &ToClientMessage,
    ctx: &mut ws::WebsocketContext<Self>,
  ) -> Result<(), serde_json::error::Error> {
    let s = serde_json::to_string(message)?;
    ctx.text(s);
    Ok(())
  }

  fn start_ping(&mut self, ctx: &mut ws::WebsocketContext<ChatSocket>) {
    ctx.run_interval(HEARTBEAT_INTERVAL, |_actor, ctx| {
      ctx.ping(&[]);
    });
  }
}

impl Handler<ToClientMessage> for ChatSocket {
  type Result = ();

  fn handle(&mut self, msg: ToClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
    self
      .send_to_client(&msg, ctx)
      .unwrap_or_else(|e| log::error!("Error writing to client: {}", e))
  }
}

pub struct AppState {
  pub world_ref: WorldRef,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum ChatRowContent {
  TextContent { id: String, text: String },
  HtmlContent { id: String, html: String },
}

impl ChatRowContent {
  pub fn new(text: &str) -> ChatRowContent {
    return ChatRowContent::TextContent {
      id: Uuid::new_v4().to_string(),
      text: text.to_string(),
    };
  }

  pub fn new_html(html: &str) -> ChatRowContent {
    return ChatRowContent::HtmlContent {
      id: Uuid::new_v4().to_string(),
      html: html.to_string(),
    };
  }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum ToClientMessage {
  Tell { content: ChatRowContent },
  Backlog { history: Vec<ChatRowContent> },
  Log { level: String, message: String },
  EditFile { name: String, content: String },
}

impl ActixMessage for ToClientMessage {
  type Result = ();
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
enum ToServerMessage {
  Login {
    username: String,
    user_type: String, // eg "user" or "group" or whatever -- will become $username/live.$user_type
  },
  Command {
    text: String,
  },
  SendMessage {
    name: String,
    payload: serde_json::Value,
  },
  ReloadCode {},
  SaveFile {
    name: String,
    content: String,
  },
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for ChatSocket {
  fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
    match msg {
      Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
      Ok(ws::Message::Text(text)) => {
        if let Err(e) = self.handle_message(&text, ctx) {
          log::error!("Failed handling message from user: {}", e);
        }
      }
      _ => (),
    }
  }
}
