use anyhow::{Context, Result, bail};
use std::io::{IsTerminal, Read};

pub const HELP: &str = "Haruflow Community

  haruflow migrate                         Apply database migrations once
  haruflow serve                           Serve the ERP and frontend (default)
  haruflow healthcheck                     Probe application readiness
  haruflow version                         Print build/schema identity as JSON
  haruflow admin reset-password EMAIL      Recover an active administrator locally
      [--password-stdin]                   Read one password line from stdin instead of a hidden prompt

Recovery requires direct database access. Passwords are never accepted as command-line arguments.
Configuration: DATABASE_URL, HARUFLOW_SETUP_TOKEN, HARUFLOW_BIND,
HARUFLOW_STATIC_DIR, HARUFLOW_SECURE_COOKIES. See README.md.";

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    Healthcheck,
    Version,
    Migrate,
    Serve,
    ResetAdmin { email: String, password_stdin: bool },
}

pub fn parse(args: &[String]) -> Result<Command> {
    match args {
        [] => Ok(Command::Serve),
        [command] => match command.as_str() {
            "--help" | "help" => Ok(Command::Help),
            "serve" => Ok(Command::Serve),
            "migrate" => Ok(Command::Migrate),
            "healthcheck" => Ok(Command::Healthcheck),
            "version" | "--version" => Ok(Command::Version),
            _ => bail!("Unknown command. Use haruflow --help."),
        },
        [admin, reset, email] if admin == "admin" && reset == "reset-password" => {
            Ok(Command::ResetAdmin {
                email: email.clone(),
                password_stdin: false,
            })
        }
        [admin, reset, email, flag]
            if admin == "admin" && reset == "reset-password" && flag == "--password-stdin" =>
        {
            Ok(Command::ResetAdmin {
                email: email.clone(),
                password_stdin: true,
            })
        }
        _ => bail!(
            "Invalid arguments. Use haruflow --help; passwords must not appear in command arguments."
        ),
    }
}

fn password_line(input: &str) -> Result<String> {
    let password = input
        .strip_suffix("\r\n")
        .or_else(|| input.strip_suffix('\n'))
        .unwrap_or(input);
    if password.contains(['\r', '\n']) || !(12..=128).contains(&password.len()) {
        bail!("Provide one password containing 12–128 bytes, optionally followed by one newline.");
    }
    Ok(password.to_owned())
}

pub fn read_recovery_password(from_stdin: bool) -> Result<String> {
    if from_stdin {
        let mut input = String::new();
        std::io::stdin()
            .lock()
            .take(132)
            .read_to_string(&mut input)
            .context("Cannot read password from stdin")?;
        return password_line(&input);
    }
    if !std::io::stdin().is_terminal() {
        bail!(
            "Interactive recovery requires a terminal. Use --password-stdin with a protected input stream for automation."
        );
    }
    let password = rpassword::prompt_password("Password baru: ")
        .context("Cannot read password from terminal")?;
    let confirmation = rpassword::prompt_password("Ulangi password baru: ")
        .context("Cannot read password confirmation")?;
    if password != confirmation {
        bail!("Password confirmation does not match. No changes were made.");
    }
    password_line(&password)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn recovery_cli_never_accepts_password_arguments() {
        assert_eq!(parse(&[]).unwrap(), Command::Serve);
        assert_eq!(parse(&args(&["--help"])).unwrap(), Command::Help);
        assert_eq!(
            parse(&args(&[
                "admin",
                "reset-password",
                "admin@example.test",
                "--password-stdin"
            ]))
            .unwrap(),
            Command::ResetAdmin {
                email: "admin@example.test".into(),
                password_stdin: true
            }
        );
        for invalid in [
            args(&["admin", "reset-password"]),
            args(&[
                "admin",
                "reset-password",
                "admin@example.test",
                "plaintext-secret",
            ]),
            args(&["serve", "ignored"]),
        ] {
            assert!(parse(&invalid).is_err());
        }
    }

    #[test]
    fn stdin_password_has_exact_bounds_without_trimming_spaces() {
        assert_eq!(
            password_line("  protected password  \r\n").unwrap(),
            "  protected password  "
        );
        assert!(password_line("short").is_err());
        assert!(password_line(&"x".repeat(129)).is_err());
        assert!(password_line("protected-password\nsecond-line").is_err());
        assert!(password_line("protected-password\n\n").is_err());
        assert!(password_line(&"x".repeat(128)).is_ok());
    }
}
