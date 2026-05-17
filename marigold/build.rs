use chrono::{DateTime, Local};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use regex::Regex;
use std::fmt::Write;
use std::fs::{DirEntry, File};
use std::io::{ErrorKind, Write as IoWrite};
use std::path::Path;
use std::process::Command;
use std::sync::mpsc::{self, Sender};

#[derive(PartialEq)]
#[allow(unused)]
enum CompileTarget {
    SpirV,
    Wgsl,
}

const TARGET: CompileTarget = CompileTarget::SpirV;

const INPUT_DIRECTORY: &str = "assets/shaders/slang";
const OUTPUT_DIRECTORY: &str = "assets/shaders/target";
const ERROR_DIRECTORY: &str = "shader_compile_errors";

fn has_entrypoint(pattern: &Regex, path: &Path) -> bool {
    let source = std::fs::read_to_string(path).unwrap();
    pattern.is_match(&source)
}

fn compile(slangc: &str, regex: &Regex, debug_info: bool, errors: Sender<String>, path: &Path) {
    // println!("cargo:warning=processing: {}", path.to_string_lossy());

    if !path.is_dir() {
        if !has_entrypoint(regex, path) {
            return;
        }

        let relative_input_path = path.strip_prefix(INPUT_DIRECTORY).unwrap();
        let mut output_path = Path::new(OUTPUT_DIRECTORY).join(relative_input_path);

        if let Some(parent) = output_path.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent).expect("failed to create output directory");
        }

        for ancestor in output_path.ancestors().skip(1) {
            // avoid recursing too far back
            if !ancestor.starts_with(INPUT_DIRECTORY) {
                break;
            }

            // println!("cargo:warning=ancesor = {}", ancestor.to_string_lossy());
            match std::fs::create_dir(ancestor).map_err(|e| e.kind()) {
                Ok(_) => {}
                Err(ErrorKind::AlreadyExists) => {}
                Err(_) => panic!("failed to create parent directory"),
            }
        }

        // remove the extension
        output_path.set_extension("");

        // println!(
        //     "cargo:warning=Compiling {} into {}",
        //     path.to_string_lossy(),
        //     output_path.to_string_lossy()
        // );

        let target = match TARGET {
            CompileTarget::SpirV => "spirv",
            CompileTarget::Wgsl => "wgsl",
        };

        let mut cmd = Command::new(slangc);

        cmd.arg(path)
            .arg("-o")
            .arg(&output_path)
            .arg("-target")
            .arg(target);

        if TARGET == CompileTarget::SpirV {
            cmd.arg("-fvk-use-entrypoint-name");
        }

        if debug_info {
            cmd.arg("-g3"); // maximum debug info
        }

        //println!("cargo:warning={:?}", cmd);

        let output = cmd.output().unwrap();

        if !output.status.success() {
            // let log_file = match log_file {
            //     Some(file) => file,
            //     None => {
            //         // create the file and use it
            //         *log_file = Some(update_log());
            //         log_file.as_mut().unwrap()
            //     }
            // };

            let stdout = String::from_utf8(output.stdout).unwrap();
            let stderr = String::from_utf8(output.stderr).unwrap();

            let mut error = String::new();

            writeln!(
                &mut error,
                "{}\nstdout:\n{}\n\nstderr:\n{}\n",
                path.to_string_lossy(),
                stdout,
                stderr
            )
            .expect("unable to write to shader compilation log file");

            errors.send(error).expect("failed to send shader error");

            println!(
                "cargo:warning=Failed to compile {} into {}, putting detailed compiler error in {}/latest.log",
                path.to_string_lossy(),
                output_path.to_string_lossy(),
                ERROR_DIRECTORY,
            );
        }

        return;
    }

    let entries: Vec<DirEntry> = std::fs::read_dir(path)
        .unwrap()
        .map(|e| e.unwrap())
        .collect();

    rayon::scope(|s| {
        entries.par_iter().for_each(|e| {
            let errors = errors.clone();
            s.spawn(move |_| {
                compile(slangc, regex, debug_info, errors, &e.path());
            })
        });
    });
}

fn update_log() -> File {
    let path = Path::new(ERROR_DIRECTORY).join("latest.log");

    if path.exists() {
        if let Ok(system_time) = std::fs::metadata(&path).and_then(|meta| meta.created()) {
            let datetime: DateTime<Local> = system_time.into();
            let timestamp = datetime.to_rfc3339();

            std::fs::rename(
                &path,
                Path::new(ERROR_DIRECTORY).join(format!("log_{}.log", timestamp)),
            )
            .expect("unable to rename shader compilation log file");
        } else {
            panic!("unable to update shader compilation log file");
        }
    }

    File::create(path).expect("unabel to create shader compilation log file")
}

fn main() {
    println!("cargo:rerun-if-changed=assets/shaders/slang");
    println!("cargo:rerun-if-env-changed=SHADER_DEBUG_INFO");

    let entrypoint_regex = Regex::new(r#"\[\[shader\("(\w+)"\)]]\s*\w+\s+(\w+)\s*\("#).unwrap();

    // use SLANGC environment variable if set, otherwise assume slangc is on the shell PATH
    let slangc = match std::env::var("SLANGC") {
        Ok(slangc) => slangc,
        _ => String::from("slangc"),
    };

    let debug_info = match std::env::var("SHADER_DEBUG_INFO") {
        Ok(flag) => match flag.parse::<u32>() {
            Ok(flag) => flag != 0,
            Err(_) => {
                println!(
                    "cargo:warning=Environment variable SHADER_DEBUG_INFO={} was a non-integral value, assuming false",
                    flag
                );
                false
            }
        },
        _ => false,
    };

    let (tx, rx) = mpsc::channel();

    compile(
        &slangc,
        &entrypoint_regex,
        debug_info,
        tx,
        Path::new(INPUT_DIRECTORY),
    );

    let mut log_file: Option<File> = None;

    for error in rx.iter() {
        let log_file = match &mut log_file {
            Some(file) => file,
            None => {
                // create the file and use it
                log_file = Some(update_log());
                log_file.as_mut().unwrap()
            }
        };

        write!(log_file, "{}", error).expect("failed to write to log file");
    }

    // if log_file is Some then there was at least one error
    if log_file.is_some() {
        panic!("stopping build due to shader compiler errors");
    }
}
