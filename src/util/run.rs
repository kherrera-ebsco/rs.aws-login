//! Provides a simplified and well integrated interface to [`Command`].

use crate::app::{Context, Result};
use crate::err;
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::join;
use tokio::process::Command;
use tokio::runtime::Runtime;

/// Simplifies the building of a new [`Command`] instance.
pub struct Run {
    /// The arguments used with the builder.
    #[cfg(debug_assertions)]
    arguments: Vec<String>,

    /// The process builder.
    builder: Command,
}

impl Run {
    /// Adds an argument to the process builder.
    ///
    /// ```
    /// let mut run = Run::new("my-app")
    ///     .arg("arg1")
    ///     .arg("arg2")
    ///     .arg("arg3");
    /// ```
    pub fn arg(&mut self, arg: &str) -> &mut Self {
        self.builder.arg(arg);

        #[cfg(debug_assertions)]
        self.arguments.push(arg.to_owned());

        self
    }

    /// Returns the arguments added to the builder.
    ///
    /// ```
    /// let mut run = Run::new("my-app")
    ///     .arg("arg1")
    ///     .arg("arg2")
    ///     .arg("arg3");
    ///
    /// let args = run.get_args();
    /// ```
    #[cfg(test)]
    pub fn get_args(&self) -> &Vec<String> {
        &self.arguments
    }

    /// Creates a new instance for the specified command line application.
    ///
    /// ```
    /// let mut run = Run::new("my-app");
    /// ```
    pub fn new(name: &str) -> Self {
        Self {
            #[cfg(debug_assertions)]
            arguments: Vec::new(),
            builder: Command::new(name),
        }
    }

    /// Runs the command and returns its output.
    ///
    /// ```
    /// use crate::app;
    /// use crate::util::run;
    /// use structopt::StructOpts;
    ///
    /// let app = app::Application::from_args();
    /// let mut context = app::ApplicationContext::new(&app);
    ///
    /// let output = run::Run::new("aws")
    ///     .with_aws_options(&context)
    ///     .arg("eks")
    ///     .arg("list-clusters")
    ///     .output()?;
    ///
    /// println!("{}", output);
    /// ```
    ///
    /// If the command exits with a non-zero status, an [`Err`] for [`Result`] will be returned,
    /// with the error output being used as the message. It is recommended that context be added
    /// for these errors.
    pub fn output(&mut self) -> Result<String> {
        Runtime::new()?.block_on(async {
            let output = self.builder.stdin(Stdio::inherit()).output().await?;

            if output.status.success() {
                let string = String::from_utf8_lossy(output.stdout.as_slice());

                Ok((*string).to_string())
            } else {
                let message = format!("{}", String::from_utf8_lossy(output.stderr.as_slice()));
                let status = output.status.code().unwrap_or(1);

                err!(status, message);
            }
        })
    }

    /// Runs the command and passes its output through the context streams.
    ///
    /// ```
    /// use crate::app;
    /// use crate::util::run;
    /// use structopt::StructOpts;
    ///
    /// let app = app::Application::from_args();
    /// let mut context = app::ApplicationContext::new(&app);
    ///
    /// run::Run::new("aws")
    ///     .with_aws_options(&context)
    ///     .arg("eks")
    ///     .arg("list-clusters")
    ///     .pass_through(&mut context)?;
    /// ```
    pub fn pass_through(&mut self, context: &mut impl Context) -> Result<()> {
        Runtime::new()?.block_on(async {
            let mut child = self
                .builder
                .stderr(Stdio::piped())
                .stdin(Stdio::inherit())
                .stdout(Stdio::piped())
                .spawn()?;

            let stderr_source = child.stderr.take();
            let stderr_lock = context.error();

            let stdout_source = child.stdout.take();
            let stdout_lock = context.output();

            let (result, _, _) = join!(
                child.wait(),
                async {
                    if let Some(mut stderr_source) = stderr_source {
                        let mut stderr_target = stderr_lock.lock().unwrap();
                        let mut buffer = vec![0];

                        loop {
                            match stderr_source.read(&mut buffer).await {
                                Ok(0) => break,
                                Ok(_) => {
                                    stderr_target.write_all(&buffer)?;
                                    stderr_target.flush()?;
                                }
                                Err(error) => err!(1, "{}", error),
                            }
                        }
                    }

                    Ok(())
                },
                async {
                    if let Some(mut stdout_source) = stdout_source {
                        let mut stdout_target = stdout_lock.lock().unwrap();
                        let mut buffer = vec![0];

                        loop {
                            match stdout_source.read(&mut buffer).await {
                                Ok(0) => break,
                                Ok(_) => {
                                    stdout_target.write_all(&buffer)?;
                                    stdout_target.flush()?;
                                }
                                Err(error) => err!(1, "{}", error),
                            }
                        }
                    }

                    Ok(())
                }
            );

            let status = result?;

            if !status.success() {
                err!(status.code().unwrap_or(1));
            }

            Ok(())
        })
    }

    /// Assumes that the AWS CLI is being invoked and adds additional arguments.
    ///
    /// The given context will be used to add the `--profile` and `--region` options for the AWS
    /// CLI if the user has provided any. This will allow the AWS CLI to inherit any profile and
    /// region information provided to this application.
    ///
    /// ```
    /// use crate::app;
    /// use structopt::StructOpts;
    ///
    /// let app = app::Application::from_args();
    /// let context = app::ApplicationContext::new(&app);
    ///
    /// Run::new("aws")
    ///     .with_aws_options(&context)
    ///     .arg("configure")
    ///     .arg("get")
    ///     .arg("sso_start_url")
    /// ```
    pub fn with_aws_options(&mut self, context: &impl Context) -> &mut Self {
        if let Some(profile) = context.profile() {
            self.arg("--profile").arg(profile);
        }

        if let Some(region) = context.region() {
            self.arg("--region").arg(region);
        }

        self
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::util::test::TestContext;

    #[test]
    fn argument_building() {
        let args = Run::new("test")
            .arg("arg1")
            .arg("arg2")
            .arg("arg3")
            .get_args()
            .to_owned();

        assert_eq!(args.as_ref(), vec!["arg1", "arg2", "arg3"]);
    }

    #[test]
    fn aws_options_added() {
        let context = TestContext::default()
            .with_profile("profile".to_owned())
            .with_region("region".to_owned());

        let args = Run::new("aws")
            .with_aws_options(&context)
            .get_args()
            .to_owned();

        assert_eq!(
            args.as_ref(),
            vec!["--profile", "profile", "--region", "region"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn collect_output() {
        let result = Run::new("printf").arg("Hello, %s!").arg("world").output();

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello, world!");
    }

    #[cfg(unix)]
    #[test]
    fn pass_through_output() {
        let mut context = TestContext::default();

        let result = Run::new("printf")
            .arg("Hello, %s!")
            .arg("world")
            .pass_through(&mut context);

        assert!(result.is_ok());
        assert_eq!(context.output_as_string(), "Hello, world!");
    }
}
