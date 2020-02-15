use std::error;
use std::sync::{Arc, RwLock, Weak};

pub type ResultAnyError<T> = Result<T, Box<dyn error::Error>>;

/// Weak reference to a read/write-locked value
pub struct WeakRw<T> {
  value: Weak<RwLock<Option<T>>>, // Only None during initialization
}

impl<T> Clone for WeakRw<T> {
  fn clone(&self) -> WeakRw<T> {
    WeakRw {
      value: self.value.clone(),
    }
  }
}

impl<T> WeakRw<T> {
  pub fn new(arc: &Arc<RwLock<Option<T>>>) -> WeakRw<T> {
    WeakRw {
      value: Arc::downgrade(arc),
    }
  }

  pub fn try_read<F, R>(&self, f: F) -> Option<R>
  where
    F: FnOnce(&T) -> R,
  {
    // This is horribly gross which is why we do it here, once.
    let arc = self.value.upgrade()?;
    let guard = arc.read().unwrap();
    let v = guard.as_ref()?;
    Some(f(&v))
  }

  pub fn try_write<F, R>(&self, f: F) -> Option<R>
  where
    F: FnOnce(&mut T) -> R,
  {
    // This is horribly gross which is why we do it here, once.
    let arc = self.value.upgrade()?;
    let mut guard = arc.write().unwrap();
    let mut v = guard.as_mut()?;
    Some(f(&mut v))
  }

  pub fn read<F, R>(&self, f: F) -> R
  where
    F: FnOnce(&T) -> R,
  {
    self.try_read(f).unwrap()
  }

  pub fn write<F, R>(&self, f: F) -> R
  where
    F: FnOnce(&mut T) -> R,
  {
    self.try_write(f).unwrap()
  }
}
