extern crate bindgen;

use regex::Regex;
use std::{env, io, io::Error, io::ErrorKind, path::Path, path::PathBuf, process::exit, process::Command};

struct InstallationPaths {
    r_home: String,
    include: String,
    library: String,
}

fn probe_r_paths() -> io::Result<InstallationPaths> {
    if let Ok(r_home) = env::var("R_HOME") {
        // When R_HOME is set, we assume a standard path layout
        let include:String = Path::new(&r_home).join("include").to_str().unwrap().to_string();
        let library:String = if cfg!(target_os = "windows") {
            Path::new(&r_home).join("bin").to_str().unwrap().to_string()
        } else {
            Path::new(&r_home).join("lib").to_str().unwrap().to_string()
        };

        return Ok(InstallationPaths {
            r_home,
            include,
            library,
        })
    }

    // If R_HOME is not set, we try to execute `R` to find `R_HOME`. Note that this is
    // discouraged, see Section 1.6 of "Writing R Extensions"
    // https://cran.r-project.org/doc/manuals/r-release/R-exts.html#Writing-portable-packages
    let rout = Command::new("R")
        .args(&[
            "-s",
            "-e",
            if cfg!(target_os = "windows") {
                r#"cat(R.home(), R.home('include'), R.home('bin'), sep = '\n')"#
            } else {
                r#"cat(R.home(), R.home('include'), R.home('lib'), sep = '\n')"#
            }
        ])
        .output()?;

    let rout = String::from_utf8_lossy(&rout.stdout);
    let mut lines = rout.lines();

    let r_home = match lines.next() {
        Some(line) => line.to_string(),
        _ => return Err(Error::new(ErrorKind::Other, "Cannot find R home")),
    };

    let include = match lines.next() {
        Some(line) => line.to_string(),
        _ => return Err(Error::new(ErrorKind::Other, "Cannot find R include")),
    };

    let library = match lines.next() {
        Some(line) => line.to_string(),
        _ => return Err(Error::new(ErrorKind::Other, "Cannot find R library")),
    };

    Ok(InstallationPaths {
        r_home,
        include,
        library,
    })
}

fn main() {
    let details = probe_r_paths();

    let details = match details {
        Ok(result) => result,
        Err(error) => {
            println!("Problem locating local R instal: {:?}", error);
            exit(1);
        }
    };

    println!("cargo:rustc-env=R_HOME={}", &details.r_home);
    println!("cargo:r_home={}", &details.r_home); // Becomes DEP_R_R_HOME for clients
    // make sure cargo links properly against library
    println!("cargo:rustc-link-search={}", &details.library);
    println!("cargo:rustc-link-lib=dylib=R");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=wrapper.h");

    // The bindgen::Builder is the main entry point
    // to bindgen, and lets you build up options for
    // the resulting bindings.
    let bindgen_builder = bindgen::Builder::default()
        // These constants from libm break bindgen.
        .blacklist_item("FP_NAN")
        .blacklist_item("FP_INFINITE")
        .blacklist_item("FP_ZERO")
        .blacklist_item("FP_SUBNORMAL")
        .blacklist_item("FP_NORMAL")
        // The input header we would like to generate
        // bindings for.
        .header("wrapper.h")
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks));

        // println!("TARGET: {}",cargo_env("TARGET"));
    // Point to the correct headers
    let bindgen_builder = bindgen_builder.clang_args(&[
        format!("-I{}", &details.include),
        format!("--target={}", std::env::var("TARGET").expect("Could not get the target triple"))
    ]);

    // Finish the builder and generate the bindings.
    let bindings = bindgen_builder
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    // Extract the version number from the R headers.
    let version_matcher = Regex::new(r"pub const R_VERSION ?: ?u32 = (\d+)").unwrap();
    if let Some(version) = version_matcher.captures(bindings.to_string().as_str()) {
        let version = version.get(1).unwrap().as_str().parse::<u32>().unwrap();
        println!("cargo:r_version={}", version);
    } else {
        panic!("failed to find R_VERSION");
    }

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());

    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings to default output path!");

    // Also write the bindings to a folder specified by $LIBRSYS_BINDINGS_DIR, if it exists

    if let Ok(alt_target) = env::var("LIBRSYS_BINDINGS_DIR") {
        let out_path = PathBuf::from(alt_target);

        bindings
            .write_to_file(out_path.join("bindings.rs"))
            .expect("Couldn't write bindings to output path specified by $LIBRSYS_BINDINGS_DIR!");

    }
}
