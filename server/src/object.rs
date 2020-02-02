use actix::{Actor, Addr, Arbiter, Context, Handler};
use std::fmt;

use std::sync::{Arc, RwLock, Weak};
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
  world: WorldRef,
}

impl Actor for ObjectActor {
  type Context = Context<Self>;
}

impl Handler<Message> for ObjectActor {
  type Result = ();

  fn handle(&mut self, msg: Message, ctx: &mut Self::Context) {
    match msg {
      Message::Say { text } => self.world.read(|w| {
        w.children(self.id)
          .for_each(|child| w.message(child, Message::Broadcast { text: text.clone() }))
      }),
      Message::Broadcast { text } => log::info!("Would send text '{}' to client", text),
    }
  }
}

pub struct World {
  objects: Vec<Object>,
  entrance_id: Option<Id>, // only None during initialization

  arbiter: Arbiter,
  own_ref: WorldRef,
}

/// Weak reference to the world for use by ObjectActors
#[derive(Clone)]
pub struct WorldRef {
  world: Weak<RwLock<Option<World>>>, // Only None during initialization
}

impl WorldRef {
  pub fn read<F, T>(&self, f: F) -> T
  where
    F: FnOnce(&World) -> T,
  {
    // This is horribly gross which is why we do it here, once.
    let arc = self.world.upgrade().unwrap();
    let guard = arc.read().unwrap();
    let w = guard.as_ref();
    f(&w.unwrap())
  }

  pub fn write<F, T>(&self, f: F) -> T
  where
    F: FnOnce(&mut World) -> T,
  {
    // This is horribly gross which is why we do it here, once.
    let arc = self.world.upgrade().unwrap();
    let mut guard = arc.write().unwrap();
    let w = guard.as_mut();
    f(&mut w.unwrap())
  }
}

impl World {
  pub fn create_in(&mut self, parent: Option<Id>) -> Id {
    let id = Id(self.objects.len());

    let world_ref = self.own_ref.clone();
    let addr = ObjectActor::start_in_arbiter(&self.arbiter, move |_ctx| ObjectActor {
      id: id,
      world: world_ref,
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
    self.entrance_id.unwrap()
  }

  pub fn children(&self, id: Id) -> impl Iterator<Item = Id> + '_ {
    self
      .objects
      .iter()
      .filter(move |o| o.parent_id == Some(id))
      .map(|o| o.id)
  }

  pub fn parent(&self, of: Id) -> Option<Id> {
    self.objects.get(of.0).unwrap().parent_id
  }

  pub fn message(&self, id: Id, message: Message) {
    self.objects.get(id.0).unwrap().address.do_send(message)
  }

  fn create_defaults(&mut self) {
    let entrance = self.create_in(None);
    self.entrance_id = Some(entrance)
  }

  pub fn new(arbiter: Arbiter) -> (Arc<RwLock<Option<World>>>, WorldRef) {
    let arc = Arc::new(RwLock::new(None));

    let world_ref = WorldRef {
      world: Arc::downgrade(&arc),
    };

    let mut world = World {
      objects: vec![],
      entrance_id: None,
      arbiter: arbiter,
      own_ref: world_ref.clone(),
    };
    world.create_defaults();

    {
      let mut maybe_world = arc.write().unwrap();
      *maybe_world = Some(world);
    }
    (arc, world_ref)
  }
}

#[derive(Debug)]
pub enum Message {
  Say { text: String },
  Broadcast { text: String },
}

impl actix::prelude::Message for Message {
  type Result = ();
}
