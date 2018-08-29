//! GCNO source path categorization.
//!
//! A GCNO file contains the source path where a function and line of code is found. Due to inlining and static linking,
//! functions from the standard libraries and extern crates are also counted. This causes report to include a lot of
//! unnecessary information about these crates we did not write.
//!
//! This module provides a function to analyze the source path and determines if it is interesting or not. User can then
//! selectively hide those reports if the category is not interesting.

use cov::IntoStringLossy;

use home::cargo_home;

use std::path::MAIN_SEPARATOR;
use std::str::FromStr;

/// Path to the hard-coded Rust source of libraries built by macOS builders on Travis CI.
const MACOS_RUSTSRC_DIR: &str = "/Users/travis/build/rust-lang/rust/";
/// Path to the hard-coded Rust source of libraries built by Docker-based builders (everything that is not macOS or
/// Windows) on Travis CI.
const DOCKER_RUSTSRC_DIR: &str = "/checkout/";
/// Path to the hard-coded Rust source of libraries built by Windows builders on AppVeyor.
const WINDOWS_RUSTSRC_DIR: &str = r"C:\projects\rust\";

lazy_static! {
    /// The path to the local Cargo registry, where the source code of external crates are copied to.
    ///
    /// The string should be equal to `$CARGO_HOME/registry/src` where `$CARGO_HOME` is `~/.cargo` by default.
    static ref REGISTRY_PATH: String = {
        let mut cargo_home = cargo_home().expect("$CARGO_HOME is undefined");
        cargo_home.push("registry");
        cargo_home.push("src");
        let mut registry_path = cargo_home.into_string_lossy();
        registry_path.push(MAIN_SEPARATOR);
        registry_path
    };
}

bitflags! {
    /// The type of source path.
    pub struct SourceType: u8 {
        /// The path is in the local workspace.
        const LOCAL = 1;
        /// The "path" is part of macro declaration.
        const MACROS = 2;
        /// Unknown kind of path.
        const UNKNOWN = 4;
        /// The path is of external crates.
        const CRATES = 8;
        /// The path is in the Rust standard libraries.
        const RUSTSRC = 16;

        /// The default set of interesting source paths.
        const DEFAULT = SourceType::LOCAL.bits | SourceType::MACROS.bits | SourceType::UNKNOWN.bits;
    }
}

/// The error raised when [`SourceType::from_str()`] encounters an unrecognized string.
///
/// [`SourceType::from_str()`]: ./struct.SourceType.html#method.from_str
#[derive(Debug)]
pub struct UnsupportedSourceTypeName;

impl SourceType {
    /// Parses an iterator of strings using [`from_str()`], and returns the union of all bitflags.
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedSourceTypeName`] when `from_str()` fails.
    ///
    /// [`from_str()`]: #method.from_str
    /// [`UnsupportedSourceTypeName`]: ./struct.UnsupportedSourceTypeName.html
    pub fn from_multi_str<'a, I>(strings: I) -> Result<SourceType, UnsupportedSourceTypeName>
    where
        I: Iterator<Item = &'a str>,
    {
        let mut res = SourceType::empty();
        for s in strings {
            res |= s.parse()?;
        }
        Ok(res)
    }

    /// Obtains the path prefix so that
    pub fn prefix(self) -> &'static str {
        match self {
            SourceType::LOCAL => ".",
            SourceType::RUSTSRC => "«rust»",
            SourceType::CRATES => "«crates»",
            _ => "",
        }
    }
}


impl FromStr for SourceType {
    type Err = UnsupportedSourceTypeName;
    fn from_str(s: &str) -> Result<SourceType, UnsupportedSourceTypeName> {
        Ok(match s {
            "local" => SourceType::LOCAL,
            "macros" => SourceType::MACROS,
            "rustsrc" => SourceType::RUSTSRC,
            "crates" => SourceType::CRATES,
            "unknown" => SourceType::UNKNOWN,
            "all" => SourceType::all(),
            _ => return Err(UnsupportedSourceTypeName),
        })
    }
}

/// Analyzes the the source path and obtain its corresponding [`SourceType`].
///
/// `crates_path` should be the string representation of the workspace path. If the `path` starts with `crates_path`, it
/// will be considered to be [`SourceType::LOCAL`].
///
/// The return type is a 2-tuple. The second type (`usize`) is the number of bytes should be removed from the prefix of
/// `path` for human display. This is used in [`simplify_source_path` filter of the default Tera
/// template](../template/fn.new.html).
///
/// # Examples
///
/// ```
/// use cargo_cov::sourcepath::*;
///
/// let source_path = "/Users/travis/build/rust-lang/rust/src/libstd/lib.rs";
/// let (source_type, prefix_len) = identify_source_path(source_path, "/workspace/path");
/// assert_eq!(source_type, SourceType::RUSTSRC);
///
/// // This is how `simplify_source_path` is created.
/// let simplified_path = format!("{}/{}", source_type.prefix(), &source_path[prefix_len..]);
/// assert_eq!(simplified_path, "«rust»/src/libstd/lib.rs");
/// ```
///
/// [`SourceType`]: ./struct.SourceType.html
/// [`SourceType::LOCAL`]: ./constant.SourceType::LOCAL.html
pub fn identify_source_path(path: &str, crates_path: &str) -> (SourceType, usize) {
    if path.starts_with(crates_path) {
        (SourceType::LOCAL, crates_path.len())
    } else if path.starts_with(&*REGISTRY_PATH) {
        let subpath = &path[REGISTRY_PATH.len()..];
        let first_slash = subpath.find(MAIN_SEPARATOR).map_or(0, |s| s + MAIN_SEPARATOR.len_utf8());
        (SourceType::CRATES, REGISTRY_PATH.len() + first_slash)
    } else if path.starts_with('<') && path.ends_with(" macros>") {
        (SourceType::MACROS, 0)
    } else if path.starts_with(MACOS_RUSTSRC_DIR) {
        (SourceType::RUSTSRC, MACOS_RUSTSRC_DIR.len())
    } else if path.starts_with(DOCKER_RUSTSRC_DIR) {
        (SourceType::RUSTSRC, DOCKER_RUSTSRC_DIR.len())
    } else if path.starts_with(WINDOWS_RUSTSRC_DIR) {
        (SourceType::RUSTSRC, WINDOWS_RUSTSRC_DIR.len())
    } else {
        (SourceType::UNKNOWN, 0)
    }
}
