use std::io::{self, Write as IoWrite, stdout};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use arrrg::CommandLine;
use cadence_agent::api_types::ScheduleItem;
use cadence_agent::{
    CadenceDocument, RhythmInput, RhythmKind, RhythmUpdate, StoredRhythm, TodayRenderOptions,
    add_rhythm, build_rhythm, convergence_response, default_file_path, defer_rhythm, delete_rhythm,
    delinquent_items, document_timezone, format_rhythm, list_rhythms,
    load_document as load_cadence_document, mark_rhythm_done, parse_date, parse_rhythm_id,
    parse_time, render_delinquent_items, render_schedule_items, render_today_items,
    save_document as save_cadence_document, schedule_items, set_spoons, today_items, update_rhythm,
};
use chrono::Utc;
use chrono_tz::Tz;
use crossterm::ExecutableCommand;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseButton, MouseEventKind,
};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::prelude::{CrosstermBackend, Terminal};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph};

type DynError = Box<dyn std::error::Error>;

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct AddOptions {
    #[arrrg(optional, "Rhythm type: daily, weekly, monthly, every-n-days")]
    r#type: Option<String>,
    #[arrrg(optional, "Time in HH:MM:SS format")]
    at: Option<String>,
    #[arrrg(optional, "Description of the rhythm")]
    desc: Option<String>,
    #[arrrg(optional, "Day of week (0=Monday, 6=Sunday) for weekly")]
    day: Option<String>,
    #[arrrg(optional, "Day of month (0-30) for monthly")]
    dotm: Option<String>,
    #[arrrg(optional, "Number of days for every-n-days")]
    num_days: Option<String>,
    #[arrrg(optional, "Slider before (e.g., 1)")]
    slider: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct ListOptions {
    #[arrrg(optional, "Filter by pattern in description")]
    pattern: Option<String>,
    #[arrrg(optional, "Filter by rhythm type")]
    r#type: Option<String>,
    #[arrrg(flag, "Output as JSON")]
    json: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct GetOptions {
    #[arrrg(optional, "Rhythm ID")]
    id: Option<String>,
    #[arrrg(flag, "Output as JSON")]
    json: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct UpdateOptions {
    #[arrrg(optional, "Rhythm ID")]
    id: Option<String>,
    #[arrrg(optional, "Rhythm type: daily, weekly, monthly, every-n-days")]
    r#type: Option<String>,
    #[arrrg(optional, "Time in HH:MM:SS format")]
    at: Option<String>,
    #[arrrg(optional, "Description")]
    desc: Option<String>,
    #[arrrg(optional, "Day of week (0=Monday, 6=Sunday) for weekly")]
    day: Option<String>,
    #[arrrg(optional, "Day of month (0-30) for monthly")]
    dotm: Option<String>,
    #[arrrg(optional, "Number of days for every-n-days")]
    num_days: Option<String>,
    #[arrrg(optional, "Slider before (e.g., 1)")]
    slider: Option<String>,
    #[arrrg(flag, "Use interactive mode")]
    interactive: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct DeleteOptions {
    #[arrrg(optional, "Rhythm ID")]
    id: Option<String>,
    #[arrrg(flag, "Force delete without confirmation")]
    force: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct EditOptions {
    #[arrrg(optional, "Rhythm ID (omit to use --all)")]
    id: Option<String>,
    #[arrrg(flag, "Edit the full YAML document")]
    all: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct TodayOptions {
    #[arrrg(flag, "Output as JSON")]
    json: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct GuiOptions {}

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct ScheduleOptions {
    #[arrrg(optional, "Start date (YYYY-MM-DD)")]
    start: Option<String>,
    #[arrrg(optional, "Number of days")]
    days: Option<String>,
    #[arrrg(flag, "Output as JSON")]
    json: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct ConvergenceOptions {
    #[arrrg(flag, "Output as JSON")]
    json: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct DelinquentOptions {
    #[arrrg(flag, "Output as JSON")]
    json: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct MarkDoneOptions {
    #[arrrg(optional, "Pattern to match rhythm descriptions")]
    pattern: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct DeferOptions {
    #[arrrg(optional, "Rhythm ID")]
    id: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct SpoonsOptions {
    #[arrrg(optional, "Date (YYYY-MM-DD)")]
    date: Option<String>,
    #[arrrg(optional, "Spoons value (0-10)")]
    value: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct ExportOptions {}

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct ImportOptions {
    #[arrrg(optional, "YAML file to import")]
    file: Option<String>,
    #[arrrg(flag, "Dry run (validate only)")]
    dry_run: bool,
}

#[derive(Clone, Debug)]
struct Config {
    file_path: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            file_path: default_file_path(),
        }
    }
}

fn prompt_input(prompt: &str) -> Result<String, DynError> {
    print!("{prompt}: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn prompt_number<T: FromStr>(prompt: &str) -> Result<T, DynError>
where
    T::Err: std::error::Error + 'static,
{
    let input = prompt_input(prompt)?;
    Ok(input.parse::<T>()?)
}

fn prompt_slider_before() -> Result<u32, DynError> {
    prompt_number::<u32>("Slider before (days)")
}

fn parse_slider_before(slider_str: &str) -> Result<u32, DynError> {
    Ok(slider_str.parse::<u32>()?)
}

fn take_positional(
    option: Option<String>,
    remaining: &[String],
    index: usize,
    label: &str,
) -> Result<String, DynError> {
    option
        .or_else(|| remaining.get(index).cloned())
        .ok_or_else(|| format!("{label} is required").into())
}

fn load_document(config: &Config) -> Result<CadenceDocument, DynError> {
    Ok(load_cadence_document(&config.file_path)?)
}

fn save_document(config: &Config, document: &CadenceDocument) -> Result<(), DynError> {
    save_cadence_document(&config.file_path, document)?;
    Ok(())
}

const HELP: &str = "cadence - Rhythm management CLI

USAGE:
    cadence [--file <PATH>] <COMMAND> [OPTIONS]

GLOBAL OPTIONS:
    --file <PATH>           YAML file path [default: ~/.config/rhythm/cadence.yaml]

COMMANDS:
    add                     Add a new rhythm
    list                    List all rhythms
    get <id>                Get details of a specific rhythm
    update <id>             Update a rhythm
    delete <id>             Delete a rhythm
    edit <id|--all>         Edit rhythm(s) in $EDITOR (YAML format)
    today                   Show today's tasks
    gui                     Interactive TUI for today's tasks
    schedule <start> <days> Show schedule for date range
    convergence             Show when all rhythms converge
    delinquent              Show delinquent rhythms
    mark-done <id> [...]    Mark rhythm(s) as done
    defer <id>              Defer a rhythm
    spoons <date> <value>   Set spoons for a date
    export                  Export the full YAML document
    import <file>           Import and replace from a YAML file

Use 'cadence <COMMAND> --help' for command-specific options.

EXAMPLES:
    cadence add --type daily --at 10:00:00 --desc \"Take medicine\"
    cadence add --type weekly --day 0 --at 09:00:00 --slider 1 --desc \"Team meeting\"
    cadence list --pattern \"meeting\" --json
    cadence mark-done rhythm:abc123 rhythm:def456
    cadence edit --all
    cadence export > my-rhythms.yaml
";

fn main() -> Result<(), DynError> {
    let mut args: Vec<String> = std::env::args().collect();
    let _program = args.remove(0);

    let mut config = Config::default();

    while !args.is_empty() && args[0].starts_with("--") {
        match args[0].as_str() {
            "--file" => {
                args.remove(0);
                if args.is_empty() {
                    eprintln!("--file requires a value");
                    std::process::exit(1);
                }
                config.file_path = PathBuf::from(args.remove(0));
            }
            "--help" | "-h" => {
                println!("{HELP}");
                return Ok(());
            }
            _ => break,
        }
    }

    if args.is_empty() {
        println!("{HELP}");
        return Ok(());
    }

    let command = args.remove(0);

    match command.as_str() {
        "add" => cmd_add(&config, &args),
        "list" => cmd_list(&config, &args),
        "get" => cmd_get(&config, &args),
        "update" => cmd_update(&config, &args),
        "delete" => cmd_delete(&config, &args),
        "edit" => cmd_edit(&config, &args),
        "today" => cmd_today(&config, &args),
        "gui" => cmd_gui(&config, &args),
        "schedule" => cmd_schedule(&config, &args),
        "convergence" => cmd_convergence(&config, &args),
        "delinquent" => cmd_delinquent(&config, &args),
        "mark-done" => cmd_mark_done(&config, &args),
        "defer" => cmd_defer(&config, &args),
        "spoons" => cmd_spoons(&config, &args),
        "export" => cmd_export(&config, &args),
        "import" => cmd_import(&config, &args),
        "help" | "-h" | "--help" => {
            println!("{HELP}");
            Ok(())
        }
        _ => {
            eprintln!("Unknown command: {command}");
            println!("{HELP}");
            std::process::exit(1);
        }
    }
}

fn cmd_add(config: &Config, args: &[String]) -> Result<(), DynError> {
    let args_str: Vec<&str> = args.iter().map(String::as_str).collect();
    let (opts, _remaining) = AddOptions::from_arguments_relaxed("cadence add", &args_str);

    let rhythm_type = opts
        .r#type
        .map(Ok)
        .unwrap_or_else(|| prompt_input("Rhythm type (daily/weekly/monthly/every-n-days)"))?;
    let time_str = opts
        .at
        .map(Ok)
        .unwrap_or_else(|| prompt_input("Time (HH:MM:SS)"))?;
    let time = parse_time(&time_str)?;
    let kind: RhythmKind = rhythm_type.parse()?;
    let mut input = RhythmInput {
        kind: Some(kind),
        at: Some(time),
        ..RhythmInput::default()
    };
    match kind {
        RhythmKind::Daily => {}
        RhythmKind::Weekly => {
            input.dotw = Some(
                opts.day
                    .as_deref()
                    .map(str::parse::<u8>)
                    .transpose()?
                    .map(Ok)
                    .unwrap_or_else(|| prompt_number::<u8>("Day of week (0=Monday, 6=Sunday)"))?,
            );
            input.slider_before = Some(
                opts.slider
                    .as_deref()
                    .map(parse_slider_before)
                    .transpose()?
                    .map(Ok)
                    .unwrap_or_else(prompt_slider_before)?,
            );
        }
        RhythmKind::Monthly => {
            input.dotm = Some(
                opts.dotm
                    .as_deref()
                    .map(str::parse::<u32>)
                    .transpose()?
                    .map(Ok)
                    .unwrap_or_else(|| prompt_number::<u32>("Day of month (0-30)"))?,
            );
            input.slider_before = Some(
                opts.slider
                    .as_deref()
                    .map(parse_slider_before)
                    .transpose()?
                    .map(Ok)
                    .unwrap_or_else(prompt_slider_before)?,
            );
        }
        RhythmKind::EveryNDays => {
            input.every_n_days = Some(
                opts.num_days
                    .as_deref()
                    .map(str::parse::<u32>)
                    .transpose()?
                    .map(Ok)
                    .unwrap_or_else(|| prompt_number::<u32>("Number of days"))?,
            );
            input.slider_before = Some(
                opts.slider
                    .as_deref()
                    .map(parse_slider_before)
                    .transpose()?
                    .map(Ok)
                    .unwrap_or_else(prompt_slider_before)?,
            );
        }
    }
    let rhythm = build_rhythm(input, None)?;

    let description = opts
        .desc
        .map(Ok)
        .unwrap_or_else(|| prompt_input("Description"))?;

    let mut document = load_document(config)?;
    add_rhythm(&mut document, rhythm, description, Utc::now())?;
    save_document(config, &document)?;

    println!("Rhythm created successfully!");
    Ok(())
}

fn cmd_list(config: &Config, args: &[String]) -> Result<(), DynError> {
    let args_str: Vec<&str> = args.iter().map(String::as_str).collect();
    let (opts, _remaining) = ListOptions::from_arguments_relaxed("cadence list", &args_str);
    let document = load_document(config)?;
    let rhythm_kind = opts.r#type.as_deref().map(str::parse).transpose()?;
    let rhythms = list_rhythms(&document, opts.pattern.as_deref(), rhythm_kind);

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&rhythms)?);
    } else if rhythms.is_empty() {
        println!("No rhythms found.");
    } else {
        for rhythm in rhythms {
            println!("{}: {} - {}", rhythm.id, rhythm.description, rhythm.rhythm);
        }
    }

    Ok(())
}

fn cmd_get(config: &Config, args: &[String]) -> Result<(), DynError> {
    let args_str: Vec<&str> = args.iter().map(String::as_str).collect();
    let (opts, remaining) = GetOptions::from_arguments_relaxed("cadence get", &args_str);
    let id = take_positional(opts.id, &remaining, 0, "rhythm id")?;
    let document = load_document(config)?;
    let rhythm_id = parse_rhythm_id(&id)?;
    let rhythm = document
        .find_rhythm(rhythm_id)
        .ok_or_else(|| format!("Rhythm not found: {id}"))?;

    if opts.json {
        println!("{}", serde_json::to_string_pretty(rhythm)?);
    } else {
        print!("{}", format_rhythm(rhythm));
    }

    Ok(())
}

fn cmd_update(config: &Config, args: &[String]) -> Result<(), DynError> {
    let args_str: Vec<&str> = args.iter().map(String::as_str).collect();
    let (opts, remaining) = UpdateOptions::from_arguments_relaxed("cadence update", &args_str);
    let id = take_positional(opts.id.clone(), &remaining, 0, "rhythm id")?;
    let mut document = load_document(config)?;
    let rhythm_id = parse_rhythm_id(&id)?;
    let existing = document
        .find_rhythm(rhythm_id)
        .cloned()
        .ok_or_else(|| format!("Rhythm not found: {id}"))?;

    let (rhythm, description) = if opts.interactive
        || (opts.r#type.is_none()
            && opts.at.is_none()
            && opts.desc.is_none()
            && opts.slider.is_none()
            && opts.day.is_none()
            && opts.dotm.is_none()
            && opts.num_days.is_none())
    {
        println!("Current rhythm:");
        println!("  Description: {}", existing.description);
        println!("  Rhythm: {}", existing.rhythm);
        println!();

        let rhythm_type: RhythmKind =
            prompt_input("Rhythm type (daily/weekly/monthly/every-n-days)")?.parse()?;
        let time_str = prompt_input("Time (HH:MM:SS)")?;
        let time = parse_time(&time_str)?;
        let mut input = RhythmInput {
            kind: Some(rhythm_type),
            at: Some(time),
            ..RhythmInput::default()
        };
        match rhythm_type {
            RhythmKind::Daily => {}
            RhythmKind::Weekly => {
                input.dotw = Some(prompt_number::<u8>("Day of week (0=Monday, 6=Sunday)")?);
                input.slider_before = Some(prompt_slider_before()?);
            }
            RhythmKind::Monthly => {
                input.dotm = Some(prompt_number::<u32>("Day of month (0-30)")?);
                input.slider_before = Some(prompt_slider_before()?);
            }
            RhythmKind::EveryNDays => {
                input.every_n_days = Some(prompt_number::<u32>("Number of days")?);
                input.slider_before = Some(prompt_slider_before()?);
            }
        }
        let rhythm = build_rhythm(input, None)?;

        let description = prompt_input("Description")?;
        (rhythm, description)
    } else {
        let description = opts.desc.unwrap_or_else(|| existing.description.clone());
        let rhythm = build_rhythm(
            RhythmInput {
                kind: opts.r#type.as_deref().map(str::parse).transpose()?,
                at: opts.at.as_deref().map(parse_time).transpose()?,
                dotw: opts.day.as_deref().map(str::parse::<u8>).transpose()?,
                dotm: opts.dotm.as_deref().map(str::parse::<u32>).transpose()?,
                every_n_days: opts
                    .num_days
                    .as_deref()
                    .map(str::parse::<u32>)
                    .transpose()?,
                slider_before: opts
                    .slider
                    .as_deref()
                    .map(parse_slider_before)
                    .transpose()?,
            },
            Some(existing.rhythm),
        )?;
        (rhythm, description)
    };

    update_rhythm(
        &mut document,
        rhythm_id,
        RhythmUpdate {
            rhythm: Some(rhythm),
            modified_at: Utc::now(),
        },
        Some(description),
    )?;
    save_document(config, &document)?;

    println!("Rhythm updated successfully!");
    Ok(())
}

fn cmd_delete(config: &Config, args: &[String]) -> Result<(), DynError> {
    let args_str: Vec<&str> = args.iter().map(String::as_str).collect();
    let (opts, remaining) = DeleteOptions::from_arguments_relaxed("cadence delete", &args_str);
    let id = take_positional(opts.id, &remaining, 0, "rhythm id")?;
    let rhythm_id = parse_rhythm_id(&id)?;

    if !opts.force {
        let confirmation = prompt_input(&format!("Delete rhythm {id}? (yes/no)"))?;
        if confirmation.to_lowercase() != "yes" {
            println!("Deletion cancelled.");
            return Ok(());
        }
    }

    let mut document = load_document(config)?;
    delete_rhythm(&mut document, rhythm_id)?;
    save_document(config, &document)?;

    println!("Rhythm deleted successfully!");
    Ok(())
}

fn cmd_edit(config: &Config, args: &[String]) -> Result<(), DynError> {
    let args_str: Vec<&str> = args.iter().map(String::as_str).collect();
    let (opts, remaining) = EditOptions::from_arguments_relaxed("cadence edit", &args_str);

    if opts.all {
        let document = load_document(config)?;
        let yaml = serde_yaml::to_string(&document)?;
        let edited = edit::edit(yaml)?;
        let updated_document: CadenceDocument = serde_yaml::from_str(&edited)?;
        save_document(config, &updated_document)?;
        println!("Cadence document updated successfully!");
        return Ok(());
    }

    if let Some(id) = opts.id.or_else(|| remaining.first().cloned()) {
        let mut document = load_document(config)?;
        let rhythm_id = parse_rhythm_id(&id)?;
        let index = document
            .rhythms
            .iter()
            .position(|rhythm| rhythm.id == rhythm_id)
            .ok_or_else(|| format!("Rhythm not found: {id}"))?;
        let current = document.rhythms[index].clone();
        let yaml = serde_yaml::to_string(&current)?;
        let edited = edit::edit(yaml)?;
        let mut updated_rhythm: StoredRhythm = serde_yaml::from_str(&edited)?;
        updated_rhythm.id = current.id;
        document.rhythms[index] = updated_rhythm;
        save_document(config, &document)?;
        println!("Rhythm updated successfully!");
        return Ok(());
    }

    Err("Either --all or <id> must be specified".into())
}

fn cmd_today(config: &Config, args: &[String]) -> Result<(), DynError> {
    let args_str: Vec<&str> = args.iter().map(String::as_str).collect();
    let (opts, _remaining) = TodayOptions::from_arguments_relaxed("cadence today", &args_str);
    let document = load_document(config)?;
    let user_tz = document_timezone(&document)?;
    let items = today_items(&document)?;

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&items)?);
    } else {
        println!(
            "{}",
            render_today_items(&items, &user_tz, TodayRenderOptions::default())
        );
    }

    Ok(())
}

struct ClickRegion {
    row: u16,
    col_start: u16,
    col_end: u16,
    action: GuiAction,
}

#[derive(Clone)]
enum GuiAction {
    MarkDone(String),
    Defer(String),
}

struct GuiApp {
    lines: Vec<String>,
    regions: Vec<ClickRegion>,
}

impl GuiApp {
    fn new(items: &[ScheduleItem], user_tz: &Tz) -> Self {
        let mut lines = Vec::new();
        let mut regions = Vec::new();

        let regular_items: Vec<_> = items.iter().filter(|item| !item.stretch_goal).collect();
        let stretch_items: Vec<_> = items.iter().filter(|item| item.stretch_goal).collect();

        let mut row = 0u16;

        if regular_items.is_empty() && stretch_items.is_empty() {
            lines.push("No scheduled items for today.".to_string());
        } else {
            for item in &regular_items {
                let local_time = item.datetime.with_timezone(user_tz);
                let (line, item_regions) =
                    Self::format_item(item, &local_time.format("%H:%M:%S").to_string(), row);
                lines.push(line);
                regions.extend(item_regions);
                row += 1;
            }

            if !stretch_items.is_empty() {
                if !regular_items.is_empty() {
                    lines.push(String::new());
                    row += 1;
                }
                lines.push("Stretch goals:".to_string());
                row += 1;
                for item in &stretch_items {
                    let local_time = item.datetime.with_timezone(user_tz);
                    let (line, item_regions) =
                        Self::format_item(item, &local_time.format("%H:%M:%S").to_string(), row);
                    lines.push(line);
                    regions.extend(item_regions);
                    row += 1;
                }
            }
        }

        Self { lines, regions }
    }

    fn format_item(
        item: &ScheduleItem,
        formatted_time: &str,
        row: u16,
    ) -> (String, Vec<ClickRegion>) {
        let mut regions = Vec::new();
        let done_text = "done";
        let defer_text = "defer";

        let done_start = 0u16;
        let done_end = done_text.len() as u16;
        regions.push(ClickRegion {
            row,
            col_start: done_start,
            col_end: done_end,
            action: GuiAction::MarkDone(item.rhythm_id.clone()),
        });

        let defer_start = done_end + 1;
        let defer_end = defer_start + defer_text.len() as u16;
        regions.push(ClickRegion {
            row,
            col_start: defer_start,
            col_end: defer_end,
            action: GuiAction::Defer(item.rhythm_id.clone()),
        });

        let line = format!(
            "{} {} {} - {} [{}]",
            done_text, defer_text, formatted_time, item.description, item.rhythm_id
        );

        (line, regions)
    }

    fn run(&self) -> io::Result<Option<GuiAction>> {
        enable_raw_mode()?;
        stdout()
            .execute(EnterAlternateScreen)?
            .execute(EnableMouseCapture)?;
        let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

        let result = self.run_loop(&mut terminal);

        disable_raw_mode()?;
        stdout()
            .execute(LeaveAlternateScreen)?
            .execute(DisableMouseCapture)?;

        result
    }

    fn run_loop(
        &self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> io::Result<Option<GuiAction>> {
        loop {
            terminal.draw(|f| self.ui(f))?;

            match event::read()? {
                Event::Key(key) => {
                    if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                        return Ok(None);
                    }
                }
                Event::Mouse(mouse) => {
                    if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                        && let Some(action) = self.find_action_at(mouse.column, mouse.row)
                    {
                        return Ok(Some(action));
                    }
                }
                _ => {}
            }
        }
    }

    fn ui(&self, frame: &mut ratatui::Frame) {
        let area = frame.area();
        let text = self.lines.join("\n");
        let paragraph = Paragraph::new(text)
            .style(Style::default().fg(Color::White))
            .block(
                Block::default()
                    .title("Today's Tasks (click done/defer, q or Esc to quit)")
                    .borders(Borders::ALL),
            );
        frame.render_widget(paragraph, area);
    }

    fn find_action_at(&self, col: u16, row: u16) -> Option<GuiAction> {
        let adjusted_row = row.saturating_sub(1);
        let adjusted_col = col.saturating_sub(1);

        self.regions
            .iter()
            .find(|region| {
                region.row == adjusted_row
                    && adjusted_col >= region.col_start
                    && adjusted_col < region.col_end
            })
            .map(|region| region.action.clone())
    }
}

fn cmd_gui(config: &Config, args: &[String]) -> Result<(), DynError> {
    let args_str: Vec<&str> = args.iter().map(String::as_str).collect();
    let (_opts, _remaining) = GuiOptions::from_arguments_relaxed("cadence gui", &args_str);

    loop {
        let document = load_document(config)?;
        let user_tz = document_timezone(&document)?;
        let items = today_items(&document)?;
        let app = GuiApp::new(&items, &user_tz);
        match app.run()? {
            Some(GuiAction::MarkDone(id)) => do_mark_done(config, &id)?,
            Some(GuiAction::Defer(id)) => do_defer(config, &id)?,
            None => break,
        }
    }

    Ok(())
}

fn do_mark_done(config: &Config, id: &str) -> Result<(), DynError> {
    let mut document = load_document(config)?;
    mark_rhythm_done(&mut document, parse_rhythm_id(id)?)?;
    save_document(config, &document)?;
    Ok(())
}

fn do_defer(config: &Config, id: &str) -> Result<(), DynError> {
    let mut document = load_document(config)?;
    defer_rhythm(&mut document, parse_rhythm_id(id)?)?;
    save_document(config, &document)?;
    Ok(())
}

fn cmd_schedule(config: &Config, args: &[String]) -> Result<(), DynError> {
    let args_str: Vec<&str> = args.iter().map(String::as_str).collect();
    let (opts, remaining) = ScheduleOptions::from_arguments_relaxed("cadence schedule", &args_str);

    let start = take_positional(opts.start, &remaining, 0, "start date")?;
    let days = take_positional(opts.days, &remaining, 1, "days")?;
    let start_date = parse_date(&start)?;
    let days: u32 = days.parse()?;

    let document = load_document(config)?;
    let user_tz = document_timezone(&document)?;
    let items = schedule_items(&document, start_date, days)?;

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&items)?);
    } else {
        println!(
            "{}",
            render_schedule_items(&items, &user_tz, "No scheduled items.")
        );
    }

    Ok(())
}

fn cmd_convergence(config: &Config, args: &[String]) -> Result<(), DynError> {
    let args_str: Vec<&str> = args.iter().map(String::as_str).collect();
    let (opts, _remaining) =
        ConvergenceOptions::from_arguments_relaxed("cadence convergence", &args_str);

    let document = load_document(config)?;
    let user_tz = document_timezone(&document)?;
    let result = convergence_response(&document)?;

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        match result.datetime {
            Some(dt) => {
                let local_time = dt.with_timezone(&user_tz);
                println!(
                    "Next convergence: {}",
                    local_time.format("%Y-%m-%d %H:%M:%S")
                );
            }
            None => println!("No convergence found."),
        }
    }

    Ok(())
}

fn cmd_delinquent(config: &Config, args: &[String]) -> Result<(), DynError> {
    let args_str: Vec<&str> = args.iter().map(String::as_str).collect();
    let (opts, _remaining) =
        DelinquentOptions::from_arguments_relaxed("cadence delinquent", &args_str);

    let document = load_document(config)?;
    let items = delinquent_items(&document)?;

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&items)?);
    } else {
        println!("{}", render_delinquent_items(&items));
    }

    Ok(())
}

fn cmd_mark_done(config: &Config, args: &[String]) -> Result<(), DynError> {
    let args_str: Vec<&str> = args.iter().map(String::as_str).collect();
    let (opts, remaining) = MarkDoneOptions::from_arguments_relaxed("cadence mark-done", &args_str);

    let mut document = load_document(config)?;
    let ids: Vec<String> = if let Some(pattern) = opts.pattern {
        let pattern = pattern.to_lowercase();
        document
            .rhythms
            .iter()
            .filter(|rhythm| rhythm.description.to_lowercase().contains(&pattern))
            .map(|rhythm| rhythm.id.to_string())
            .collect()
    } else {
        remaining
    };

    if ids.is_empty() {
        println!("No rhythms to mark as done.");
        return Ok(());
    }

    for id in &ids {
        mark_rhythm_done(&mut document, parse_rhythm_id(id)?)?;
        println!("Rhythm {id} marked as done.");
    }
    save_document(config, &document)?;

    Ok(())
}

fn cmd_defer(config: &Config, args: &[String]) -> Result<(), DynError> {
    let args_str: Vec<&str> = args.iter().map(String::as_str).collect();
    let (opts, remaining) = DeferOptions::from_arguments_relaxed("cadence defer", &args_str);
    let id = take_positional(opts.id, &remaining, 0, "rhythm id")?;
    do_defer(config, &id)?;
    println!("Rhythm deferred.");
    Ok(())
}

fn cmd_spoons(config: &Config, args: &[String]) -> Result<(), DynError> {
    let args_str: Vec<&str> = args.iter().map(String::as_str).collect();
    let (opts, remaining) = SpoonsOptions::from_arguments_relaxed("cadence spoons", &args_str);

    let date = take_positional(opts.date, &remaining, 0, "date")?;
    let value = take_positional(opts.value, &remaining, 1, "value")?;
    let date = parse_date(&date)?;
    let value: u8 = value.parse()?;

    let mut document = load_document(config)?;
    set_spoons(&mut document, date, value)?;
    save_document(config, &document)?;

    println!("Spoons set to {value} for {date}.");
    Ok(())
}

fn cmd_export(config: &Config, args: &[String]) -> Result<(), DynError> {
    let args_str: Vec<&str> = args.iter().map(String::as_str).collect();
    let (_opts, remaining) = ExportOptions::from_arguments_relaxed("cadence export", &args_str);
    if !remaining.is_empty() {
        return Err("export does not accept rhythm IDs in local mode".into());
    }

    let document = load_document(config)?;
    let yaml = serde_yaml::to_string(&document)?;
    println!("{yaml}");
    Ok(())
}

fn cmd_import(config: &Config, args: &[String]) -> Result<(), DynError> {
    let args_str: Vec<&str> = args.iter().map(String::as_str).collect();
    let (opts, remaining) = ImportOptions::from_arguments_relaxed("cadence import", &args_str);
    let file = take_positional(opts.file, &remaining, 0, "file")?;

    let file_path = Path::new(&file);
    let canonical_path = file_path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve file path '{}': {}", file, e))?;

    if !canonical_path.is_file() {
        return Err(format!("'{}' is not a file", file).into());
    }

    let metadata = std::fs::metadata(&canonical_path)?;
    const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;
    if metadata.len() > MAX_FILE_SIZE {
        return Err(format!(
            "File too large: {} bytes (max {} bytes / ~10MB)",
            metadata.len(),
            MAX_FILE_SIZE
        )
        .into());
    }

    let contents = std::fs::read_to_string(&canonical_path)?;
    let document: CadenceDocument = serde_yaml::from_str(&contents)?;
    document.to_manager()?;

    if opts.dry_run {
        println!(
            "Dry run: would import {} rhythms in timezone {}.",
            document.rhythms.len(),
            document.timezone
        );
        return Ok(());
    }

    save_document(config, &document)?;
    println!("Cadence document imported successfully!");
    Ok(())
}
