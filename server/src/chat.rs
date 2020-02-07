use crate::lua;
use crate::object_actor::ObjectMessage;
use crate::world::{Id, WorldRef};
use actix::{Actor, AsyncContext, Handler, Message, StreamHandler};
use actix_web::web;
use actix_web_actors::ws;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json;
use uuid::Uuid;

pub struct ChatSocket {
  app_data: web::Data<AppState>,
  self_id: Option<Id>,
}

impl Actor for ChatSocket {
  type Context = ws::WebsocketContext<Self>;

  fn started(&mut self, _ctx: &mut Self::Context) {
    log::info!("ChatSocket stared");
  }

  fn stopped(&mut self, ctx: &mut Self::Context) {
    if let Some(id) = self.self_id {
      self
        .app_data
        .world_ref
        .write(|world| world.remove_chat_connection(id, ctx.address()));
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

    match message {
      ToServerMessage::Login { username } => self.handle_login(&username, ctx),
      ToServerMessage::Say { text } => self.handle_say(&text, ctx),
    }

    Ok(())
  }

  fn handle_login(&mut self, username: &str, ctx: &mut ws::WebsocketContext<Self>) {
    let world_ref = self.app_data.world_ref.clone();
    world_ref.write(|world| {
      let id = world.get_or_create_user(username);

      if let Some(existing_id) = self.self_id {
        world.remove_chat_connection(existing_id, ctx.address());
      }

      world.register_chat_connect(id, ctx.address());
      self.self_id = Some(id);
      self
        .send_to_client(
          &ToClientMessage::Tell {
            text: format!("You are object {}", id),
          },
          ctx,
        )
        .unwrap();
    });
  }

  fn handle_say(&self, text: &str, ctx: &mut ws::WebsocketContext<Self>) {
    if self.self_id.is_none() {
      log::warn!("Got tell when had no id")
    } else {
      self.app_data.world_ref.read(|world| {
        if let Some(container) = world.parent(self.id()) {
          world.send_message(
            container,
            ObjectMessage {
              immediate_sender: self.id(),
              name: "command".to_string(),
              payload: serde_json::from_str(text).unwrap(),
            },
          );
        } else {
          self
            .send_to_client(
              &ToClientMessage::Tell {
                text: "You aren't anywhere.".to_string(),
              },
              ctx,
            )
            .unwrap();
        }
      });
    }
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
}

impl Handler<ToClientMessage> for ChatSocket {
  type Result = ();

  fn handle(&mut self, msg: ToClientMessage, ctx: &mut ws::WebsocketContext<Self>) {
    self.send_to_client(&msg, ctx).unwrap();
  }
}

pub struct AppState {
  pub world_ref: WorldRef,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum ToClientMessage {
  Tell { text: String },
  Backlog { history: Vec<String> },
}

impl Message for ToClientMessage {
  type Result = ();
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
enum ToServerMessage {
  Login { username: String },
  Say { text: String },
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for ChatSocket {
  fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
    match msg {
      Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
      Ok(ws::Message::Text(text)) => {
        self.handle_message(&text, ctx).unwrap();
      }
      Ok(ws::Message::Binary(bin)) => ctx.binary(bin),
      _ => (),
    }
  }
}
