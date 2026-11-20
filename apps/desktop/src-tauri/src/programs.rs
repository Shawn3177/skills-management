//! Skills that their own program installs and updates. Agent Reach, for one,
//! ships its SKILL.md as a template that the `agent-reach` program fills in
//! and then places in every AI's skills folder itself. Copying that skill
//! would hand the tools a skill with holes in it and fight the program over
//! the same folders, so Skills Manage leaves the skill to its program and
//! only runs the update command the user set for it.
//!
//! `<data root>/program-updates.json`: `{ "<skill folder>": ["exe", "arg", …] }`.

use crate::fs_ops::default_data_root;
use crate::targets;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const PROGRAMS_FILE: &str = "program-updates.json";
/// Agent Reach gives its own post-upgrade sync 30 minutes.
const RUN_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// How long to keep reading output after the program exits: a process it
/// started in the background can hold the pipe open.
const OUTPUT_GRACE: Duration = Duration::from_secs(2);
/// How much of the program's output is passed back.
const OUTPUT_TAIL_CHARS: usize = 4000;

type Programs = BTreeMap<String, Vec<String>>;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProgramUpdate {
    /// The skill's folder name.
    pub skill_name: String,
    /// The command as the user would type it.
    pub command: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetProgramUpdateResult {
    pub program: ProgramUpdate,
    /// The shared-library copy went to the trash: the program places the skill now.
    pub removed_from_library: bool,
    /// Tools whose managed copy went to the trash with it.
    pub removed_from: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgramRunResult {
    pub skill_name: String,
    pub command: String,
    /// The end of what the program printed.
    pub output: String,
}

pub fn list_program_updates() -> Result<Vec<ProgramUpdate>, String> {
    Ok(list_with_root(&default_data_root()?))
}

/// Update `skill_name` by running `command` from now on. A shared-library copy
/// of it goes to the trash first (with the copies it placed in tools), so
/// Skills Manage no longer writes where the program does.
pub fn set_program_update(skill_name: String, command: String) -> Result<SetProgramUpdateResult, String> {
    let data_root = default_data_root()?;
    let roots = targets::toggleable_target_ids()
        .iter()
        .map(|id| targets::target_root_for(id).map(|root| (*id, root)))
        .collect::<Result<Vec<_>, _>>()?;
    set_with_roots(&data_root, &roots, &skill_name, &command)
}

pub fn remove_program_update(skill_name: String) -> Result<(), String> {
    let data_root = default_data_root()?;
    let mut programs = load(&data_root);
    if programs.remove(&skill_name).is_some() {
        save(&data_root, &programs)?;
    }
    Ok(())
}

pub fn run_program_update(skill_name: String) -> Result<ProgramRunResult, String> {
    run_with_root(&default_data_root()?, &skill_name, RUN_TIMEOUT)
}

fn list_with_root(data_root: &Path) -> Vec<ProgramUpdate> {
    load(data_root)
        .into_iter()
        .map(|(skill_name, argv)| ProgramUpdate { skill_name, command: join_command(&argv) })
        .collect()
}

fn set_with_roots(
    data_root: &Path,
    target_roots: &[(&str, PathBuf)],
    skill_name: &str,
    command: &str,
) -> Result<SetProgramUpdateResult, String> {
    if skill_name.is_empty() || skill_name == "." || skill_name == ".." || skill_name.contains(['/', '\\', ':']) {
        return Err(format!("'{skill_name}' is not a skill folder name."));
    }
    let argv = split_command(command)?;

    let library = data_root.join("library").join(skill_name);
    let (removed_from_library, removed_from) = if library.is_dir() {
        let removed = targets::remove_skill_from_library_with_roots(&library, data_root, target_roots)?;
        (true, removed.removed_from)
    } else {
        (false, Vec::new())
    };

    let mut programs = load(data_root);
    programs.insert(skill_name.to_string(), argv.clone());
    save(data_root, &programs)?;
    Ok(SetProgramUpdateResult {
        program: ProgramUpdate { skill_name: skill_name.to_string(), command: join_command(&argv) },
        removed_from_library,
        removed_from,
    })
}

fn run_with_root(data_root: &Path, skill_name: &str, timeout: Duration) -> Result<ProgramRunResult, String> {
    let argv = load(data_root)
        .remove(skill_name)
        .ok_or_else(|| format!("No update command is set for '{skill_name}'."))?;
    let command = join_command(&argv);
    let (success, code, output) = run_command(&argv, timeout).map_err(|error| format!("`{command}`: {error}"))?;
    if !success {
        let code = code.map(|code| format!(" (exit code {code})")).unwrap_or_default();
        return Err(format!("`{command}` failed{code}: {}", last_lines(&output, 6)));
    }
    Ok(ProgramRunResult { skill_name: skill_name.to_string(), command, output: tail(&output, OUTPUT_TAIL_CHARS) })
}

/// Run without a shell or a console window; returns (succeeded, exit code, output).
fn run_command(argv: &[String], timeout: Duration) -> Result<(bool, Option<i32>, String), String> {
    let mut command = Command::new(&argv[0]);
    command.args(&argv[1..]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            format!(
                "'{}' was not found. Install the program, or restart Skills Manage if you just installed it.",
                argv[0]
            )
        } else {
            format!("could not start it: {error}")
        }
    })?;

    let output = Arc::new(Mutex::new(Vec::new()));
    let pipes: Vec<Box<dyn Read + Send>> = [
        child.stdout.take().map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
        child.stderr.take().map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    .collect();
    let readers: Vec<_> = pipes
        .into_iter()
        .map(|mut pipe| {
            let output = Arc::clone(&output);
            thread::spawn(move || {
                let mut buffer = [0u8; 8192];
                while let Ok(read) = pipe.read(&mut buffer) {
                    if read == 0 {
                        break;
                    }
                    output.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).extend_from_slice(&buffer[..read]);
                }
            })
        })
        .collect();

    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| format!("could not wait for it: {error}"))? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("it did not finish within {} seconds and was stopped.", timeout.as_secs()));
        }
        thread::sleep(Duration::from_millis(100));
    };
    let grace = Instant::now() + OUTPUT_GRACE;
    while readers.iter().any(|reader| !reader.is_finished()) && Instant::now() < grace {
        thread::sleep(Duration::from_millis(20));
    }
    let text = String::from_utf8_lossy(&output.lock().unwrap_or_else(|poisoned| poisoned.into_inner())).to_string();
    Ok((status.success(), status.code(), text))
}

/// Split a command line into arguments; double quotes group words.
fn split_command(command: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quoted = false;
    for c in command.trim().chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if quoted {
        return Err("The command has an unclosed quote.".to_string());
    }
    if started {
        args.push(current);
    }
    if args.is_empty() {
        return Err("Enter the command that updates this skill, for example `agent-reach update`.".to_string());
    }
    Ok(args)
}

fn join_command(argv: &[String]) -> String {
    argv.iter()
        .map(|arg| if arg.is_empty() || arg.contains(char::is_whitespace) { format!("\"{arg}\"") } else { arg.clone() })
        .collect::<Vec<_>>()
        .join(" ")
}

fn tail(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    text.chars().skip(count.saturating_sub(max_chars)).collect::<String>().trim().to_string()
}

fn last_lines(text: &str, lines: usize) -> String {
    let kept: Vec<&str> = text.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    let joined = kept[kept.len().saturating_sub(lines)..].join("\n");
    if joined.is_empty() { "no output.".to_string() } else { tail(&joined, 800) }
}

fn load(data_root: &Path) -> Programs {
    fs::read_to_string(data_root.join(PROGRAMS_FILE))
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

fn save(data_root: &Path, programs: &Programs) -> Result<(), String> {
    fs::create_dir_all(data_root).map_err(|error| format!("Could not create the data root: {error}"))?;
    let json = serde_json::to_string_pretty(programs)
        .map_err(|error| format!("Could not serialize update commands: {error}"))?;
    // Write a sibling file first so a crash never leaves a half-written store.
    let partial = data_root.join(format!("{PROGRAMS_FILE}.partial"));
    fs::write(&partial, json).map_err(|error| format!("Could not save update commands: {error}"))?;
    fs::rename(&partial, data_root.join(PROGRAMS_FILE)).map_err(|error| format!("Could not save update commands: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs_ops::test_temp_dir as unique_temp_dir;

    #[test]
    fn splits_commands_with_quoted_paths() {
        assert_eq!(split_command("  agent-reach   update ").unwrap(), ["agent-reach", "update"]);
        assert_eq!(
            split_command(r#""C:\Program Files\Tool\tool.exe" update --all"#).unwrap(),
            [r"C:\Program Files\Tool\tool.exe", "update", "--all"]
        );
        assert!(split_command("   ").is_err());
        assert!(split_command("\"tool update").is_err());
        assert_eq!(
            join_command(&split_command(r#""C:\Program Files\t.exe" up"#).unwrap()),
            r#""C:\Program Files\t.exe" up"#
        );
    }

    #[test]
    fn setting_a_command_hands_the_skill_over_from_the_library() {
        let data_root = unique_temp_dir("program-set");
        let library = data_root.join("library").join("agent-reach");
        fs::create_dir_all(&library).unwrap();
        fs::write(library.join("SKILL.md"), "---\nname: agent-reach\ndescription: Read.\n---\n").unwrap();
        let claude_root = data_root.join("claude-root");
        targets::set_skill_target_enabled_with_root(&library, "claude-code", true, &data_root, &claude_root).unwrap();
        let other = claude_root.join("other");
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("SKILL.md"), "mine").unwrap();

        let roots = vec![("claude-code", claude_root.clone())];
        let result = set_with_roots(&data_root, &roots, "agent-reach", "agent-reach update").unwrap();

        assert!(result.removed_from_library);
        assert_eq!(result.removed_from, ["Claude Code"]);
        assert!(!library.exists());
        assert!(!claude_root.join("agent-reach").exists());
        assert_eq!(fs::read_to_string(other.join("SKILL.md")).unwrap(), "mine");
        assert!(fs::read_dir(data_root.join("trash")).unwrap().count() >= 2, "both copies went to the trash");
        assert_eq!(
            list_with_root(&data_root),
            [ProgramUpdate { skill_name: "agent-reach".to_string(), command: "agent-reach update".to_string() }]
        );

        // A skill only in tool folders is just recorded.
        let second = set_with_roots(&data_root, &roots, "codegraph", "codegraph upgrade").unwrap();
        assert!(!second.removed_from_library);
        assert_eq!(list_with_root(&data_root).len(), 2);
        assert!(set_with_roots(&data_root, &roots, "../x", "x update").is_err());
        let _ = fs::remove_dir_all(&data_root);
    }

    #[cfg(windows)]
    fn shell(script: &str) -> Vec<String> {
        vec!["cmd".to_string(), "/C".to_string(), script.to_string()]
    }

    #[cfg(not(windows))]
    fn shell(script: &str) -> Vec<String> {
        vec!["sh".to_string(), "-c".to_string(), script.to_string()]
    }

    fn with_program(prefix: &str, argv: Vec<String>) -> PathBuf {
        let data_root = unique_temp_dir(prefix);
        let mut programs = Programs::new();
        programs.insert("tool".to_string(), argv);
        save(&data_root, &programs).unwrap();
        data_root
    }

    #[test]
    fn runs_the_command_and_returns_its_output() {
        let data_root = with_program("program-run-ok", shell("echo updated to 1.6.1"));
        let result = run_with_root(&data_root, "tool", Duration::from_secs(30)).unwrap();
        assert!(result.output.contains("updated to 1.6.1"), "{}", result.output);
        let _ = fs::remove_dir_all(&data_root);
    }

    #[test]
    fn a_failing_command_reports_its_exit_code_and_last_lines() {
        let data_root = with_program("program-run-fail", shell("echo network down && exit 3"));
        let error = run_with_root(&data_root, "tool", Duration::from_secs(30)).unwrap_err();
        assert!(error.contains("exit code 3"), "{error}");
        assert!(error.contains("network down"), "{error}");
        let _ = fs::remove_dir_all(&data_root);
    }

    #[test]
    fn a_missing_program_or_command_is_explained() {
        let data_root = with_program("program-run-missing", vec!["no-such-program-skills-manage".to_string()]);
        let error = run_with_root(&data_root, "tool", Duration::from_secs(30)).unwrap_err();
        assert!(error.contains("was not found"), "{error}");
        assert!(run_with_root(&data_root, "other", Duration::from_secs(30)).unwrap_err().contains("No update command"));
        let _ = fs::remove_dir_all(&data_root);
    }

    #[test]
    fn a_command_that_runs_too_long_is_stopped() {
        #[cfg(windows)]
        let slow = shell("ping -n 30 127.0.0.1 > nul");
        #[cfg(not(windows))]
        let slow = shell("sleep 30");
        let data_root = with_program("program-run-timeout", slow);
        let started = Instant::now();
        let error = run_with_root(&data_root, "tool", Duration::from_millis(500)).unwrap_err();
        assert!(error.contains("did not finish"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(10));
        let _ = fs::remove_dir_all(&data_root);
    }
}
