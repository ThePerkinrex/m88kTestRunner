use std::path::Path;

use serde::Deserialize;

#[derive(Debug)]
pub enum LoadError {
    IOError(std::io::Error),
    YAMLError(serde_yaml::Error),
}

impl From<std::io::Error> for LoadError {
    fn from(e: std::io::Error) -> Self {
        Self::IOError(e)
    }
}

impl From<serde_yaml::Error> for LoadError {
    fn from(e: serde_yaml::Error) -> Self {
        Self::YAMLError(e)
    }
}

pub trait Loadable: Sized {
    fn load<P: AsRef<Path>>(path: P) -> Result<Self, LoadError>;
}

impl<T> Loadable for T
where
    T: Sized + for<'a> Deserialize<'a>,
{
    fn load<P: AsRef<Path>>(path: P) -> Result<Self, LoadError> {
        Ok(serde_yaml::from_str(&std::fs::read_to_string(path)?)?)
    }
}
