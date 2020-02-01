use std::fmt;

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Id(usize);

impl fmt::Display for Id {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "#{}", self.0)
  }
}

#[derive(Debug)]
struct Object {
  parent_id: Option<Id>,
}

pub struct World {
  objects: Vec<Object>,
}

impl World {
  pub fn create(&mut self) -> Id {
    self.objects.push(Object { parent_id: None });
    Id(self.objects.len())
  }

  pub fn new() -> World {
    World { objects: vec![] }
  }
}
