use std::{
    env::current_dir,
    path::{Path, PathBuf},
    process::{Command, Output},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CompilerBuilder {
    assembler: PathBuf,
    ens_file: Option<PathBuf>,
    outfile: Option<PathBuf>,
    current_working_dir: Option<PathBuf>,
}

impl CompilerBuilder {
    pub const fn new(assembler: PathBuf) -> Self {
        Self {
            assembler,
            ens_file: None,
            outfile: None,
            current_working_dir: None,
        }
    }

    pub fn outfile_mut(&mut self, outfile: PathBuf) -> &mut Self {
        self.outfile = Some(outfile);
        self
    }

    pub fn outfile(&self, outfile: PathBuf) -> Self {
        let mut s = self.clone();
        s.outfile_mut(outfile);
        s
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn ens_file(mut self, ens_file: PathBuf) -> Self {
        self.ens_file = Some(ens_file);
        self
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn current_dir(mut self, current_dir: PathBuf) -> Self {
        self.current_working_dir = Some(current_dir);
        self
    }

    pub fn build(&self) -> Compiler {
        Compiler {
            assembler: &self.assembler,
            ens_file: self.ens_file.as_ref().unwrap(),
            outfile: self
                .outfile
                .as_ref()
                .map(AsRef::as_ref)
                .unwrap_or_else(|| Path::new(STD_OUTFILE)),
            current_dir: self.current_working_dir.as_ref().map(AsRef::as_ref),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Compiler<'a> {
    assembler: &'a Path,
    ens_file: &'a Path,
    outfile: &'a Path,
    current_dir: Option<&'a Path>,
}

pub const STD_OUTFILE: &str = "CDV.bin";

impl<'a> Compiler<'a> {
    // pub fn new<P1: AsRef<Path>, P2: AsRef<Path>>(assembler: &'a P1, ens_file: &'a P2) -> Self {
    //     Self {
    //         assembler: assembler.as_ref(),
    //         ens_file: ens_file.as_ref(),
    //         outfile: Path::new(STD_OUTFILE),
    //     }
    // }

    pub fn run(&self, test_name: &str) -> std::io::Result<Output> {
        let mut c = Command::new(self.assembler);
        c.arg("-e")
            .arg(test_name)
            .arg("-o")
            .arg(current_dir().unwrap().join(self.outfile))
            .arg(current_dir().unwrap().join(self.ens_file));
        if let Some(cwd) = self.current_dir {
            c.current_dir(cwd);
        }
        c.output()
    }
}
