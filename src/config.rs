use std::path::PathBuf;

use serde::Deserialize;

use crate::tests::Tests;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub assembler: Option<PathBuf>,
    pub emulator: Option<PathBuf>,
    pub ens_file: Option<PathBuf>,
    pub serie_file: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ConfigAll {
    pub config: Config,

    pub tests: Tests,
}
