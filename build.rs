use chrono::{DateTime, Local};
use regex::Regex;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::Command;

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

fn compile(slangc: &str, regex: &Regex, log_file: &mut Option<File>, path: &Path) {
    if !path.is_dir() {
        if !has_entrypoint(regex, path) {
            return;
        }

        let relative_input_path = path.strip_prefix(INPUT_DIRECTORY).unwrap();
        let mut output_path = Path::new(OUTPUT_DIRECTORY).join(relative_input_path);

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

        //println!("cargo:warning={:?}", cmd);

        let output = cmd.output().unwrap();

        if !output.status.success() {
            let log_file = match log_file {
                Some(file) => file,
                None => {
                    // create the file and use it
                    *log_file = Some(update_log());
                    log_file.as_mut().unwrap()
                }
            };

            let stdout = String::from_utf8(output.stdout).unwrap();
            let stderr = String::from_utf8(output.stderr).unwrap();

            writeln!(
                log_file,
                "{}\nstdout:\n{}\n\nstderr:\n{}\n",
                path.to_string_lossy(),
                stdout,
                stderr
            )
            .expect("unable to write to shader compilation log file");

            println!(
                "cargo:warning=Failed to compile {} into {}, putting detailed compiler error in {}/latest.log",
                path.to_string_lossy(),
                output_path.to_string_lossy(),
                ERROR_DIRECTORY,
            );
        }

        return;
    }

    for entry in std::fs::read_dir(path).unwrap() {
        compile(slangc, regex, log_file, &entry.unwrap().path());
    }
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
    println!("cargo::rerun-if-changed=assets/shaders/slang");

    let entrypoint_regex = Regex::new(r#"\[\[shader\("(\w+)"\)]]\s*\w+\s+(\w+)\s*\("#).unwrap();

    // use SLANGC environment variable if set, otherwise assume slangc is on the shell PATH
    let slangc = match std::env::var("SLANGC") {
        Ok(slangc) => slangc,
        _ => String::from("slangc"),
    };

    let mut log_file: Option<File> = None;

    compile(
        &slangc,
        &entrypoint_regex,
        &mut log_file,
        Path::new(INPUT_DIRECTORY),
    );

    // if log_file is Some then there was at least one error
    if log_file.is_some() {
        panic!("stopping build due to shader compiler errors");
    }
}
