use crate::chat::ServerMessage;
use crate::world::*;
use actix::{Actor, Context, Handler, Message};

pub struct ObjectActor {
  id: Id,
  world: WorldRef,
}

impl ObjectActor {
  pub fn new(id: Id, world_ref: WorldRef) -> ObjectActor {
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
