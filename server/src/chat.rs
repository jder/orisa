use actix::{Actor, Arbiter, StreamHandler};
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_web_actors::ws;
use serde::{Deserialize, Serialize};
use serde_json;
use std::error::Error;

use std::sync::{Arc, RwLock};
use uuid::Uuid;

use crate::object::{Id, Message, World, WorldRef};

pub struct ChatSocket {
  app_data: web::Data<AppState>,
  self_id: Option<Id>,
}

impl Actor for ChatSocket {
  type Context = ws::WebsocketContext<Self>;

  fn started(&mut self, ctx: &mut Self::Context) {
    let world_ref = self.app_data.world_ref.clone();
    world_ref.write(|world| {
      let entrance = world.entrance();
      let id = world.create_in(Some(entrance));
      self.self_id = Some(id);
      self.send_to_client(&ServerMessage::new(&format!("You are object {}", id)), ctx);
    });
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
    &self,
    text: &str,
    ctx: &mut ws::WebsocketContext<Self>,
  ) -> Result<(), serde_json::error::Error> {
    let message: ClientMessage = serde_json::from_str(&text)?;
    self.app_data.world_ref.read(|world| {
      if let Some(container) = world.parent(self.self_id.unwrap()) {
        world.message(container, Message::Say { text: message.text });
      } else {
        self.send_to_client(&ServerMessage::new(&format!("You aren't anywhere.",)), ctx);
      }
    });
    Ok(())
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

pub struct AppState {
  pub world: Arc<RwLock<Option<World>>>,
  pub world_ref: WorldRef,
}

#[derive(Serialize, Deserialize)]
struct ClientMessage {
  id: String,
  text: String,
}

#[derive(Serialize, Deserialize)]
struct ServerMessage {
  id: String,
  text: String,
}

impl ServerMessage {
  fn new(text: &str) -> ServerMessage {
    ServerMessage {
      id: Uuid::new_v4().to_string(),
      text: text.to_string(),
    }
  }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for ChatSocket {
  fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
    match msg {
      Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
      Ok(ws::Message::Text(text)) => {
        self.handle_message(&text, ctx);
      }
      Ok(ws::Message::Binary(bin)) => ctx.binary(bin),
      _ => (),
    }
  }
}
