use std::{fs, io::Write, ops::Deref, path::PathBuf, process::Output, sync::Arc};

use clap::Parser;
use compiler::{Compiler, CompilerBuilder};
use config::ConfigAll;
use emulator::{Emulator, EmulatorBuilder, GPRegister, MemoryData, Operation};
use loadable::Loadable;
use termcolor::{BufferedStandardStream, Color, ColorSpec, WriteColor};
use tests::TestData;
use threadpool::ThreadPool;

mod compiler;
mod config;
mod emulator;
mod iter;
mod loadable;
mod tests;
mod threadpool;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[clap(default_value = "tests.yml")]
    #[clap(short)]
    config: PathBuf,
    #[clap(short)]
    ens_file: Option<PathBuf>,
    #[clap(long)]
    assembler: Option<PathBuf>,
    #[clap(long)]
    emulator: Option<PathBuf>,
    #[clap(long)]
    serie_file: Option<PathBuf>,
}

fn main() {
    let args = Args::parse();

    let conf = ConfigAll::load(args.config).expect("correct test file");
    // dbg!(&conf);
    let assembler = args
        .assembler
        .or(conf.config.assembler)
        .expect("assembler in args or config");
    let emulator = args
        .emulator
        .or(conf.config.emulator)
        .expect("emulator in args or config");
    let ens_file = args
        .ens_file
        .or(conf.config.ens_file)
        .expect("ens_file in args or config");
    let serie_file = args
        .serie_file
        .or(conf.config.serie_file)
        .unwrap_or_else(|| {
            emulator
                .parent()
                .expect("emulator must have parent if serie_file not specified")
                .join("serie")
        });

    let ens_file_contents = fs::read_to_string(&ens_file).unwrap();
    let assembler_builder = CompilerBuilder::new(assembler);
    let emulator_builder = EmulatorBuilder::new(&emulator, &serie_file);

    let start = std::time::Instant::now();
    let threadpool = {
        let (assembler_builder, emulator_builder) =
            (assembler_builder.clone(), emulator_builder.clone());
        ThreadPool::<(String, String, _), _>::new(
            move |(group, name, registers): (String, String, TestData), id| {
                let path: PathBuf = PathBuf::from("tmp").join(format!("{id}"));
                fs::create_dir_all(&path).unwrap();
                let bin_path = path.join("CDV.bin");
                let ens_path = path.join("CDV.ens");
                fs::write(&ens_path, ens_file_contents.clone()).unwrap();
                let mut emulator = emulator_builder.binfile(bin_path.clone()).build();
                let builder = assembler_builder
                    .outfile(bin_path)
                    .ens_file(ens_path)
                    .current_dir(path);
                let assembler = builder.build();
                // println!("Running job {id}: {} {}", group, name);
                let r = run_test(&assembler, &mut emulator, name.as_str(), &registers);
                (group, name, r)
            },
            4,
        )
    };

    let assembler_builder = assembler_builder.ens_file(ens_file);
    let mut groups = 0;
    for (group, tests) in conf.tests.get_tests() {
        for (test_name, registers) in tests {
            threadpool.send_data((group.clone(), test_name, registers.clone()))
        }
        groups += 1;
    }

    let mut results = Vec::with_capacity(groups);
    let mut current = (None, vec![]);
    for (res_g, name, res) in threadpool.results() {
        if let Some(group) = current.0.take() {
            if group == res_g {
                current.1.push((name, res));
                current.0 = Some(group)
            } else {
                results.push((group, current.1));
                current = (None, vec![]);
            }
        } else {
            current.0 = Some(res_g);
            current.1.push((name, res));
        }
    }
    fs::remove_dir_all("tmp").unwrap();
    // let end = std::time::Instant::now();

    // println!("Results: {results:#?}");
    // println!("Time for multithreaded: {} ms", (end - start).as_millis());

    let mut stdout = BufferedStandardStream::stdout(termcolor::ColorChoice::Auto);
    let normal_color_spec = ColorSpec::new();
    let mut ok_color_spec = ColorSpec::new();
    ok_color_spec.set_fg(Some(Color::Green)).set_bold(true);
    let mut error_color_spec = ColorSpec::new();
    error_color_spec.set_fg(Some(Color::Red)).set_bold(true);
    let mut blue_color_spec = ColorSpec::new();
    blue_color_spec.set_fg(Some(Color::Blue)).set_bold(true);

    let mut bold_color_spec = ColorSpec::new();
    bold_color_spec.set_bold(true);

    // let start = std::time::Instant::now();
    let mut failed_tests = 0;
    let mut ok_tests = 0;
    for (group, tests) in results {
        stdout.set_color(&bold_color_spec).unwrap();
        writeln!(stdout, "{group}").unwrap();
        stdout.flush().unwrap();
        for (test_name, result) in tests {
            stdout.set_color(&normal_color_spec).unwrap();
            write!(stdout, "{test_name:>30} ").unwrap();
            stdout.flush().unwrap();
            match result {
                Ok(()) => {
                    ok_tests += 1;
                    stdout.set_color(&ok_color_spec).unwrap();
                    writeln!(stdout, "OK").unwrap();
                    stdout.flush().unwrap();
                }
                Err(x) => {
                    failed_tests += 1;
                    stdout.set_color(&error_color_spec).unwrap();
                    writeln!(stdout, "ERROR").unwrap();
                    stdout.flush().unwrap();
                    stdout.set_color(&normal_color_spec).unwrap();
                    match x {
                        RunError::CompileExec(out) => {
                            writeln!(stdout, "{:>20} compiling: {out}", "").unwrap()
                        }
                        RunError::RunExec(out) => {
                            writeln!(stdout, "{:>20} running: {out}", "").unwrap()
                        }
                        RunError::Compile(out) => {
                            writeln!(stdout, "{:>20} compiling (OUTPUT):", "").unwrap();
                            writeln!(stdout, "STDOUT:").unwrap();
                            stdout.write_all(&out.stdout).unwrap();
                            writeln!(stdout, "STDERR:").unwrap();
                            stdout.write_all(&out.stdout).unwrap();
                            writeln!(stdout).unwrap();
                        }
                        RunError::Run(out) => {
                            writeln!(stdout, "{:>20} running (OUTPUT):", "").unwrap();
                            writeln!(stdout, "STDOUT:").unwrap();
                            stdout.write_all(&out.stdout).unwrap();
                            writeln!(stdout, "STDERR:").unwrap();
                            stdout.write_all(&out.stdout).unwrap();
                            writeln!(stdout).unwrap();
                        }
                        RunError::RegistersFailed(failures) => {
                            for failure in failures {
                                let (name, expected, found) = match failure {
                                    DataFailure::Register(a, b, c) => {
                                        (format!("{a}"), format!("{b}"), format!("{c}"))
                                    }
                                    DataFailure::Memory(a, b, c) => {
                                        (format!("m[0x{a:X}]"), format!("{b:?}"), format!("{c:?}"))
                                    }
                                };
                                stdout.set_color(&normal_color_spec).unwrap();
                                write!(stdout, " =+= ").unwrap();
                                stdout.set_color(&blue_color_spec).unwrap();
                                write!(stdout, "{name}").unwrap();
                                stdout.set_color(&normal_color_spec).unwrap();
                                write!(stdout, " was ").unwrap();
                                stdout.set_color(&error_color_spec).unwrap();
                                write!(stdout, "{found}").unwrap();
                                stdout.set_color(&normal_color_spec).unwrap();
                                write!(stdout, ", but ").unwrap();
                                stdout.set_color(&blue_color_spec).unwrap();
                                write!(stdout, "{expected}").unwrap();
                                stdout.set_color(&normal_color_spec).unwrap();
                                writeln!(stdout, " was expected =+=").unwrap();
                                stdout.flush().unwrap();
                            }
                            writeln!(stdout).unwrap();
                            stdout.flush().unwrap();
                        }
                    }
                    stdout.flush().unwrap();
                }
            }
        }
    }
    // for (group, tests) in conf.tests.get_tests() {
    //     stdout.set_color(&bold_color_spec).unwrap();
    //     writeln!(stdout, "{group}").unwrap();
    //     stdout.flush().unwrap();
    //     for (test_name, registers) in tests {
    //         stdout.set_color(&normal_color_spec).unwrap();
    //         write!(stdout, "{test_name:>30} ").unwrap();
    //         stdout.flush().unwrap();
    //         match run_test(
    //             &assembler_builder.build(),
    //             &mut emulator_builder.build(),
    //             &test_name,
    //             registers,
    //         ) {
    //             Ok(()) => {
    //                 ok_tests += 1;
    //                 stdout.set_color(&ok_color_spec).unwrap();
    //                 writeln!(stdout, "OK").unwrap();
    //                 stdout.flush().unwrap();
    //             }
    //             Err(x) => {
    //                 failed_tests += 1;
    //                 stdout.set_color(&error_color_spec).unwrap();
    //                 writeln!(stdout, "ERROR").unwrap();
    //                 stdout.flush().unwrap();
    //                 stdout.set_color(&normal_color_spec).unwrap();
    //                 match x {
    //                     RunError::CompileExec(out) => {
    //                         writeln!(stdout, "{:>20} compiling: {out}", "").unwrap()
    //                     }
    //                     RunError::RunExec(out) => {
    //                         writeln!(stdout, "{:>20} running: {out}", "").unwrap()
    //                     }
    //                     RunError::Compile(out) => {
    //                         writeln!(stdout, "{:>20} compiling (OUTPUT):", "").unwrap();
    //                         writeln!(stdout, "STDOUT:").unwrap();
    //                         stdout.write_all(&out.stdout).unwrap();
    //                         writeln!(stdout, "STDERR:").unwrap();
    //                         stdout.write_all(&out.stdout).unwrap();
    //                         writeln!(stdout).unwrap();
    //                     }
    //                     RunError::Run(out) => {
    //                         writeln!(stdout, "{:>20} running (OUTPUT):", "").unwrap();
    //                         writeln!(stdout, "STDOUT:").unwrap();
    //                         stdout.write_all(&out.stdout).unwrap();
    //                         writeln!(stdout, "STDERR:").unwrap();
    //                         stdout.write_all(&out.stdout).unwrap();
    //                         writeln!(stdout).unwrap();
    //                     }
    //                     RunError::RegistersFailed(failures) => {
    //                         for failure in failures {
    //                             let (name, expected, found) = match failure {
    //                                 DataFailure::Register(a, b, c) => {
    //                                     (format!("{a}"), format!("{b}"), format!("{c}"))
    //                                 }
    //                                 DataFailure::Memory(a, b, c) => {
    //                                     (format!("m[0x{a:X}]"), format!("{b:?}"), format!("{c:?}"))
    //                                 }
    //                             };
    //                             stdout.set_color(&normal_color_spec).unwrap();
    //                             write!(stdout, " =+= ").unwrap();
    //                             stdout.set_color(&blue_color_spec).unwrap();
    //                             write!(stdout, "{name}").unwrap();
    //                             stdout.set_color(&normal_color_spec).unwrap();
    //                             write!(stdout, " was ").unwrap();
    //                             stdout.set_color(&error_color_spec).unwrap();
    //                             write!(stdout, "{found}").unwrap();
    //                             stdout.set_color(&normal_color_spec).unwrap();
    //                             write!(stdout, ", but ").unwrap();
    //                             stdout.set_color(&blue_color_spec).unwrap();
    //                             write!(stdout, "{expected}").unwrap();
    //                             stdout.set_color(&normal_color_spec).unwrap();
    //                             writeln!(stdout, " was expected =+=").unwrap();
    //                             stdout.flush().unwrap();
    //                         }
    //                         writeln!(stdout).unwrap();
    //                         stdout.flush().unwrap();
    //                     }
    //                 }
    //                 stdout.flush().unwrap();
    //             }
    //         }
    //     }
    // }

    let end = std::time::Instant::now();

    // println!("Results: {results:#?}");
    println!("Time for multithreaded: {} ms", (end - start).as_millis());
    stdout.set_color(&bold_color_spec).unwrap();
    writeln!(stdout).unwrap();
    writeln!(stdout, "{failed_tests:>6} tests failed").unwrap();
    writeln!(stdout, "{ok_tests:>6} tests passed").unwrap();
    stdout.flush().unwrap();

    std::process::exit(failed_tests)
}

#[derive(Debug, Clone, PartialEq)]
enum DataFailure {
    Register(GPRegister, u32, u32),
    Memory(u32, MemoryData, MemoryData),
}

#[derive(Debug)]
enum RunError {
    CompileExec(std::io::Error),
    Compile(Output),
    RunExec(std::io::Error),
    Run(Output),
    RegistersFailed(Vec<DataFailure>),
}

fn run_test(
    assembler: &Compiler,
    emulator: &mut Emulator,
    test_name: &str,
    registers: &TestData,
) -> Result<(), RunError> {
    let mut error_mem = Vec::new();
    let (entrypoint, registers, operations) = match registers {
        TestData::NoSetup(checks) => (None, checks, vec![]),
        TestData::WithSetup {
            name: _,
            entrypoint,
            setup,
            checks,
        } => (
            entrypoint.clone(),
            checks,
            setup
                .iter()
                .filter_map(|x| match x {
                    tests::TestCheck::Register(reg, val) => Some(Operation::SetReg(*reg, *val)),
                    tests::TestCheck::Memory(addr, data) => {
                        if addr % 4 == 0 {
                            let r = data.words().ok().map(|w| Operation::SetMem(*addr, w));
                            if r.is_none() {
                                error_mem.push(*addr);
                            }
                            r
                        } else {
                            error_mem.push(*addr);
                            None
                        }
                    }
                })
                .collect(),
        ),
    };
    if !error_mem.is_empty() {
        eprintln!(
            "Memory setup can only be done at word level, error for addresses: {error_mem:?}"
        );
    }
    let assembled = assembler
        .run(entrypoint.as_deref().unwrap_or(test_name))
        .map_err(RunError::CompileExec)?;
    if !assembled.status.success() {
        return Err(RunError::Compile(assembled));
    }
    // println!("\tCompile OK");
    let memory_tests = registers
        .iter()
        .filter_map(|c| match c {
            tests::TestCheck::Register(_, _) => None,
            tests::TestCheck::Memory(addr, data) => Some((*addr, data.len_real())),
        })
        .collect::<Vec<_>>();

    let run_res = emulator
        .run(&operations, &memory_tests)
        .map_err(|e| match e {
            emulator::EmulatorError::Failure(e) => RunError::Run(e),
            emulator::EmulatorError::IO(e) => RunError::RunExec(e),
        })?;
    // println!("{r1:?}");
    let mut res = vec![];
    for check in registers.deref() {
        match check {
            tests::TestCheck::Register(register, val) => {
                let val = *val as u32;
                let found = run_res.get_reg(register);
                if val != found {
                    res.push(DataFailure::Register(*register, val, found));
                }
            }
            tests::TestCheck::Memory(addr, data) => {
                let val = data.clone();
                let found = run_res.get_mem(*addr).unwrap().clone();
                if val != found {
                    res.push(DataFailure::Memory(*addr, val, found));
                }
            }
        }
    }
    if res.is_empty() {
        Ok(())
    } else {
        Err(RunError::RegistersFailed(res))
    }
}
