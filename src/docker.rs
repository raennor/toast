use std::{
  io,
  io::Read,
  process::{ChildStdin, Command, Stdio},
};

// Query whether an image exists locally.
pub fn image_exists(image: &str) -> bool {
  debug!("Checking existence of image `{}`...", image);
  run_quiet(
    &["image", "inspect", image],
    &format!("The image `{}` does not exist.", image),
  )
  .is_ok()
}

// Push an image.
pub fn push_image(image: &str) -> Result<(), String> {
  debug!("Pushing image `{}`...", image);
  run_loud(
    &["image", "push", image],
    &format!("Unable to push image `{}`.", image),
  )
  .map(|_| ())
}

// Pull an image.
pub fn pull_image(image: &str) -> Result<(), String> {
  debug!("Pulling image `{}`...", image);
  run_loud(
    &["image", "pull", image],
    &format!("Unable to pull image `{}`.", image),
  )
  .map(|_| ())
}

// Delete an image.
pub fn delete_image(image: &str) -> Result<(), String> {
  debug!("Deleting image `{}`...", image);
  run_quiet(
    &["image", "rm", "--force", image],
    &format!("Unable to delete image `{}`.", image),
  )
  .map(|_| ())
}

// Create a container and return its ID.
pub fn create_container(image: &str, command: &str) -> Result<String, String> {
  debug!(
    "Creating container from image `{}` with command `{}`...",
    image, command
  );

  // Why `--init`? (1) PID 1 is supposed to reap orphaned zombie processes,
  // otherwise they can accumulate. Bash does this, but we run `/bin/sh` in the
  // container, which may or may not be Bash. So `--init` runs Tini
  // (https://github.com/krallin/tini) as PID 1, which properly reaps orphaned
  // zombies. (2) PID 1 also does not exhibit the default behavior (crashing)
  // for signals like SIGINT and SIGTERM. However, PID 1 can still handle these
  // signals by explicitly trapping them. Tini traps these signals and forwards
  // them to the child process. Then the default signal handling behavior of
  // the child process (in our case, `/bin/sh`) works normally. [tag:--init]
  Ok(
    run_quiet(
      vec![
        "container",
        "create",
        "--init",
        image,
        "/bin/sh",
        "-c",
        command,
      ]
      .as_ref(),
      &format!(
        "Unable to create container from image `{}` with command `{}`.",
        image, command
      ),
    )?
    .trim()
    .to_owned(),
  )
}

// Copy files into a container.
pub fn copy_into_container<R: Read>(
  container: &str,
  mut tar: R,
) -> Result<(), String> {
  debug!("Copying files into container `{}`...", container);
  run_quiet_stdin(
    &["container", "cp", "-", &format!("{}:{}", container, "/")],
    "Unable to copy files into the container.",
    |mut stdin| {
      io::copy(&mut tar, &mut stdin).map_err(|e| {
        format!("Unable to copy files into the container.. Details: {}", e)
      })?;

      Ok(())
    },
  )
  .map(|_| ())
}

// Start a container.
pub fn start_container(container: &str) -> Result<(), String> {
  debug!("Starting container `{}`...", container);
  run_loud(
    &["container", "start", "--attach", container],
    &format!("Unable to start container `{}`.", container),
  )
  .map(|_| ())
}

// Stop a container.
pub fn stop_container(container: &str) -> Result<(), String> {
  debug!("Stopping container `{}`...", container);
  run_quiet(
    &["container", "stop", container],
    &format!("Unable to stop container `{}`.", container),
  )
  .map(|_| ())
}

// Commit a container to an image.
pub fn commit_container(container: &str, image: &str) -> Result<(), String> {
  debug!(
    "Committing container `{}` to image `{}`...",
    container, image
  );
  run_quiet(
    &["container", "commit", container, image],
    &format!(
      "Unable to commit container `{}` to image `{}`.",
      container, image
    ),
  )
  .map(|_| ())
}

// Delete a container.
pub fn delete_container(container: &str) -> Result<(), String> {
  debug!("Deleting container `{}`...", container);
  run_quiet(
    &["container", "rm", "--force", container],
    &format!("Unable to delete container `{}`.", container),
  )
  .map(|_| ())
}

// Run an interactive shell.
pub fn spawn_shell(image: &str) -> Result<(), String> {
  debug!("Spawning an interactive shell for image `{}`...", image);
  run_attach(
    &[
      "container",
      "run",
      "--rm",
      "--interactive",
      "--tty",
      "--init", // [ref:--init]
      image,
      "/bin/su", // We use `su` rather than `sh` to use the root user's shell.
      "-l",
    ],
    "The shell exited with a failure.",
  )
}

// Run a command and return its standard output.
fn run_quiet(args: &[&str], error: &str) -> Result<String, String> {
  let output = command(args)
    .stdin(Stdio::null())
    .output()
    .map_err(|e| format!("{}\nDetails: {}", error, e))?;
  if !output.status.success() {
    return Err(format!(
      "{}\nDetails: {}",
      error,
      String::from_utf8_lossy(&output.stderr)
    ));
  }
  Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// Run a command and return its standard output. Accepts a closure which
// receives a pipe to the standard input stream of the child process.
fn run_quiet_stdin<W: FnOnce(&mut ChildStdin) -> Result<(), String>>(
  args: &[&str],
  error: &str,
  writer: W,
) -> Result<String, String> {
  let mut child = command(args)
    .stdin(Stdio::piped()) // [tag:stdin_piped]
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(|e| format!("{}\nDetails: {}", error, e))?;
  writer(child.stdin.as_mut().unwrap())?; // [ref:stdin_piped]
  let output = child
    .wait_with_output()
    .map_err(|e| format!("{}\nDetails: {}", error, e))?;
  if !output.status.success() {
    return Err(format!(
      "{}\nDetails: {}",
      error,
      String::from_utf8_lossy(&output.stderr)
    ));
  }
  Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// Run a command and forward its standard output and error streams.
fn run_loud(args: &[&str], error: &str) -> Result<(), String> {
  let status = command(args)
    .stdin(Stdio::null())
    .status()
    .map_err(|e| format!("{}\nDetails: {}", error, e))?;
  if !status.success() {
    return Err(error.to_owned());
  }
  Ok(())
}

// Run a command and forward its standard input, output, and error streams.
fn run_attach(args: &[&str], error: &str) -> Result<(), String> {
  let status = command(args)
    .status()
    .map_err(|e| format!("{}\nDetails: {}", error, e))?;
  if !status.success() {
    return Err(error.to_owned());
  }
  Ok(())
}

// Construct a Docker `Command` from an array of arguments.
fn command(args: &[&str]) -> Command {
  let mut command = Command::new("docker");
  for arg in args {
    command.arg(arg);
  }
  command
}