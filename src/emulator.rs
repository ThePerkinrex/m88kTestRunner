use std::{
    collections::HashMap,
    fmt::Debug,
    io::Write,
    iter::repeat,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

use serde::Deserialize;

use crate::{compiler::STD_OUTFILE, iter::IteratorExt};

#[derive(Debug, Clone)]
pub struct EmulatorBuilder {
    emu: PathBuf,
    serie: PathBuf,
    binfile: Option<PathBuf>,
}

impl EmulatorBuilder {
    pub fn new<EmuPath: AsRef<Path>, SeriePath: AsRef<Path>>(
        emu: EmuPath,
        serie: SeriePath,
    ) -> Self {
        Self {
            emu: emu.as_ref().to_path_buf(),
            serie: serie.as_ref().to_path_buf(),
            binfile: None,
        }
    }

    pub fn binfile_mut(&mut self, binfile: PathBuf) -> &mut Self {
        self.binfile = Some(binfile);
        self
    }

    pub fn binfile(&self, binfile: PathBuf) -> Self {
        let mut s = self.clone();
        s.binfile_mut(binfile);
        s
    }

    pub fn build(&self) -> Emulator {
        let mut cmd = Command::new(&self.emu);
        cmd.arg("-c")
            .arg(&self.serie)
            .arg(
                self.binfile
                    .as_ref()
                    .map(AsRef::as_ref)
                    .unwrap_or_else(|| Path::new(STD_OUTFILE)),
            )
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .stdout(Stdio::piped());
        Emulator { command: cmd }
    }
}

#[derive(Debug)]
pub struct Emulator {
    command: Command,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct GPRegister(u8);

impl GPRegister {
    pub fn new(n: u8) -> Option<Self> {
        (n < 32).then_some(Self(n))
    }
}

impl std::fmt::Debug for GPRegister {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "r{:02}", self.0)
    }
}

impl std::fmt::Display for GPRegister {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub enum Operation {
    SetReg(GPRegister, u32),
    SetMem(u32, Vec<u32>),
}

pub enum EmulatorError {
    Failure(Output),
    IO(std::io::Error),
}

impl From<std::io::Error> for EmulatorError {
    fn from(e: std::io::Error) -> Self {
        Self::IO(e)
    }
}

impl Emulator {
    // pub fn new<EmuPath: AsRef<Path>, SeriePath: AsRef<Path>>(
    //     emu: EmuPath,
    //     serie: SeriePath,
    // ) -> Self {
    //     EmulatorBuilder::new(&emu, &serie).build()
    // }

    pub fn run(
        &mut self,
        operations: &[Operation],
        memory_res: &[(u32, u32)],
    ) -> Result<RunResult, EmulatorError> {
        let mut child = self.command.spawn()?;
        let mut stdin = child.stdin.take().unwrap();
        // let mut stdout = BufReader::new(emulated.stdout.take().unwrap());
        let mut op_skip = 0;
        // let mut set_mem = false;
        for op in operations {
            match op {
                Operation::SetReg(GPRegister(n), val) => {
                    writeln!(stdin, "r {n} 0x{val:x}")?;
                    op_skip += 11;
                }
                Operation::SetMem(addr, data) => {
                    // set_mem = true;
                    for (i, word) in data.iter().enumerate() {
                        writeln!(stdin, "I {} 0x{word:08x}", *addr + i as u32 * 4)?;
                        stdin.flush()?;
                        op_skip += 1;
                    }
                }
            }
            stdin.flush()?;
        }
        stdin.write_all(b"e\n")?;
        stdin.flush()?;
        for (mem, len) in memory_res {
            let word_len = (len / 4) + u32::from(len % 4 > 0);
            writeln!(stdin, "v {mem} {word_len}")?;
            stdin.flush()?;
        }
        stdin.write_all(b"q\n")?;
        stdin.flush()?;
        let output = child.wait_with_output()?;
        if !output.status.success() {
            return Err(EmulatorError::Failure(output));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(RunResult::new(
            stdout.lines().skip(op_skip + 13),
            memory_res,
        ))
    }
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryData {
    Bytes(Vec<u8>),
    Byte(u8),
    HalfWord(u16),
    Word(u32),
    DoubleWord(u64),
    Text(String),
}

impl Debug for MemoryData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bytes(arg0) => hexdump(f, arg0),
            Self::Byte(arg0) => f.debug_tuple("Byte").field(arg0).finish(),
            Self::HalfWord(arg0) => f.debug_tuple("HalfWord").field(arg0).finish(),
            Self::Word(arg0) => write!(f, "Word(0x{:X})", arg0),
            Self::DoubleWord(arg0) => f.debug_tuple("DoubleWord").field(arg0).finish(),
            Self::Text(arg0) => f.debug_tuple("Text").field(arg0).finish(),
        }
    }
}

fn hexdump(f: &mut std::fmt::Formatter<'_>, b: &[u8]) -> std::fmt::Result {
    const HD_BYTES: usize = 16;
    const SECTIONS: usize = 2;
    const SECTIONED_BYTES: usize = HD_BYTES / SECTIONS;
    const EMPTY: &[u8] = &[];

    writeln!(f)?;
    for line in b.chunks(HD_BYTES) {
        let iter = line
            .chunks(SECTIONED_BYTES)
            .chain(repeat(EMPTY))
            .take(SECTIONS);
        for section in iter
            .clone()
            .map(|s| s.iter().map(Some).chain(repeat(None)).take(SECTIONED_BYTES))
        {
            for b in section {
                if let Some(b) = b {
                    write!(f, "{b:02x} ")?;
                } else {
                    write!(f, "   ")?;
                }
            }
            write!(f, "   ")?;
        }

        for section in iter {
            for b in section {
                let c = *b as char;
                write!(f, "{}", if c.is_ascii_graphic() { c } else { '.' })?;
            }
            write!(f, " ")?;
        }
        writeln!(f)?;
    }

    Ok(())
}

// const fn word_aligned_len(len: u32) -> u32 {
//     if len % 4 == 0 {
//         len
//     } else {
//         len + (4 - len % 4)
//     }
// }

impl MemoryData {
    // pub fn len_try_match_word(&self) -> u32 {
    //     use MemoryData::*;
    //     match self {
    //         Bytes(x) => word_aligned_len(x.len() as u32),
    //         Byte(_) => 1,
    //         HalfWord(_) => 2,
    //         Word(_) => 4,
    //         DoubleWord(_) => 8,
    //         Text(s) => word_aligned_len(s.len() as u32 + 1),
    //     }
    // }

    pub fn len_real(&self) -> u32 {
        use MemoryData::*;
        match self {
            Bytes(x) => x.len() as u32,
            Byte(_) => 1,
            HalfWord(_) => 2,
            Word(_) => 4,
            DoubleWord(_) => 8,
            Text(s) => (s.len() + 1) as u32,
        }
    }

    pub fn words(&self) -> Result<Vec<u32>, ()> {
        match self {
            Self::Bytes(b) => Ok(b
                .chunks(4)
                .map(|x| {
                    u32::from_le_bytes([
                        x[0],
                        x.get(1).copied().unwrap_or_default(),
                        x.get(2).copied().unwrap_or_default(),
                        x.get(3).copied().unwrap_or_default(),
                    ])
                })
                .collect()),
            Self::Word(w) => Ok(vec![*w]),
            Self::DoubleWord(d) => Ok(vec![*d as u32, (d >> 32) as u32]),
            Self::Text(s) => Ok(s
                .bytes()
                .chain(std::iter::once(0))
                .chunks(4)
                .map(|x| {
                    u32::from_le_bytes([
                        x[0],
                        x.get(1).copied().unwrap_or_default(),
                        x.get(2).copied().unwrap_or_default(),
                        x.get(3).copied().unwrap_or_default(),
                    ])
                })
                .collect()),
            _ => Err(()),
        }
    }
}

pub struct RunResult {
    registers: [u32; 32],
    memory: HashMap<u32, MemoryData>,
}

impl RunResult {
    fn new<'a, I: Iterator<Item = &'a str> + Clone>(lines: I, memory: &[(u32, u32)]) -> Self {
        let lines = lines.skip(2); // Special registers
        let mut registers = [0; 32];
        for (i, reg) in lines
            .clone()
            .take(8)
            .flat_map(|s| s.trim().split('h'))
            .filter_map(|s| (!s.trim().is_empty()).then(|| &s.trim()[6..]))
            .enumerate()
            .map(|(i, reg)| (i + 1, u32::from_str_radix(reg, 16).unwrap()))
        {
            registers[i] = reg;
        }
        let mut extra = lines.skip(8);
        let mut memory_res: HashMap<u32, MemoryData> = HashMap::with_capacity(memory.len());
        for (addr, len) in memory {
            extra.next();
            extra.next();
            let words = len / 4 + u32::from(len % 4 > 0);
            let lines = len / 16 + u32::from((addr + len) % 16 > 0);

            // println!("{lines}l {words}w");
            // let initial_word_addr = (addr / 4) * 4;
            let mut words = Vec::<u32>::with_capacity(words as usize);
            for i in 0..lines {
                let line = extra.next().unwrap();
                let addr = addr + i * (16 - (addr % 16));
                for shift in ((addr % 16) / 4)..4 {
                    let num_start = 17 + shift as usize * 13;
                    if line.len() <= num_start {
                        break;
                    }
                    let num = u32::from_str_radix(&line[num_start..(num_start + 8)], 16)
                        .unwrap()
                        .to_be();
                    words.push(num);
                }
                // println!("{} >{} {len} {line:?}", addr - addr % 16, (addr % 16) / 4)
            }
            if *len == 1 {
                memory_res.insert(
                    *addr,
                    MemoryData::Byte(words[0].to_le_bytes()[*addr as usize % 4]),
                );
            } else if addr % 2 == 0 && *len == 2 {
                let bytes =
                    &words[0].to_le_bytes()[(*addr as usize % 2)..((*addr as usize % 2) + 2)];
                memory_res.insert(
                    *addr,
                    MemoryData::HalfWord(u16::from_le_bytes([bytes[0], bytes[1]])),
                );
            } else if addr % 4 == 0 && *len == 4 {
                memory_res.insert(*addr, MemoryData::Word(words[0]));
            } else if addr % 8 == 0 && *len == 8 {
                memory_res.insert(
                    *addr,
                    MemoryData::DoubleWord(words[0] as u64 + ((words[1] as u64) << 32)),
                );
            } else {
                memory_res.insert(
                    *addr,
                    MemoryData::Bytes(
                        words
                            .iter()
                            .flat_map(|x| x.to_le_bytes())
                            .skip(*addr as usize % 4)
                            .take(*len as usize)
                            .collect(),
                    ),
                );
            }
        }
        // for (addr, k) in &memory_res {
        //     println!("{addr}: {k:?}")
        // }
        Self {
            registers,
            memory: memory_res,
        }
    }

    pub const fn get_reg(&self, reg: &GPRegister) -> u32 {
        self.registers[reg.0 as usize]
    }

    pub fn get_mem(&self, addr: u32) -> Option<&MemoryData> {
        self.memory.get(&addr)
    }
}
