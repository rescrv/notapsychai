use std::io::{self, Write as IoWrite};
use std::str::FromStr;

use arrrg::CommandLine;
use chrono::{NaiveDate, NaiveTime, Utc};
use chrono_tz::Tz;
use notapsychai::api_types::{
    ConvergenceResponse, CreateRhythmRequest, DeferRequest, DelinquentItem, LoginRequest,
    LoginResponse, MarkDoneRequest, RegisterRequest, RegisterResponse, RhythmResponse,
    ScheduleItem, ScheduleQuery, SetSpoonsRequest, UserResponse,
};
use notapsychai::{Rhythm, Slider};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct RegisterOptions {
    #[arrrg(optional, "Username")]
    username: Option<String>,
    #[arrrg(optional, "Email address")]
    email: Option<String>,
    #[arrrg(optional, "Password")]
    password: Option<String>,
    #[arrrg(optional, "Timezone (e.g., America/New_York, UTC)")]
    timezone: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct LoginOptions {
    #[arrrg(optional, "Username")]
    username: Option<String>,
    #[arrrg(optional, "Password")]
    password: Option<String>,
}

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
    #[arrrg(optional, "Slider before,after (e.g., 1,2)")]
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
    #[arrrg(required, "Rhythm ID")]
    id: String,
    #[arrrg(flag, "Output as JSON")]
    json: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct UpdateOptions {
    #[arrrg(required, "Rhythm ID")]
    id: String,
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
    #[arrrg(optional, "Slider before,after (e.g., 1,2)")]
    slider: Option<String>,
    #[arrrg(flag, "Use interactive mode")]
    interactive: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct DeleteOptions {
    #[arrrg(required, "Rhythm ID")]
    id: String,
    #[arrrg(flag, "Force delete without confirmation")]
    force: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct EditOptions {
    #[arrrg(optional, "Rhythm ID (omit to use --all)")]
    id: Option<String>,
    #[arrrg(flag, "Edit all rhythms")]
    all: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct TodayOptions {
    #[arrrg(flag, "Output as JSON")]
    json: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct ScheduleOptions {
    #[arrrg(required, "Start date (YYYY-MM-DD)")]
    start: String,
    #[arrrg(required, "Number of days")]
    days: String,
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
    #[arrrg(required, "Rhythm ID")]
    id: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct SpoonsOptions {
    #[arrrg(required, "Date (YYYY-MM-DD)")]
    date: String,
    #[arrrg(required, "Spoons value (0-10)")]
    value: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct ExportOptions {}

#[derive(Clone, Debug, Default, Eq, PartialEq, arrrg_derive::CommandLine)]
struct ImportOptions {
    #[arrrg(required, "YAML file to import")]
    file: String,
    #[arrrg(flag, "Dry run (validate only)")]
    dry_run: bool,
}

fn prompt_input(prompt: &str) -> Result<String, Box<dyn std::error::Error>> {
    print!("{prompt}: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn prompt_password(prompt: &str) -> Result<String, Box<dyn std::error::Error>> {
    print!("{prompt}: ");
    io::stdout().flush()?;
    Ok(rpassword::read_password()?)
}

fn prompt_number<T: FromStr>(prompt: &str) -> Result<T, Box<dyn std::error::Error>>
where
    T::Err: std::error::Error + 'static,
{
    let input = prompt_input(prompt)?;
    Ok(input.parse::<T>()?)
}

fn prompt_slider() -> Result<Slider, Box<dyn std::error::Error>> {
    let before = prompt_number::<u32>("Slider before (days)")?;
    let after = prompt_number::<u32>("Slider after (days)")?;
    Ok(Slider::new(before, after))
}

fn parse_slider(slider_str: &str) -> Result<Slider, Box<dyn std::error::Error>> {
    let parts: Vec<&str> = slider_str.split(',').collect();
    if parts.len() != 2 {
        return Err("Slider must be in format 'before,after' (e.g. '1,2')".into());
    }
    let before = parts[0].parse::<u32>()?;
    let after = parts[1].parse::<u32>()?;
    Ok(Slider::new(before, after))
}

fn parse_time(time_str: &str) -> Result<NaiveTime, Box<dyn std::error::Error>> {
    NaiveTime::parse_from_str(time_str, "%H:%M:%S").map_err(|e| {
        format!(
            "Failed to parse time '{}': {}. Expected format: HH:MM:SS (e.g., 09:30:00)",
            time_str, e
        )
        .into()
    })
}

async fn check_response_success(
    response: reqwest::Response,
    operation: &str,
) -> Result<reqwest::Response, Box<dyn std::error::Error>> {
    if !response.status().is_success() {
        let error_text = response.text().await?;
        return Err(format!("{operation} failed: {error_text}").into());
    }
    Ok(response)
}

#[derive(Serialize, Deserialize, Default)]
struct Config {
    server_url: String,
    auth_token: Option<String>,
}

impl Config {
    fn load() -> Self {
        let config_path = Self::config_path();
        if let Ok(contents) = std::fs::read_to_string(&config_path) {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = std::fs::metadata(&config_path) {
                    let mode = metadata.permissions().mode();
                    if mode & 0o077 != 0 {
                        eprintln!(
                            "Warning: Config file {} has overly permissive permissions ({:o}). Consider running: chmod 600 {}",
                            config_path.display(),
                            mode & 0o777,
                            config_path.display()
                        );
                    }
                }
            }
            serde_json::from_str(&contents).unwrap_or_default()
        } else {
            Self {
                server_url: "http://localhost:3000".to_string(),
                auth_token: None,
            }
        }
    }

    fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_dir = Self::config_dir();
        std::fs::create_dir_all(&config_dir)?;
        let config_path = Self::config_path();
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&config_path, json)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&config_path)?.permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&config_path, perms)?;
        }

        Ok(())
    }

    fn config_dir() -> std::path::PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("rhythm")
    }

    fn config_path() -> std::path::PathBuf {
        Self::config_dir().join("config.json")
    }

    fn require_auth(&self) -> Result<&str, Box<dyn std::error::Error>> {
        self.auth_token
            .as_deref()
            .ok_or_else(|| "Not logged in. Run 'cadence login' first.".into())
    }
}

const HELP: &str = "cadence - Rhythm management CLI

USAGE:
    cadence [--server <URL>] <COMMAND> [OPTIONS]

GLOBAL OPTIONS:
    --server <URL>          Server URL [default: http://localhost:3000]

COMMANDS:
    register                Register a new user account
    login                   Login and save authentication token
    add                     Add a new rhythm
    list                    List all rhythms
    get <id>                Get details of a specific rhythm
    update <id>             Update a rhythm
    delete <id>             Delete a rhythm
    edit <id|--all>         Edit rhythm(s) in $EDITOR (YAML format)
    today                   Show today's tasks
    schedule <start> <days> Show schedule for date range
    convergence             Show when all rhythms converge
    delinquent              Show delinquent rhythms
    mark-done <id> [...]    Mark rhythm(s) as done
    defer <id>              Defer a rhythm
    spoons <date> <value>   Set spoons for a date
    export [<id>...]        Export rhythms to YAML
    import <file>           Import rhythms from YAML file

Use 'cadence <COMMAND> --help' for command-specific options.

EXAMPLES:
    cadence add --type daily --at 10:00:00 --desc \"Take medicine\"
    cadence add --type weekly --day 0 --at 09:00:00 --slider 1,1 --desc \"Team meeting\"
    cadence list --pattern \"meeting\" --json
    cadence mark-done rhythm:abc123 rhythm:def456
    cadence edit rhythm:abc123
    cadence export > my-rhythms.yaml
";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().collect();
    let _program = args.remove(0);

    let mut config = Config::load();

    while !args.is_empty() && args[0].starts_with("--") {
        match args[0].as_str() {
            "--server" => {
                args.remove(0);
                if args.is_empty() {
                    eprintln!("--server requires a value");
                    std::process::exit(1);
                }
                config.server_url = args.remove(0);
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
        "register" => cmd_register(&mut config, &args).await,
        "login" => cmd_login(&mut config, &args).await,
        "add" => cmd_add(&config, &args).await,
        "list" => cmd_list(&config, &args).await,
        "get" => cmd_get(&config, &args).await,
        "update" => cmd_update(&config, &args).await,
        "delete" => cmd_delete(&config, &args).await,
        "edit" => cmd_edit(&config, &args).await,
        "today" => cmd_today(&config, &args).await,
        "schedule" => cmd_schedule(&config, &args).await,
        "convergence" => cmd_convergence(&config, &args).await,
        "delinquent" => cmd_delinquent(&config, &args).await,
        "mark-done" => cmd_mark_done(&config, &args).await,
        "defer" => cmd_defer(&config, &args).await,
        "spoons" => cmd_spoons(&config, &args).await,
        "export" => cmd_export(&config, &args).await,
        "import" => cmd_import(&config, &args).await,
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

async fn cmd_register(
    config: &mut Config,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let (opts, _remaining) = RegisterOptions::from_arguments_relaxed("cadence register", &args_str);

    let username = opts
        .username
        .map(Ok)
        .unwrap_or_else(|| prompt_input("Username"))?;
    let email = opts
        .email
        .map(Ok)
        .unwrap_or_else(|| prompt_input("Email"))?;
    let password = opts
        .password
        .map(Ok)
        .unwrap_or_else(|| prompt_password("Password"))?;
    let timezone = opts.timezone.unwrap_or_else(|| {
        prompt_input("Timezone [UTC]")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "UTC".to_string())
    });

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/auth/register", config.server_url))
        .json(&RegisterRequest {
            username,
            email,
            password,
            timezone,
        })
        .send()
        .await?;

    let response = check_response_success(response, "Registration").await?;
    let _result: RegisterResponse = response.json().await?;
    println!("Registration successful! Please login with 'cadence login'");

    Ok(())
}

async fn cmd_login(config: &mut Config, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let (opts, _remaining) = LoginOptions::from_arguments_relaxed("cadence login", &args_str);

    let username = opts
        .username
        .map(Ok)
        .unwrap_or_else(|| prompt_input("Username"))?;
    let password = opts
        .password
        .map(Ok)
        .unwrap_or_else(|| prompt_password("Password"))?;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/auth/login", config.server_url))
        .json(&LoginRequest { username, password })
        .send()
        .await?;

    let response = check_response_success(response, "Login").await?;
    let result: LoginResponse = response.json().await?;
    config.auth_token = Some(result.token);
    config.save()?;

    println!("Login successful!");

    Ok(())
}

async fn cmd_add(config: &Config, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let (opts, _remaining) = AddOptions::from_arguments_relaxed("cadence add", &args_str);
    let token = config.require_auth()?;

    let rhythm_type = opts
        .r#type
        .map(Ok)
        .unwrap_or_else(|| prompt_input("Rhythm type (daily/weekly/monthly/every-n-days)"))?;
    let time_str = opts
        .at
        .map(Ok)
        .unwrap_or_else(|| prompt_input("Time (HH:MM:SS)"))?;
    let time = parse_time(&time_str)?;

    let rhythm = match rhythm_type.to_lowercase().as_str() {
        "daily" => Rhythm::Daily { at: time },
        "weekly" => {
            let dotw = opts
                .day
                .map(|s| s.parse::<u8>())
                .transpose()?
                .map(Ok)
                .unwrap_or_else(|| prompt_number::<u8>("Day of week (0=Monday, 6=Sunday)"))?;
            if dotw > 6 {
                return Err("Day of week must be in range 0-6 (0=Monday, 6=Sunday)".into());
            }
            let slider = if let Some(ref slider_str) = opts.slider {
                parse_slider(slider_str)?
            } else {
                prompt_slider()?
            };

            Rhythm::WeekDaily {
                dotw,
                at: time,
                slider,
            }
        }
        "monthly" => {
            let dotm = opts
                .dotm
                .map(|s| s.parse::<u32>())
                .transpose()?
                .map(Ok)
                .unwrap_or_else(|| prompt_number::<u32>("Day of month (0-30)"))?;
            if dotm > 30 {
                return Err("Day of month must be in range 0-30".into());
            }
            let slider = if let Some(ref slider_str) = opts.slider {
                parse_slider(slider_str)?
            } else {
                prompt_slider()?
            };

            Rhythm::Monthly {
                dotm,
                at: time,
                slider,
            }
        }
        "every-n-days" => {
            let n = opts
                .num_days
                .map(|s| s.parse::<u32>())
                .transpose()?
                .map(Ok)
                .unwrap_or_else(|| prompt_number::<u32>("Number of days"))?;
            if n == 0 {
                return Err("Number of days must be greater than 0".into());
            }
            let slider = if let Some(ref slider_str) = opts.slider {
                parse_slider(slider_str)?
            } else {
                prompt_slider()?
            };

            Rhythm::EveryNDays {
                n,
                at: time,
                slider,
            }
        }
        _ => {
            return Err("Invalid rhythm type".into());
        }
    };

    let description = opts
        .desc
        .map(Ok)
        .unwrap_or_else(|| prompt_input("Description"))?;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/rhythms", config.server_url))
        .bearer_auth(token)
        .json(&CreateRhythmRequest {
            rhythm,
            description,
        })
        .send()
        .await?;

    check_response_success(response, "Create rhythm").await?;

    println!("Rhythm created successfully!");

    Ok(())
}

async fn cmd_list(config: &Config, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let (opts, _remaining) = ListOptions::from_arguments_relaxed("cadence list", &args_str);
    let token = config.require_auth()?;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/rhythms", config.server_url))
        .bearer_auth(token)
        .send()
        .await?;

    let response = check_response_success(response, "List rhythms").await?;
    let mut rhythms: Vec<RhythmResponse> = response.json().await?;

    if let Some(pattern) = opts.pattern {
        let pattern = pattern.to_lowercase();
        rhythms.retain(|r| r.description.to_lowercase().contains(&pattern));
    }

    if let Some(type_filter) = opts.r#type {
        let type_filter = type_filter.to_lowercase();
        let normalized_filter = match type_filter.as_str() {
            "weekly" => "weekdaily",
            "every-n-days" => "every_n_days",
            other => other,
        };
        rhythms.retain(|r| r.rhythm.type_name() == normalized_filter);
    }

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

async fn cmd_get(config: &Config, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let (opts, _remaining) = GetOptions::from_arguments_relaxed("cadence get", &args_str);
    let token = config.require_auth()?;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/rhythms/{}", config.server_url, opts.id))
        .bearer_auth(token)
        .send()
        .await?;

    let response = check_response_success(response, "Get rhythm").await?;
    let rhythm: RhythmResponse = response.json().await?;

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&rhythm)?);
    } else {
        println!("ID: {}", rhythm.id);
        println!("Description: {}", rhythm.description);
        println!("Rhythm: {}", rhythm.rhythm);
        println!("Created: {} ({})", rhythm.created_at, rhythm.created_at_tz);
        println!(
            "Modified: {} ({})",
            rhythm.modified_at, rhythm.modified_at_tz
        );
    }

    Ok(())
}

async fn cmd_update(config: &Config, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let (opts, _remaining) = UpdateOptions::from_arguments_relaxed("cadence update", &args_str);
    let token = config.require_auth()?;

    let client = reqwest::Client::new();
    let get_response = client
        .get(format!("{}/rhythms/{}", config.server_url, opts.id))
        .bearer_auth(token)
        .send()
        .await?;

    let get_response = check_response_success(get_response, "Get rhythm").await?;
    let current: RhythmResponse = get_response.json().await?;

    if opts.interactive
        || (opts.r#type.is_none()
            && opts.at.is_none()
            && opts.desc.is_none()
            && opts.slider.is_none()
            && opts.day.is_none()
            && opts.dotm.is_none()
            && opts.num_days.is_none())
    {
        println!("Current rhythm:");
        println!("  Description: {}", current.description);
        println!("  Rhythm: {}", current.rhythm);
        println!();

        let rhythm_type =
            prompt_input("Rhythm type (daily/weekly/monthly/every-n-days)")?.to_lowercase();
        let time_str = prompt_input("Time (HH:MM:SS)")?;
        let time = parse_time(&time_str)?;

        let rhythm = match rhythm_type.as_str() {
            "daily" => Rhythm::Daily { at: time },
            "weekly" => {
                let dotw = prompt_number::<u8>("Day of week (0=Monday, 6=Sunday)")?;
                let slider = prompt_slider()?;

                Rhythm::WeekDaily {
                    dotw,
                    at: time,
                    slider,
                }
            }
            "monthly" => {
                let dotm = prompt_number::<u32>("Day of month (0-30)")?;
                let slider = prompt_slider()?;

                Rhythm::Monthly {
                    dotm,
                    at: time,
                    slider,
                }
            }
            "every-n-days" => {
                let n = prompt_number::<u32>("Number of days")?;
                let slider = prompt_slider()?;

                Rhythm::EveryNDays {
                    n,
                    at: time,
                    slider,
                }
            }
            _ => {
                return Err("Invalid rhythm type".into());
            }
        };

        let description = prompt_input("Description")?;

        let client = reqwest::Client::new();
        let response = client
            .put(format!("{}/rhythms/{}", config.server_url, opts.id))
            .bearer_auth(token)
            .json(&CreateRhythmRequest {
                rhythm,
                description,
            })
            .send()
            .await?;

        check_response_success(response, "Update rhythm").await?;
    } else {
        let description = opts.desc.unwrap_or(current.description.clone());
        let time = if let Some(ref at_str) = opts.at {
            parse_time(at_str)?
        } else {
            current.rhythm.at()
        };

        let rhythm = if let Some(type_str) = opts.r#type {
            match type_str.to_lowercase().as_str() {
                "daily" => Rhythm::Daily { at: time },
                "weekly" => {
                    let dotw = opts.day.ok_or("--day required for weekly")?.parse()?;
                    if dotw > 6 {
                        return Err("Day of week must be in range 0-6 (0=Monday, 6=Sunday)".into());
                    }
                    let slider = if let Some(ref s) = opts.slider {
                        parse_slider(s)?
                    } else {
                        current.rhythm.slider()
                    };
                    Rhythm::WeekDaily {
                        dotw,
                        at: time,
                        slider,
                    }
                }
                "monthly" => {
                    let dotm = opts.dotm.ok_or("--dotm required for monthly")?.parse()?;
                    if dotm > 30 {
                        return Err("Day of month must be in range 0-30".into());
                    }
                    let slider = if let Some(ref s) = opts.slider {
                        parse_slider(s)?
                    } else {
                        current.rhythm.slider()
                    };
                    Rhythm::Monthly {
                        dotm,
                        at: time,
                        slider,
                    }
                }
                "every-n-days" => {
                    let n = opts
                        .num_days
                        .ok_or("--num-days required for every-n-days")?
                        .parse()?;
                    if n == 0 {
                        return Err("Number of days must be greater than 0".into());
                    }
                    let slider = if let Some(ref s) = opts.slider {
                        parse_slider(s)?
                    } else {
                        current.rhythm.slider()
                    };
                    Rhythm::EveryNDays {
                        n,
                        at: time,
                        slider,
                    }
                }
                _ => return Err("Invalid rhythm type".into()),
            }
        } else {
            current.rhythm
        };

        let client = reqwest::Client::new();
        let response = client
            .put(format!("{}/rhythms/{}", config.server_url, opts.id))
            .bearer_auth(token)
            .json(&CreateRhythmRequest {
                rhythm,
                description,
            })
            .send()
            .await?;

        check_response_success(response, "Update rhythm").await?;
    }

    println!("Rhythm updated successfully!");

    Ok(())
}

async fn cmd_delete(config: &Config, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let (opts, _remaining) = DeleteOptions::from_arguments_relaxed("cadence delete", &args_str);
    let token = config.require_auth()?;

    if !opts.force {
        let confirmation = prompt_input(&format!("Delete rhythm {}? (yes/no)", opts.id))?;
        if confirmation.to_lowercase() != "yes" {
            println!("Deletion cancelled.");
            return Ok(());
        }
    }

    let client = reqwest::Client::new();
    let response = client
        .delete(format!("{}/rhythms/{}", config.server_url, opts.id))
        .bearer_auth(token)
        .send()
        .await?;

    check_response_success(response, "Delete rhythm").await?;

    println!("Rhythm deleted successfully!");

    Ok(())
}

async fn cmd_edit(config: &Config, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let (opts, _remaining) = EditOptions::from_arguments_relaxed("cadence edit", &args_str);
    let token = config.require_auth()?;

    if opts.all {
        let client = reqwest::Client::new();
        let response = client
            .get(format!("{}/rhythms", config.server_url))
            .bearer_auth(token)
            .send()
            .await?;

        let response = check_response_success(response, "List rhythms").await?;
        let rhythms: Vec<RhythmResponse> = response.json().await?;

        let yaml = serde_yaml::to_string(&rhythms)?;
        let edited = edit::edit(yaml)?;
        let updated_rhythms: Vec<RhythmResponse> = serde_yaml::from_str(&edited)?;

        for rhythm in updated_rhythms {
            let client = reqwest::Client::new();
            let response = client
                .put(format!("{}/rhythms/{}", config.server_url, rhythm.id))
                .bearer_auth(token)
                .json(&CreateRhythmRequest {
                    rhythm: rhythm.rhythm,
                    description: rhythm.description,
                })
                .send()
                .await?;

            check_response_success(response, "Update rhythm").await?;
        }

        println!("All rhythms updated successfully!");
    } else if let Some(id) = opts.id {
        let client = reqwest::Client::new();
        let response = client
            .get(format!("{}/rhythms/{id}", config.server_url))
            .bearer_auth(token)
            .send()
            .await?;

        let response = check_response_success(response, "Get rhythm").await?;
        let rhythm: RhythmResponse = response.json().await?;

        let yaml = serde_yaml::to_string(&rhythm)?;
        let edited = edit::edit(yaml)?;
        let updated_rhythm: RhythmResponse = serde_yaml::from_str(&edited)?;

        let client = reqwest::Client::new();
        let response = client
            .put(format!("{}/rhythms/{id}", config.server_url))
            .bearer_auth(token)
            .json(&CreateRhythmRequest {
                rhythm: updated_rhythm.rhythm,
                description: updated_rhythm.description,
            })
            .send()
            .await?;

        check_response_success(response, "Update rhythm").await?;

        println!("Rhythm updated successfully!");
    } else {
        return Err("Either --all or <id> must be specified".into());
    }

    Ok(())
}

async fn get_user_timezone(config: &Config) -> Result<Tz, Box<dyn std::error::Error>> {
    let token = config.require_auth()?;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/user", config.server_url))
        .bearer_auth(token)
        .send()
        .await?;

    let response = check_response_success(response, "Get user info").await?;
    let user: UserResponse = response.json().await?;
    let tz: Tz = user.timezone.parse()?;
    Ok(tz)
}

async fn cmd_today(config: &Config, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let (opts, _remaining) = TodayOptions::from_arguments_relaxed("cadence today", &args_str);
    let token = config.require_auth()?;
    let user_tz = get_user_timezone(config).await?;

    let now = Utc::now().with_timezone(&user_tz);
    let today = now.date_naive();

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/schedule", config.server_url))
        .bearer_auth(token)
        .query(&[("start", today.to_string()), ("days", "90".to_string())])
        .send()
        .await?;

    let response = check_response_success(response, "Get schedule").await?;
    let items: Vec<ScheduleItem> = response.json().await?;

    let today_items: Vec<_> = items
        .into_iter()
        .filter(|item| {
            let local_time = item.datetime.with_timezone(&user_tz);
            local_time.date_naive() == today
        })
        .collect();

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&today_items)?);
    } else if today_items.is_empty() {
        println!("No scheduled items for today.");
    } else {
        for item in today_items {
            let local_time = item.datetime.with_timezone(&user_tz);
            println!(
                "{} - {} [{}]",
                local_time.format("%H:%M:%S"),
                item.description,
                item.rhythm_id
            );
        }
    }

    Ok(())
}

async fn cmd_schedule(config: &Config, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let (opts, _remaining) = ScheduleOptions::from_arguments_relaxed("cadence schedule", &args_str);
    let token = config.require_auth()?;

    let start_date = NaiveDate::parse_from_str(&opts.start, "%Y-%m-%d")?;
    let days: u32 = opts.days.parse()?;

    let user_tz = get_user_timezone(config).await?;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/schedule", config.server_url))
        .bearer_auth(token)
        .query(&ScheduleQuery {
            start: start_date,
            days,
        })
        .send()
        .await?;

    let response = check_response_success(response, "Get schedule").await?;
    let items: Vec<ScheduleItem> = response.json().await?;

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&items)?);
    } else if items.is_empty() {
        println!("No scheduled items.");
    } else {
        for item in items {
            let local_time = item.datetime.with_timezone(&user_tz);
            println!(
                "{} - {} [{}]",
                local_time.format("%Y-%m-%d %H:%M:%S"),
                item.description,
                item.rhythm_id
            );
        }
    }

    Ok(())
}

async fn cmd_convergence(
    config: &Config,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let (opts, _remaining) =
        ConvergenceOptions::from_arguments_relaxed("cadence convergence", &args_str);
    let token = config.require_auth()?;
    let user_tz = get_user_timezone(config).await?;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/convergence", config.server_url))
        .bearer_auth(token)
        .send()
        .await?;

    let response = check_response_success(response, "Get convergence").await?;
    let result: ConvergenceResponse = response.json().await?;

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

async fn cmd_delinquent(
    config: &Config,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let (opts, _remaining) =
        DelinquentOptions::from_arguments_relaxed("cadence delinquent", &args_str);
    let token = config.require_auth()?;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/delinquent", config.server_url))
        .bearer_auth(token)
        .send()
        .await?;

    let response = check_response_success(response, "Get delinquent items").await?;
    let items: Vec<DelinquentItem> = response.json().await?;

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&items)?);
    } else if items.is_empty() {
        println!("No delinquent rhythms.");
    } else {
        for item in items {
            println!("{} [{}]", item.description, item.rhythm_id);
        }
    }

    Ok(())
}

async fn cmd_mark_done(config: &Config, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let (opts, remaining) = MarkDoneOptions::from_arguments_relaxed("cadence mark-done", &args_str);
    let token = config.require_auth()?;

    let ids = if let Some(pattern) = opts.pattern {
        let client = reqwest::Client::new();
        let response = client
            .get(format!("{}/rhythms", config.server_url))
            .bearer_auth(token)
            .send()
            .await?;

        let response = check_response_success(response, "List rhythms").await?;
        let rhythms: Vec<RhythmResponse> = response.json().await?;

        let pattern = pattern.to_lowercase();
        rhythms
            .into_iter()
            .filter(|r| r.description.to_lowercase().contains(&pattern))
            .map(|r| r.id)
            .collect()
    } else {
        remaining
    };

    if ids.is_empty() {
        println!("No rhythms to mark as done.");
        return Ok(());
    }

    for id in ids {
        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/rhythms/{id}/done", config.server_url))
            .bearer_auth(token)
            .json(&MarkDoneRequest { when: None })
            .send()
            .await?;

        check_response_success(response, "Mark done").await?;
        println!("Rhythm {id} marked as done.");
    }

    Ok(())
}

async fn cmd_defer(config: &Config, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let (opts, _remaining) = DeferOptions::from_arguments_relaxed("cadence defer", &args_str);
    let token = config.require_auth()?;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/rhythms/{}/defer", config.server_url, opts.id))
        .bearer_auth(token)
        .json(&DeferRequest { when: None })
        .send()
        .await?;

    check_response_success(response, "Defer rhythm").await?;

    println!("Rhythm deferred.");

    Ok(())
}

async fn cmd_spoons(config: &Config, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let (opts, _remaining) = SpoonsOptions::from_arguments_relaxed("cadence spoons", &args_str);
    let token = config.require_auth()?;

    let date = NaiveDate::parse_from_str(&opts.date, "%Y-%m-%d")?;
    let value: u8 = opts.value.parse()?;
    if value > 10 {
        return Err("Spoons value must be in range 0-10".into());
    }

    let client = reqwest::Client::new();
    let response = client
        .put(format!("{}/spoons", config.server_url))
        .bearer_auth(token)
        .json(&SetSpoonsRequest { date, value })
        .send()
        .await?;

    check_response_success(response, "Set spoons").await?;

    println!("Spoons set to {value} for {date}.");

    Ok(())
}

async fn cmd_export(config: &Config, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let (_opts, remaining) = ExportOptions::from_arguments_relaxed("cadence export", &args_str);
    let token = config.require_auth()?;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/rhythms", config.server_url))
        .bearer_auth(token)
        .send()
        .await?;

    let response = check_response_success(response, "List rhythms").await?;
    let rhythms: Vec<RhythmResponse> = response.json().await?;

    let rhythms_to_export: Vec<_> = if remaining.is_empty() {
        rhythms
    } else {
        rhythms
            .into_iter()
            .filter(|r| remaining.contains(&r.id))
            .collect()
    };

    let yaml = serde_yaml::to_string(&rhythms_to_export)?;
    println!("{yaml}");

    Ok(())
}

async fn cmd_import(config: &Config, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let (opts, _remaining) = ImportOptions::from_arguments_relaxed("cadence import", &args_str);
    let token = config.require_auth()?;

    let file_path = std::path::Path::new(&opts.file);
    let canonical_path = file_path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve file path '{}': {}", opts.file, e))?;

    if !canonical_path.is_file() {
        return Err(format!("'{}' is not a file", opts.file).into());
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
    let rhythms: Vec<RhythmResponse> = serde_yaml::from_str(&contents)?;

    if opts.dry_run {
        println!("Dry run: would import {} rhythms:", rhythms.len());
        for rhythm in rhythms {
            println!("  - {}: {}", rhythm.id, rhythm.description);
        }
        return Ok(());
    }

    for rhythm in rhythms {
        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/rhythms", config.server_url))
            .bearer_auth(token)
            .json(&CreateRhythmRequest {
                rhythm: rhythm.rhythm,
                description: rhythm.description,
            })
            .send()
            .await?;

        check_response_success(response, "Create rhythm").await?;
    }

    println!("Rhythms imported successfully!");

    Ok(())
}
