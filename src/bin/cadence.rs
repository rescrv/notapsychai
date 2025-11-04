use std::io::{self, Write};
use std::str::FromStr;

use chrono::{NaiveDate, NaiveTime, Utc};
use chrono_tz::Tz;
use notapsychai::api_types::{
    ConvergenceResponse, CreateRhythmRequest, DeferRequest, DelinquentItem, LoginRequest,
    LoginResponse, MarkDoneRequest, RegisterRequest, RegisterResponse, RhythmResponse,
    ScheduleItem, ScheduleQuery, SetSpoonsRequest, UserResponse,
};
use notapsychai::{Rhythm, Slider};
use serde::{Deserialize, Serialize};

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

const HELP: &str = "
cadence - Rhythm management CLI

USAGE:
    cadence [OPTIONS] <COMMAND> [ARGS]

OPTIONS:
    --server <URL>          Server URL [default: http://localhost:3000]

COMMANDS:
    register                Register a new user account
    login                   Login and save authentication token
    add                     Add a new rhythm interactively
    list                    List all rhythms
    get <id>                Get details of a specific rhythm
    update <id>             Update a rhythm interactively
    delete <id>             Delete a rhythm
    today                   Show today's tasks
    schedule <start> <days> Show schedule for date range
    convergence             Show when all rhythms converge
    delinquent              Show delinquent rhythms
    mark-done <id>          Mark a rhythm as done
    defer <id>              Defer a rhythm
    spoons <date> <value>   Set spoons for a date
    help                    Show this help message
";

#[derive(Serialize, Deserialize, Default)]
struct Config {
    server_url: String,
    auth_token: Option<String>,
}

impl Config {
    fn load() -> Self {
        let config_path = Self::config_path();
        if let Ok(contents) = std::fs::read_to_string(config_path) {
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
        std::fs::write(config_path, json)?;
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args();
    let _program = args.next();

    let mut config = Config::load();
    let mut command_args = Vec::new();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--server" => {
                config.server_url = args.next().expect("--server requires a value");
            }
            _ => {
                command_args.push(arg);
                command_args.extend(args);
                break;
            }
        }
    }

    if command_args.is_empty() {
        println!("{HELP}");
        return Ok(());
    }

    let command = &command_args[0];
    let args = &command_args[1..];

    match command.as_str() {
        "register" => cmd_register(&mut config).await,
        "login" => cmd_login(&mut config).await,
        "add" => cmd_add(&config).await,
        "list" => cmd_list(&config).await,
        "get" => {
            if args.len() != 1 {
                eprintln!("Usage: cadence get <rhythm-id>");
                std::process::exit(1);
            }
            cmd_get(&config, &args[0]).await
        }
        "update" => {
            if args.len() != 1 {
                eprintln!("Usage: cadence update <rhythm-id>");
                std::process::exit(1);
            }
            cmd_update(&config, &args[0]).await
        }
        "delete" => {
            if args.len() != 1 {
                eprintln!("Usage: cadence delete <rhythm-id>");
                std::process::exit(1);
            }
            cmd_delete(&config, &args[0]).await
        }
        "today" => cmd_today(&config).await,
        "schedule" => {
            if args.len() != 2 {
                eprintln!("Usage: cadence schedule <start-date> <days>");
                std::process::exit(1);
            }
            cmd_schedule(&config, &args[0], &args[1]).await
        }
        "convergence" => cmd_convergence(&config).await,
        "delinquent" => cmd_delinquent(&config).await,
        "mark-done" => {
            if args.len() != 1 {
                eprintln!("Usage: cadence mark-done <rhythm-id>");
                std::process::exit(1);
            }
            cmd_mark_done(&config, &args[0]).await
        }
        "defer" => {
            if args.len() != 1 {
                eprintln!("Usage: cadence defer <rhythm-id>");
                std::process::exit(1);
            }
            cmd_defer(&config, &args[0]).await
        }
        "spoons" => {
            if args.len() != 2 {
                eprintln!("Usage: cadence spoons <date> <value>");
                std::process::exit(1);
            }
            cmd_spoons(&config, &args[0], &args[1]).await
        }
        "help" => {
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

async fn cmd_register(config: &mut Config) -> Result<(), Box<dyn std::error::Error>> {
    let username = prompt_input("Username")?;
    let email = prompt_input("Email")?;
    let password = prompt_password("Password")?;
    let timezone_input = prompt_input("Timezone [UTC]")?;
    let timezone = if timezone_input.is_empty() {
        "UTC"
    } else {
        &timezone_input
    };

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/auth/register", config.server_url))
        .json(&RegisterRequest {
            username: username.to_string(),
            email: email.to_string(),
            password,
            timezone: timezone.to_string(),
        })
        .send()
        .await?;

    let response = check_response_success(response, "Registration").await?;
    let _result: RegisterResponse = response.json().await?;
    println!("Registration successful! Please login with 'cadence login'");

    Ok(())
}

async fn cmd_login(config: &mut Config) -> Result<(), Box<dyn std::error::Error>> {
    let username = prompt_input("Username")?;
    let password = prompt_password("Password")?;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/auth/login", config.server_url))
        .json(&LoginRequest {
            username: username.to_string(),
            password,
        })
        .send()
        .await?;

    let response = check_response_success(response, "Login").await?;
    let result: LoginResponse = response.json().await?;
    config.auth_token = Some(result.token);
    config.save()?;

    println!("Login successful!");

    Ok(())
}

async fn cmd_add(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let token = config.require_auth()?;

    let rhythm_type =
        prompt_input("Rhythm type (daily/weekly/monthly/every-n-days)")?.to_lowercase();
    let time_str = prompt_input("Time (HH:MM:SS)")?;
    let time = NaiveTime::parse_from_str(&time_str, "%H:%M:%S")?;

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

async fn cmd_list(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let token = config.require_auth()?;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/rhythms", config.server_url))
        .bearer_auth(token)
        .send()
        .await?;

    let response = check_response_success(response, "List rhythms").await?;
    let rhythms: Vec<RhythmResponse> = response.json().await?;

    if rhythms.is_empty() {
        println!("No rhythms found.");
    } else {
        for rhythm in rhythms {
            println!(
                "{}: {} - {:?}",
                rhythm.id, rhythm.description, rhythm.rhythm
            );
        }
    }

    Ok(())
}

async fn cmd_get(config: &Config, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let token = config.require_auth()?;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/rhythms/{id}", config.server_url))
        .bearer_auth(token)
        .send()
        .await?;

    let response = check_response_success(response, "Get rhythm").await?;
    let rhythm: RhythmResponse = response.json().await?;

    println!("ID: {}", rhythm.id);
    println!("Description: {}", rhythm.description);
    println!("Rhythm: {:?}", rhythm.rhythm);
    println!("Created: {} ({})", rhythm.created_at, rhythm.created_at_tz);
    println!(
        "Modified: {} ({})",
        rhythm.modified_at, rhythm.modified_at_tz
    );

    Ok(())
}

async fn cmd_update(config: &Config, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let token = config.require_auth()?;

    let client = reqwest::Client::new();
    let get_response = client
        .get(format!("{}/rhythms/{id}", config.server_url))
        .bearer_auth(token)
        .send()
        .await?;

    let get_response = check_response_success(get_response, "Get rhythm").await?;
    let current: RhythmResponse = get_response.json().await?;

    println!("Current rhythm:");
    println!("  Description: {}", current.description);
    println!("  Rhythm: {:?}", current.rhythm);
    println!();

    let rhythm_type =
        prompt_input("Rhythm type (daily/weekly/monthly/every-n-days)")?.to_lowercase();
    let time_str = prompt_input("Time (HH:MM:SS)")?;
    let time = NaiveTime::parse_from_str(&time_str, "%H:%M:%S")?;

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
        .put(format!("{}/rhythms/{id}", config.server_url))
        .bearer_auth(token)
        .json(&CreateRhythmRequest {
            rhythm,
            description,
        })
        .send()
        .await?;

    check_response_success(response, "Update rhythm").await?;

    println!("Rhythm updated successfully!");

    Ok(())
}

async fn cmd_delete(config: &Config, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let token = config.require_auth()?;

    let confirmation = prompt_input(&format!("Delete rhythm {id}? (yes/no)"))?;
    if confirmation.to_lowercase() != "yes" {
        println!("Deletion cancelled.");
        return Ok(());
    }

    let client = reqwest::Client::new();
    let response = client
        .delete(format!("{}/rhythms/{id}", config.server_url))
        .bearer_auth(token)
        .send()
        .await?;

    check_response_success(response, "Delete rhythm").await?;

    println!("Rhythm deleted successfully!");

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

async fn cmd_today(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
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

    if today_items.is_empty() {
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

async fn cmd_schedule(
    config: &Config,
    start: &str,
    days: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let token = config.require_auth()?;
    let start_date = NaiveDate::parse_from_str(start, "%Y-%m-%d")?;
    let days: u32 = days.parse()?;

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

    if items.is_empty() {
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

async fn cmd_convergence(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
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

    Ok(())
}

async fn cmd_delinquent(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let token = config.require_auth()?;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/delinquent", config.server_url))
        .bearer_auth(token)
        .send()
        .await?;

    let response = check_response_success(response, "Get delinquent items").await?;
    let items: Vec<DelinquentItem> = response.json().await?;

    if items.is_empty() {
        println!("No delinquent rhythms.");
    } else {
        for item in items {
            println!("{} [{}]", item.description, item.rhythm_id);
        }
    }

    Ok(())
}

async fn cmd_mark_done(config: &Config, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let token = config.require_auth()?;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/rhythms/{id}/done", config.server_url))
        .bearer_auth(token)
        .json(&MarkDoneRequest { when: None })
        .send()
        .await?;

    check_response_success(response, "Mark done").await?;

    println!("Rhythm marked as done.");

    Ok(())
}

async fn cmd_defer(config: &Config, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let token = config.require_auth()?;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/rhythms/{id}/defer", config.server_url))
        .bearer_auth(token)
        .json(&DeferRequest { when: None })
        .send()
        .await?;

    check_response_success(response, "Defer rhythm").await?;

    println!("Rhythm deferred.");

    Ok(())
}

async fn cmd_spoons(
    config: &Config,
    date: &str,
    value: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let token = config.require_auth()?;
    let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")?;
    let value: u8 = value.parse()?;

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
