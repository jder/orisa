use std::error;

pub type ResultAnyError<T> = Result<T, Box<dyn error::Error>>;
