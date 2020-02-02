use crate::chat::ServerMessage;
use crate::world::*;
use actix::{Actor, Context, Handler, Message};
use serde::{Deserialize, Serialize};

pub struct ObjectActor {
  id: Id,
  world: WorldRef,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ObjectActorState {}

impl ObjectActor {
  pub fn new(id: Id, world_ref: WorldRef, state: Option<ObjectActorState>) -> ObjectActor {
    ObjectActor {
      id: id,
      world: world_ref,
    }
  }
}

impl Actor for ObjectActor {
  type Context = Context<Self>;
}

impl Handler<ObjectMessage> for ObjectActor {
  type Result = ();

  fn handle(&mut self, msg: ObjectMessage, _ctx: &mut Self::Context) {
    let sender = msg.immediate_sender;
    match msg.payload {
      ObjectMessagePayload::Say { text } => self.world.read(|w| {
        let name = w.username(sender);
        w.children(self.id).for_each(|child| {
          w.send_message(
            child,
            ObjectMessage {
              immediate_sender: self.id,
              payload: ObjectMessagePayload::Broadcast {
                text: format!("{}: {}", name, text.clone()),
              },
            },
          )
        })
      }),
      //TODO: only handle broadcasts for object types expected to have chat connections (i.e. people)
      ObjectMessagePayload::Broadcast { text } => self
        .world
        .read(|w| w.send_client_message(self.id, ServerMessage::new(&text))),
    }
  }
}

impl Handler<FreezeMessage> for ObjectActor {
  type Result = Option<FreezeResponse>;

  fn handle(&mut self, _msg: FreezeMessage, _ctx: &mut Self::Context) -> Option<FreezeResponse> {
    Some(FreezeResponse {
      id: self.id,
      state: ObjectActorState {},
    })
  }
}

#[derive(Debug)]
pub struct ObjectMessage {
  pub immediate_sender: Id,
  pub payload: ObjectMessagePayload,
}

#[derive(Debug)]
pub enum ObjectMessagePayload {
  Say { text: String },
  Broadcast { text: String },
}

impl Message for ObjectMessage {
  type Result = ();
}
