//! Provides support for integrating into the Fish shell.
//!
//! This support module will allow the application to generate a shell script that is evaluated
//! once the application has exited. The location of the script will depend on the value of the
//! `AWS_LOGIN_SCRIPT` environment variable.

use crate::app::{self, ErrorContext};
use crate::util::config;
use std::io::Write;
use std::{env, fs, path};

/// The comment used to check if the integration script is installed.
///
/// The presence of this comment in the profile startup script will inform the application that
/// the integration script has already been installed. If the integration needs to be re-done,
/// the user must undo the integration.
const INSTALLED_COMMENT: &str = "# Integrate aws-login into the shell environment.";

/// The name of the environment variable used to specify the shell script path.
///
/// The file path defined in this environment variable will be created if it does not already
/// exist, and then appended to as changes are specified for the environment. Once the utility
/// exits, the parent process is expected to evaluate and then clean up the file.
const SCRIPT_PATH: &str = "AWS_LOGIN_SCRIPT";

/// Manages the current Fish environment.
pub struct Environment {
    /// The file that will be used to evaluate shell code.
    file: fs::File,
}

impl super::Environment for Environment {
    fn set_var(&mut self, name: &str, value: &str) -> crate::app::Result<()> {
        write!(self.file, "set -gx {} \"{}\"", name, value)
            .map_err(app::Error::from)
            .with_context(|| "Could not set environment variable.".to_owned())
    }
}

impl Default for Environment {
    fn default() -> Self {
        let path = path::PathBuf::from(
            env::var(SCRIPT_PATH).expect("Unable to determine where to write the shell script to."),
        );

        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap_or_else(|_| panic!("{}: Unable to open the file for writing.", path.display()));

        Self { file }
    }
}

/// Manages the integration of the application into a Fish environment.
pub struct Setup {
    /// The path to the profile startup script.
    script: path::PathBuf,
}

impl Setup {
    /// Creates a new instance of [`Setup`] for managing Fish integration.
    pub fn new(profile: Option<&str>) -> Self {
        let script = profile
            .map(path::PathBuf::from)
            .unwrap_or_else(get_default_profile);

        Self { script }
    }
}

impl super::Setup for Setup {
    fn generate_script(&self) -> String {
        include_str!("init.fish")
            .replace("{AWS_LOGIN}", &config::BIN_NAME)
            .replace("{AWS_LOGIN_SHELL}", super::SHELL_NAME)
    }

    fn install(&self) -> app::Result<()> {
        let parent_dir = self.script.parent().unwrap();

        if !parent_dir.exists() {
            fs::create_dir_all(parent_dir)
                .map_err(|error| {
                    app::Error::new(error.raw_os_error().unwrap_or(1))
                        .with_message(format!("{}", error))
                })
                .with_context(|| {
                    "Could not create the directory containing the profile script.".to_owned()
                })?;
        }

        let mut handle = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.script)?;

        writeln!(handle, "\n{}", INSTALLED_COMMENT)?;
        writeln!(handle, "{} shell init -s fish | source", *config::BIN_NAME)?;

        Ok(())
    }

    fn is_installed(&self) -> app::Result<bool> {
        if self.script.exists() {
            let contents = fs::read_to_string(&self.script)?;

            if contents.contains(INSTALLED_COMMENT) {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

/// Generates the path to the default profile script location.
fn get_default_profile() -> path::PathBuf {
    home::home_dir()
        .expect("The home directory could not be determined.")
        .join(".config")
        .join("fish")
        .join("config.fish")
}
