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
    let message: ClientMessage = serde_json::from_str(&text)?;

    lazy_static! {
      static ref LOGIN_REGEX: Regex = Regex::new(" */login +([[:alpha:]]+)").unwrap();
    }
    if let Some(caps) = LOGIN_REGEX.captures(&message.text) {
      let world_ref = self.app_data.world_ref.clone();
      world_ref.write(|world| {
        let id = world.get_or_create_user(caps.get(1).unwrap().as_str());

        if let Some(existing_id) = self.self_id {
          world.remove_chat_connection(existing_id, ctx.address());
        }

        world.register_chat_connect(id, ctx.address());
        self.self_id = Some(id);
        self
          .send_to_client(&ServerMessage::new(&format!("You are object {}", id)), ctx)
          .unwrap();
      });
    } else if self.self_id.is_none() {
      self
        .send_to_client(
          &ServerMessage::new(&format!("Please `/login username` first")),
          ctx,
        )
        .unwrap();
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
            .send_to_client(&ServerMessage::new("You aren't anywhere."), ctx)
            .unwrap();
        }
      });
    }

    Ok(())
  }

  fn id(&self) -> Id {
    self.self_id.unwrap()
  }

  fn send_to_client(
    &self,
    message: &ServerMessage,
    ctx: &mut ws::WebsocketContext<Self>,
  ) -> Result<(), serde_json::error::Error> {
    let s = serde_json::to_string(message)?;
    ctx.text(s);
    Ok(())
  }
}

impl Handler<ServerMessage> for ChatSocket {
  type Result = ();

  fn handle(&mut self, msg: ServerMessage, ctx: &mut ws::WebsocketContext<Self>) {
    self.send_to_client(&msg, ctx).unwrap();
  }
}

pub struct AppState {
  pub world_ref: WorldRef,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ClientMessage {
  id: String,
  text: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ServerMessage {
  id: String,
  text: String,
}

impl ServerMessage {
  pub fn new(text: &str) -> ServerMessage {
    ServerMessage {
      id: Uuid::new_v4().to_string(),
      text: text.to_string(),
    }
  }
}

// TODO: Consider separating "messages we send to client" from "messages other objects send to chat socket"
impl Message for ServerMessage {
  type Result = ();
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
