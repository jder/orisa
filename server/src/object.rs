use actix::{Actor, Addr, Arbiter, Context, Handler};
use std::fmt;

use std::sync::{Arc, Mutex};
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Id(usize);

impl fmt::Display for Id {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "#{}", self.0)
  }
}

struct Object {
  id: Id,
  parent_id: Option<Id>,

  address: Addr<ObjectActor>,
}

struct ObjectActor {
  id: Id,
  world: Arc<Mutex<World>>,
}

impl Actor for ObjectActor {
  type Context = Context<Self>;
}

impl Handler<Message> for ObjectActor {
  type Result = ();

  fn handle(&mut self, msg: Message, ctx: &mut Self::Context) {
    match msg {
      Message::Say { text } => {
        let world = self.world.lock().unwrap();
        world
          .children(self.id)
          .for_each(|child| world.message(child, Message::Broadcast { text: text }))
      }
    }
  }
}

pub struct World {
  objects: Vec<Object>,
  entrance_id: Id,

  arbiter: Arbiter,
}

/// Weak reference to the world for use by ObjectActors
struct WorldRef {
  world: Weak<
}

impl World {
  pub fn create_in(&mut self, parent: Option<Id>) -> Id {
    let id = Id(self.objects.len());

    let addr = ObjectActor::start_in_arbiter(&self.arbiter, |ctx| ObjectActor {
      id: id,
      world: self,
    });

    let o = Object {
      id: id,
      parent_id: parent,
      address: addr,
    };
    self.objects.push(o);
    id
  }

  pub fn entrance(&self) -> Id {
    self.entrance_id
  }

  pub fn parent(&self, of: Id) -> Option<Id> {
    self.objects.get(of.0).unwrap().parent_id
  }

  pub fn message(&self, id: Id, message: Message) -> Result<(), SendError<Message>> {
    self.objects.get(id.0).unwrap().address.do_send(message)
  }

  pub fn new(arbiter: Arbiter) -> World {
    let entrance = Object {
      id: Id(0),
      parent_id: None,
    };

    World {
      objects: vec![entrance],
      entrance_id: Id(0),
      arbiter: arbiter,
    }
  }
}

#[derive(Debug)]
enum Message {
  Say { text: String },
  Broadcast { text: String },
}

impl actix::prelude::Message for Message {
  type Result = ();
}
