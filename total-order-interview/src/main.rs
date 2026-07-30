use std::env;
use std::io::{self, Write};
use std::path::Path;

use rustyline::DefaultEditor;
use total_order_interview::{
    Answer, Error, EventKind, Result, SessionLog, append_report_events, build_report_data,
    derive_state, format_pair_table, load_session, next_pair, parse_report_args,
    render_markdown_report, save_session_atomic, seed_from_clock, write_text,
};

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let command = args
        .first()
        .cloned()
        .ok_or_else(|| Error::Invalid(usage()))?;
    args.remove(0);
    match command.as_str() {
        "new" => cmd_new(&args),
        "interview" => cmd_interview(&args),
        "report" => cmd_report(&args),
        "inspect" => cmd_inspect(&args),
        "help" | "--help" | "-h" => {
            println!("{}", usage());
            Ok(())
        }
        other => Err(Error::Invalid(format!(
            "unknown command: {other}\n\n{}",
            usage()
        ))),
    }
}

fn cmd_new(args: &[String]) -> Result<()> {
    let (path, seed) = parse_new_args(args)?;
    if path.exists() {
        return Err(Error::Invalid(format!("{} already exists", path.display())));
    }
    let title = prompt_line("Title: ")?;
    let notes = prompt_line("Notes (optional): ")?;
    println!("Enter binary feature names, one per line. Submit a blank line when done.");
    let mut features = Vec::new();
    loop {
        let line = prompt_line(&format!("Feature {}: ", features.len() + 1))?;
        let line = line.trim().to_string();
        if line.is_empty() {
            break;
        }
        features.push(line);
        if features.len() == 8 {
            println!("warning: more than 8 binary features can make interviews long");
        }
    }
    let log = SessionLog::new(title, notes, features, seed.unwrap_or_else(seed_from_clock))?;
    save_session_atomic(path, &log)?;
    println!("created {}", path.display());
    Ok(())
}

fn cmd_interview(args: &[String]) -> Result<()> {
    let path = single_path_arg(args, "interview")?;
    let mut editor = DefaultEditor::new().map_err(|err| Error::Invalid(err.to_string()))?;
    loop {
        let mut log = load_session(path)?;
        let state = derive_state(&log)?;
        let Some((left, right)) = next_pair(&state)? else {
            println!(
                "No next comparison is needed. The current observed satisfying set is complete."
            );
            return Ok(());
        };
        println!();
        println!("{}", format_pair_table(&left, &right, &state.features)?);
        println!(
            "Commands: a, b, equal, reject a, reject b, reject both, skip, undo, retract EVENT_ID, quit, help"
        );
        let line = editor
            .readline("> ")
            .map_err(|err| Error::Invalid(err.to_string()))?;
        let command = line.trim().to_lowercase();
        if command.is_empty() {
            continue;
        }
        let _ = editor.add_history_entry(command.as_str());
        match command.as_str() {
            "a" => append_answer(path, &mut log, &left, &right, Answer::PreferLeft)?,
            "b" => append_answer(path, &mut log, &left, &right, Answer::PreferRight)?,
            "equal" | "=" | "no preference" => {
                append_answer(path, &mut log, &left, &right, Answer::Equal)?
            }
            "reject a" => append_answer(path, &mut log, &left, &right, Answer::RejectLeft)?,
            "reject b" => append_answer(path, &mut log, &left, &right, Answer::RejectRight)?,
            "reject both" => append_answer(path, &mut log, &left, &right, Answer::RejectBoth)?,
            "skip" => append_answer(path, &mut log, &left, &right, Answer::Skip)?,
            "undo" => {
                let event_id = log
                    .latest_active_answer_id()
                    .ok_or_else(|| Error::Invalid("no answer to undo".to_string()))?;
                log.push(EventKind::AnswerRetracted { event_id });
                save_session_atomic(path, &log)?;
                println!("undone");
            }
            "quit" | "q" => return Ok(()),
            "help" => print_interview_help(),
            other if other.starts_with("retract ") => {
                let event_id = other.trim_start_matches("retract ").trim();
                if event_id.is_empty() {
                    return Err(Error::Invalid("retract requires an event id".to_string()));
                }
                log.push(EventKind::AnswerRetracted {
                    event_id: event_id.to_string(),
                });
                save_session_atomic(path, &log)?;
                println!("retracted {event_id}");
            }
            other => println!("unknown command: {other}"),
        }
    }
}

fn append_answer(
    path: &Path,
    log: &mut SessionLog,
    left: &str,
    right: &str,
    answer: Answer,
) -> Result<()> {
    let mut candidate = log.clone();
    candidate.push(EventKind::ComparisonAnswered {
        left: left.to_string(),
        right: right.to_string(),
        answer,
    });
    match derive_state(&candidate) {
        Ok(_) => {
            *log = candidate;
            save_session_atomic(path, log)?;
            Ok(())
        }
        Err(Error::Inconsistent(err)) => {
            println!("That answer creates an inconsistent preorder: {err}");
            println!(
                "No event was written. Use `retract EVENT_ID` if an earlier answer should change."
            );
            Ok(())
        }
        Err(err) => Err(err),
    }
}

fn cmd_report(args: &[String]) -> Result<()> {
    if args.is_empty() {
        return Err(Error::Invalid("report requires a session path".to_string()));
    }
    let path = Path::new(&args[0]);
    let mut log = load_session(path)?;
    let (md, json) = parse_report_args(&args[1..])?;
    append_report_events(&mut log, md.as_deref(), json.as_deref());
    let report = build_report_data(&log)?;
    if let Some(path) = &md {
        write_text(path, &render_markdown_report(&report))?;
        println!("wrote {}", path.display());
    }
    if let Some(path) = &json {
        write_text(path, &serde_json::to_string_pretty(&report)?)?;
        println!("wrote {}", path.display());
    }
    save_session_atomic(path, &log)?;
    Ok(())
}

fn cmd_inspect(args: &[String]) -> Result<()> {
    let path = single_path_arg(args, "inspect")?;
    let log = load_session(path)?;
    let state = derive_state(&log)?;
    println!("Title: {}", state.title);
    println!("Features: {}", state.features.len());
    println!("Candidates: {}", state.candidates.len());
    println!("Observed: {}", state.observed.len());
    println!("Rejected: {}", state.rejected.len());
    println!("Active comparisons: {}", state.active_comparisons.len());
    match next_pair(&state)? {
        Some((left, right)) => println!("Next pair: {left} vs {right}"),
        None => println!("Next pair: none"),
    }
    Ok(())
}

fn parse_new_args(args: &[String]) -> Result<(&Path, Option<u64>)> {
    if args.is_empty() {
        return Err(Error::Invalid("new requires a session path".to_string()));
    }
    let path = Path::new(&args[0]);
    let mut seed = None;
    let mut idx = 1;
    while idx < args.len() {
        match args[idx].as_str() {
            "--seed" => {
                idx += 1;
                let value = args
                    .get(idx)
                    .ok_or_else(|| Error::Invalid("--seed requires a value".to_string()))?;
                seed = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| Error::Invalid(format!("invalid seed: {value}")))?,
                );
            }
            other => return Err(Error::Invalid(format!("unknown new argument: {other}"))),
        }
        idx += 1;
    }
    Ok((path, seed))
}

fn single_path_arg<'a>(args: &'a [String], command: &str) -> Result<&'a Path> {
    if args.len() != 1 {
        return Err(Error::Invalid(format!(
            "{command} requires exactly one session path"
        )));
    }
    Ok(Path::new(&args[0]))
}

fn prompt_line(prompt: &str) -> Result<String> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(line.trim_end().to_string())
}

fn print_interview_help() {
    println!("a             prefer candidate A");
    println!("b             prefer candidate B");
    println!("equal         no preference between A and B");
    println!("reject a      mark candidate A unsatisfying");
    println!("reject b      mark candidate B unsatisfying");
    println!("reject both   mark both candidates unsatisfying");
    println!("skip          record no evidence for this prompt");
    println!("undo          retract the latest active answer");
    println!("retract ID    retract a specific answer event");
    println!("quit          leave the interview");
}

fn usage() -> String {
    "usage:
  total-order-interview new session.json [--seed SEED]
  total-order-interview interview session.json
  total-order-interview report session.json --md report.md --json report.json
  total-order-interview inspect session.json"
        .to_string()
}
