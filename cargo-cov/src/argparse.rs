//! Extra functions for command line argument parsing.

use error::Result;
use sourcepath::SourceType;
use utils::{join_3, parent_3};

use clap::ArgMatches;

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

lazy_static! {
    /// The list of special arguments. See [`update_from_clap()`] for detail.
    ///
    /// [`update_from_clap()`]: ./fn.update_from_clap.html
    static ref SPECIALS: HashSet<&'static str> = [
        "manifest-path",
        "target",
        "profiler",
    ].iter().cloned().collect();

    /// The list of `rustc` flags that take a value (i.e. of the form `--foo bar`).
    static ref RUSTC_FLAGS_WITH_VALUES: HashSet<&'static str> = [
        "--allow",
        "--cap-lints",
        "--cfg",
        "--codegen",
        "--color",
        "--crate-name",
        "--crate-type",
        "--deny",
        "--emit",
        "--error-format",
        "--explain",
        "--extern",
        "--forbid",
        "--out-dir",
        "--pretty",
        "--print",
        "--sysroot",
        "--target",
        "--unpretty",
        "--warn",
        "-A",
        "-C",
        "-D",
        "-F",
        "-l",
        "-L",
        "-o",
        "-W",
        "-Z",
    ].iter().cloned().collect();
}

/// Map of special arguments.
///
/// The key is the `clap` argument name, and the value is the value of the argument. See [`update_from_clap()`] for
/// detail.
///
/// [`update_from_clap()`]: ./fn.update_from_clap.html
pub type SpecialMap<'a> = HashMap<&'static str, &'a OsStr>;

/// If `matches` contains any of *special arguments*, read their values and insert them into `specialized`.
///
/// In `cargo cov`, special arguments are arguments needed to be known for every `cargo cov` subcommand, in order to
/// setup the coverage environment. They take values and can appear before or after the subcommand, for instance the
/// following two commands should have the same effect:
///
/// ```sh
/// cargo cov --manifest-path Cargo.toml clean
/// cargo cov clean --manifest-path Cargo.toml
/// ```
///
/// Currently the list of special arguments are:
///
/// * `--manifest-path`
/// * `--target`
/// * `--profiler`
pub fn update_from_clap<'a>(matches: &'a ArgMatches, specialized: &mut SpecialMap<'a>) {
    for name in SPECIALS.iter() {
        if let Some(value) = matches.value_of_os(name) {
            specialized.insert(name, value);
        }
    }
}

/// Finds out the path to the crate `rustc` is building from its arguments. If the path is a descendant of
/// `workspace_path`, returns true.
pub fn is_rustc_compiling_local_crate<'a, I: IntoIterator<Item = &'a OsStr>>(args: I, workspace_path: &Path) -> bool {
    let mut skip_next = false;
    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }

        if let Some(s) = arg.to_str() {
            if RUSTC_FLAGS_WITH_VALUES.contains(s) {
                skip_next = true;
                continue;
            } else if s == "--" {
                return false;
            } else if s.starts_with('-') {
                continue;
            }
        }

        let crate_path = Path::new(arg);
        return crate_path.is_relative() || crate_path.starts_with(workspace_path);
    }
    false
}

/// Extracts *special arguments* from the iterator of arguments.
///
/// The values will be inserted to the `specialized` map. Remaining arguments are returned as a vector.
///
/// The meaning of special arguments is described in [`update_from_clap()`]. When used in the `build`, `run` and `test`
/// external subcommands, `clap` will be distinguish the special arguments from the rest that are forwarded to the
/// corresponding `cargo` subcommand. This function performs the second command-line argument to partition these special
/// arguments from all other arguments.
///
/// [`update_from_clap()`]: ./fn.update_from_clap.html
pub fn normalize<'a, I: IntoIterator<Item = &'a OsStr>>(args: I, specialized: &mut SpecialMap<'a>) -> Vec<&'a OsStr> {
    let mut normalized = Vec::new();

    let mut current_name = None;
    let mut encountered_double_minus = false;
    for arg in args {
        if !encountered_double_minus {
            if let Some(name) = current_name.take() {
                specialized.insert(name, arg);
                continue;
            }

            if let Some(s) = arg.to_str() {
                if s.starts_with("--") {
                    let s = &s[2..];
                    if s.is_empty() {
                        encountered_double_minus = true;
                    } else if let Some(name) = SPECIALS.get(s) {
                        current_name = Some(name);
                        continue;
                    } else if let Some(eq_index) = s.find('=') {
                        if let Some(name) = SPECIALS.get(&s[..eq_index]) {
                            let value = OsStr::new(&s[(eq_index + 1)..]);
                            specialized.insert(name, value);
                            continue;
                        }
                    }
                }
            }
        }

        normalized.push(arg);
    }

    normalized
}


/// Parsed command-line configuration for the `report` subcommand.
pub struct ReportConfig<'a> {
    pub workspace_path: Cow<'a, Path>,
    pub gcno_path: Cow<'a, Path>,
    pub gcda_path: Cow<'a, Path>,
    pub output_path: Cow<'a, Path>,
    pub template_name: &'a OsStr,
    pub allowed_source_types: SourceType,
}

impl<'a> ReportConfig<'a> {
    /// Parses the command-line arguments for the `report` subcommand.
    ///
    /// Returns None if
    pub fn parse(matches: &'a ArgMatches<'a>, cov_build_path: Result<PathBuf>) -> Result<ReportConfig<'a>> {
        fn match_or_else<'a, F: FnOnce() -> PathBuf>(matches: &'a ArgMatches<'a>, name: &str, default: F) -> Cow<'a, Path> {
            match matches.value_of_os(name) {
                Some(path) => Cow::Borrowed(Path::new(path)),
                None => Cow::Owned(default()),
            }
        }


        let (workspace_path, cov_build_path) = match (matches.value_of_os("workspace"), cov_build_path) {
            (Some(workspace_path), _) => {
                let workspace_path = Path::new(workspace_path);
                let cov_build_path = join_3(workspace_path, "target", "cov", "build");
                (Cow::Borrowed(workspace_path), cov_build_path)
            },
            (_, Ok(cov_build_path)) => (Cow::Owned(parent_3(&cov_build_path).to_owned()), cov_build_path),
            (None, Err(e)) => return Err(e),
        };

        let gcno_path = match_or_else(matches, "gcno", || cov_build_path.join("gcno"));
        let gcda_path = match_or_else(matches, "gcda", || cov_build_path.join("gcda"));
        let output_path = match_or_else(matches, "output", || join_3(&workspace_path, "target", "cov", "report"));

        let template_name = matches.value_of_os("template").unwrap_or_else(|| OsStr::new("html"));
        let allowed_source_types = matches.values_of("include").map_or(SourceType::DEFAULT, |it| SourceType::from_multi_str(it).expect("SourceType"));

        Ok(ReportConfig {
            workspace_path,
            gcno_path,
            gcda_path,
            output_path,
            template_name,
            allowed_source_types,
        })
    }
}
