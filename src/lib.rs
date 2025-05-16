use std::fs::OpenOptions;
use std::io::Write;

use chrono::Local;
use rustyline::config::EditMode;
use rustyline::error::ReadlineError;
use rustyline::hint::HistoryHinter;
use rustyline::{CompletionType, Config, Editor, EventHandler, KeyEvent};

pub mod stayfocused;

mod cli;

use cli::{CommandHint, ShellHelper, TabEventHandler};

const LAST_SLEPT: &str = "last-slept";
const SLEPT_HOW_LONG: &str = "slept-how-long";
const QUALITY_OF_SLEEP: &str = "quality-of-sleep";
const MEDICATION: &str = "medication";
const HYGIENE: &str = "hygiene";

/*
/////////////////////////////////////////////// Error //////////////////////////////////////////////

#[derive(Debug)]
pub enum Error {
    Internal(String),
    IO(std::io::Error),
    Json(serde_json::Error),
    Reqwest(reqwest::Error),
    Yammer(yammer::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:#?}", self)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::IO(err)
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Self::Reqwest(err)
    }
}

impl From<yammer::Error> for Error {
    fn from(err: yammer::Error) -> Self {
        Self::Yammer(err)
    }
}

///////////////////////////////////////////// NotAPsych ////////////////////////////////////////////

pub struct NotAPsych<HELPER: rustyline::Helper, HISTORY: rustyline::history::History> {
    editor: Editor<HELPER, HISTORY>,
}

impl<HELPER: rustyline::Helper, HISTORY: rustyline::history::History> NotAPsych<HELPER, HISTORY> {
    pub async fn checkin(&mut self) {
        self.sleep_checkin().await;
        println!();
        self.medications().await;
        println!();
        self.hygiene().await;
    }

    pub async fn sleep_checkin(&mut self) {
        self.last_slept().await;
        println!();
        self.slept_how_long().await;
        println!();
        self.quality_of_sleep().await;
    }

    pub async fn last_slept(&mut self) {
        #[derive(serde::Deserialize, yammer_derive::JsonSchema)]
        struct LastSleptAnswer {
            awake_hours: f64,
            justification: String,
        }
        for idx in 0..3 {
            let answer: LastSleptAnswer = match self
                .question_and_answer(LAST_SLEPT, "When did you last wakeup? ")
                .await
            {
                Ok(Some(answer)) => answer,
                Ok(None) => {
                    eprintln!("A blank answer is unacceptable (unless given three times).");
                    continue;
                }
                Err(err) => {
                    if idx < 2 {
                        eprintln!("error: {err}\n\nPlease try again:\n\n");
                    }
                    continue;
                }
            };
            let log_line = LogLine::LastSlept {
                recorded_at: Local::now().fixed_offset().to_rfc3339(),
                awake_hours: answer.awake_hours,
                justification: answer.justification,
            };
            self.log(log_line);
            return;
        }
        eprintln!("Could not interpret input, moving on...\n\n");
    }

    pub async fn slept_how_long(&mut self) {
        #[derive(serde::Deserialize, yammer_derive::JsonSchema)]
        struct SleptHowLongAnswer {
            sleep_hours: f64,
            justification: String,
        }
        for idx in 0..3 {
            let answer: SleptHowLongAnswer = match self
                .question_and_answer(
                    SLEPT_HOW_LONG,
                    "When you last slept, for how many hours did you sleep? ",
                )
                .await
            {
                Ok(Some(answer)) => answer,
                Ok(None) => {
                    eprintln!("A blank answer is unacceptable (unless given three times).");
                    continue;
                }
                Err(err) => {
                    if idx < 2 {
                        eprintln!("error: {err}\n\nPlease try again:\n\n");
                    }
                    continue;
                }
            };
            let log_line = LogLine::HoursSlept {
                recorded_at: Local::now().fixed_offset().to_rfc3339(),
                sleep_hours: answer.sleep_hours,
                justification: answer.justification,
            };
            self.log(log_line);
            return;
        }
        eprintln!("Could not interpret input, moving on...\n\n");
    }

    pub async fn quality_of_sleep(&mut self) {
        #[derive(serde::Deserialize, yammer_derive::JsonSchema)]
        struct QualityOfSleepAnswer {
            answer: f64,
            justification: String,
        }
        for idx in 0..3 {
            let answer: QualityOfSleepAnswer = match self
                .question_and_answer(
                    QUALITY_OF_SLEEP,
                    "How would you rate the quality of your last sleep schedule on a scale of 1 being the worst and 10 being the best? "
                )
                .await
            {
                Ok(Some(answer)) => answer,
                Ok(None) => {
                    eprintln!("A blank answer is unacceptable (unless given three times).");
                    continue;
                }
                Err(err) => {
                    if idx < 2 {
                        eprintln!("error: {err}\n\nPlease try again:\n\n");
                    }
                    continue;
                }
            };
            let log_line = LogLine::SleepQuality {
                recorded_at: Local::now().fixed_offset().to_rfc3339(),
                answer: answer.answer,
                justification: answer.justification,
            };
            self.log(log_line);
            return;
        }
        eprintln!("Could not interpret input, moving on...\n\n");
    }

    pub async fn medications(&mut self) {
        #[derive(serde::Deserialize, yammer_derive::JsonSchema)]
        struct MedicationAnswer {
            substance: String,
            quantity: f64,
            units: String,
            times_daily: f64,
            justification: String,
        }
        let mut failures = 0;
        loop {
            let answer: MedicationAnswer = match self.question_and_answer(MEDICATION,
                    "List every medication you took since last checkin, and the dosage, e.g. \"30mg magic pill 3x daily.\"
List caffeine, nicotine, and other substances as appropriate.
Enter an empty line to continue: ").await {
                Ok(Some(answer)) => answer,
                Ok(None) => {
                    return;
                }
                Err(err) => {
                    if failures < 2 {
                        eprintln!("error: {err}\n\nPlease try again:\n\n");
                        continue;
                    } else {
                        eprintln!("Could not interpret input, moving on...\n\n");
                        return;
                    }
                }
            };
            let log_line = LogLine::Medication {
                recorded_at: Local::now().fixed_offset().to_rfc3339(),
                substance: answer.substance,
                dose: Dose::Daily {
                    quantity: answer.quantity,
                    units: answer.units,
                    times_daily: answer.times_daily,
                },
                justification: answer.justification,
            };
            self.log(log_line);
            println!();
            failures = 0;
        }
    }

    pub async fn hygiene(&mut self) {
        let hygiene_system = self.load_system(HYGIENE);
        let hygiene = self
                .read_line(
                    "Describe your hygiene in a way that translates to POOR, FAIR, GOOD, GREAT, EXCELLENT since your last report: "
                )
                .await;
        let req = GenerateRequest {
            model: self.model(),
            prompt: hygiene,
            format: Some(serde_json::json! {{
                "type": "object",
                "properties": {
                    "answer": {
                        "type": "string",
                        "enum": [
                            "POOR",
                            "FAIR",
                            "GOOD",
                            "GREAT",
                            "EXCELLENT"
                        ]
                    },
                    "justification": {
                        "type": "string"
                    }
                },
                "required": [
                  "answer",
                  "justification"
                ]
            }}),
            system: Some(hygiene_system),
            suffix: None,
            stream: Some(false),
            images: None,
            template: None,
            raw: None,
            keep_alive: None,
            options: None,
        };
        let req = req.make_request(&self.ollama_host());
        let spinner = Spinner::new();
        spinner.start();
        let resp = req
            .send()
            .await
            .expect("encountered an error")
            .error_for_status()
            .expect("encountered an error")
            .text()
            .await
            .expect("encountered an error");
        let resp: yammer::GenerateResponse =
            serde_json::from_str(&resp).expect("json should parse");
        spinner.inhibit();
        #[derive(serde::Deserialize)]
        struct Justification {
            answer: String,
            justification: String,
        }
        let answer: Justification = match serde_json::from_str(&resp.response) {
            Ok(json) => json,
            Err(err) => {
                eprintln!("Model gave bogus json: {err}");
                return;
            }
        };
        let log_line = LogLine::Hygiene {
            recorded_at: Local::now().fixed_offset().to_rfc3339(),
            hygiene: answer.answer,
            justification: answer.justification,
        };
        self.log(log_line);
    }

    async fn question_and_answer<T: for<'a> serde::Deserialize<'a> + yammer::JsonSchema>(
        &mut self,
        system: &str,
        question: &str,
    ) -> Result<Option<T>, Error> {
        let system = self.load_system(system);
        let answer = self.read_line(question).await;
        if answer.trim().is_empty() {
            return Ok(None);
        }
        let req = GenerateRequest {
            model: self.model(),
            prompt: answer,
            format: Some(T::json_schema()),
            system: Some(system),
            suffix: None,
            stream: Some(false),
            images: None,
            template: None,
            raw: None,
            keep_alive: None,
            options: None,
        };
        let req = req.make_request(&self.ollama_host());
        let spinner = Spinner::new();
        spinner.start();
        let resp = req
            .send()
            .await?
            .error_for_status()?
            .json::<GenerateResponse>()
            .await?;
        spinner.inhibit();
        Ok(Some(serde_json::from_str(&resp.response)?))
    }

    fn model(&self) -> String {
        match std::env::var("NOTAPSYCH_MODEL") {
            Ok(model) => model,
            Err(_) => {
                eprintln!("please set NOTAPSYCH_MODEL in your environment");
                std::process::exit(13);
            }
        }
    }

    fn load_system(&self, slug: &str) -> String {
        // TODO(rescrv):  Load from filesystem or a remote database?
        match slug {
            LAST_SLEPT => r#"Measure the time since the user reports they last wokeup.

You are to provide your answer in hours, along with a justification in plain text.  Respond in
JSON.

To calculate this accurately, you must think step-by-step.  For example, if the user reports they
last slept at 5:30am yesterday, and it is now 3:15pm today, first compute that there are 18.5 hours
between 5:30am and midnight, and then 15.25 hours between midnight and now.  Add 18.5 + 15.25 to
get 33.75 hours.  Double check your math by working in reverse, starting from now and computing backwards,

Triple check your results by computing the roundup to the nearest hour at each end and then count
the intervening hours.  For example, if the user reports they last woke at 7:25am and it is now
5:45pm., round up 25 minutes to the hour to get 35 minutes, round 5:45pm down to the hour to get 45
minutes (the number of minutes past the hour).  Then count that there are 9 hours between 8:00am
and 5:00pm, for a total of 9 hours + 35 minutes + 45 minutes, or 10 hours, 20 minutes.

When all three computations agree, report your results.

"#
            .to_string() + &format!("It is currently {}.", Local::now().to_rfc2822()),
            SLEPT_HOW_LONG => r#"Measure the number of hours the user reports they slept during their most recent sleep cycle.

Example:
"8 hours" => {"sleep_hours": 8, "justification": "The user said they slept 8 hours."}
"#.to_string(),
            QUALITY_OF_SLEEP => r#"Interpret the user's response as a number on a scale from 0.0 to 10.0"#.to_string(),
            MEDICATION => r#"Parse the amount of medication the user reports taking.

Report -1 times-daily when there is not enough information to make a decision.

Respond using JSON.

Example generic:
30mg of something once daily => {"medication": "something", "quantity": 30, "units": "mg", "times-daily": 1}

Example branded, as needed (report once per day):
Advil, 200mg, as needed => {"medication": "Advil", "quantity": 200, "units": "mg", "times-daily": 1}

Example caffeine:
Two cups of coffee per day => {"medication": "caffeine", "quantity": 150, "units": "mg", "times-daily": 2}

Example alcohol (assume: 0.02 BAC per liquor shot, wine glass, or beer bottle, in a single sitting):
Two shots of whisky and a shot of whiskey => {"medication": "alcohol", "quantity": 0.06, "units": "BAC", "times-daily": 1}
One glass of red wine with dinner => {"medication": "alcohol", "quantity": 0.02, "units": "BAC", "times-daily": 1}
A fifth of Jack, once a month => {"medication": "alcohol", "quantity": 0.48, "units": "BAC", "times-daily": 0.033333}
A fifth of Jack => {"medication": "alcohol", "quantity": 0.48, "units": "BAC", "times-daily": -1}
A thirty rack of bud with Tucker => {"medication": "alcohol", "quantity": 0.60, "units": "BAC", "times-daily": 1}

Example nicotine:
Smoke a pack a day => {"medication": "nicotine", "quantity": 10, "units": "mg", "times-daily": 20}
Smoke two packs a day => {"medication": "nicotine", "quantity": 10, "units": "mg", "times-daily": 40}
"#.to_string(),
            HYGIENE => r#"Make a judgement call about the user's hygiene habits.

Someone who showers and shaves every day has excellent hygiene.
Someone who showers infrequently has poor hygiene.
It is a spectrum of POOR, FAIR, GOOD, GREAT, EXCELLENT.
"#.to_string(),
            _ => panic!("logic error: {slug} not supported"),
        }
    }

    fn ollama_host(&self) -> String {
        match std::env::var("OLLAMA_HOST") {
            Ok(model) => model,
            Err(_) => {
                eprintln!("please set OLLAMA_HOST in your environment");
                std::process::exit(13);
            }
        }
    }

    fn log(&self, log_line: LogLine) {
        let transcript = match std::env::var("NOTAPSYCH_TRANSCRIPT") {
            Ok(transcript) => transcript,
            Err(_) => {
                eprintln!("please set NOTAPSYCH_TRANSCRIPT in your environment");
                std::process::exit(13);
            }
        };
        let mut log = OpenOptions::new()
            .append(true)
            .create(true)
            .open(transcript)
            .expect("could not open transcript for append");
        log.write_all(
            (serde_json::to_string(&log_line).expect("log line should always serialize") + "\n")
                .as_bytes(),
        )
        .expect("could not append to log; it may be corrupt");
    }

    async fn read_line(&mut self, question: &str) -> String {
        match self.editor.readline(question) {
            Ok(line) => line,
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                std::process::exit(0);
            }
            Err(err) => {
                eprintln!("could not read line: {}", err);
                std::process::exit(13);
            }
        }
    }
}

pub async fn notapsych() {
    let config = Config::builder()
        .auto_add_history(true)
        .edit_mode(EditMode::Vi)
        .completion_type(CompletionType::List)
        .check_cursor_position(true)
        .max_history_size(1_000_000)
        .expect("this should always work")
        .history_ignore_dups(true)
        .expect("this should always work")
        .history_ignore_space(true)
        .build();
    let history = rustyline::history::FileHistory::new();
    let mut rl = Editor::with_history(config, history).expect("this should always work");
    let commands = vec![
        CommandHint::new(":help", ":help"),
        CommandHint::new(":exit", ":exit"),
        CommandHint::new(":quit", ":quit"),
    ];
    let h = ShellHelper {
        commands: commands.clone(),
        hinter: HistoryHinter::new(),
        hints: commands.clone(),
    };
    rl.set_helper(Some(h));
    rl.bind_sequence(
        KeyEvent::from('\t'),
        EventHandler::Conditional(Box::new(TabEventHandler)),
    );
    let mut not_a_psych = NotAPsych { editor: rl };
    not_a_psych.checkin().await;
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(tag = "type")]
pub enum LogLine {
    #[serde(rename = "last-slept")]
    LastSlept {
        recorded_at: String,
        awake_hours: f64,
        justification: String,
    },
    #[serde(rename = "hours-slept")]
    HoursSlept {
        recorded_at: String,
        sleep_hours: f64,
        justification: String,
    },
    #[serde(rename = "sleep-quality")]
    SleepQuality {
        recorded_at: String,
        answer: f64,
        justification: String,
    },
    #[serde(rename = "medication")]
    Medication {
        recorded_at: String,
        substance: String,
        dose: Dose,
        justification: String,
    },
    #[serde(rename = "hygiene")]
    Hygiene {
        recorded_at: String,
        hygiene: String,
        justification: String,
    },
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(tag = "type")]
pub enum Dose {
    #[serde(rename = "daily")]
    Daily {
        quantity: f64,
        units: String,
        times_daily: f64,
    },
}
*/
