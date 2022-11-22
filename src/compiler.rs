use std::{
    path::Path,
    process::{Command, Output},
};

pub struct Compiler<'a> {
    assembler: &'a Path,
    ens_file: &'a Path,
}

impl<'a> Compiler<'a> {
    pub fn new<P1: AsRef<Path>, P2: AsRef<Path>>(assembler: &'a P1, ens_file: &'a P2) -> Self {
        Self {
            assembler: assembler.as_ref(),
            ens_file: ens_file.as_ref(),
        }
    }

    pub fn run(&self, test_name: &str) -> std::io::Result<Output> {
        Command::new(self.assembler)
            .arg("-e")
            .arg(test_name)
            .args(["-o", "CDV.bin"])
            .arg(self.ens_file)
            .output()
    }
}
