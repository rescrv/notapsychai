use std::env;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use chrono::Utc;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

use notapsychai::api_types::{
    ConvergenceResponse, CreateRhythmRequest, DeferRequest, DelinquentItem, LoginRequest,
    LoginResponse, MarkDoneRequest, RegisterRequest, RegisterResponse, RhythmResponse,
    ScheduleItem, ScheduleQuery, SetSpoonsRequest, SpoonsQuery, SpoonsResponse, UpdateUserRequest,
    UserResponse,
};
use notapsychai::{auth, db, RhythmDefinition, RhythmID};

struct AppError(StatusCode, String);

impl From<notapsychai::Error> for AppError {
    fn from(err: notapsychai::Error) -> Self {
        AppError(StatusCode::INTERNAL_SERVER_ERROR, format!("{err}"))
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.0, self.1).into_response()
    }
}

#[derive(Clone)]
struct AppState {
    pool: PgPool,
    jwt_secret: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let jwt_secret = env::var("JWT_SECRET").expect("JWT_SECRET must be set");
    let port = env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let allowed_origins = env::var("ALLOWED_ORIGINS").unwrap_or_else(|_| "*".to_string());

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    let state = Arc::new(AppState { pool, jwt_secret });

    let cors = if allowed_origins == "*" {
        CorsLayer::permissive()
    } else {
        CorsLayer::new()
            .allow_origin(allowed_origins.parse::<axum::http::HeaderValue>()?)
            .allow_methods(Any)
            .allow_headers(Any)
            .allow_credentials(true)
    };

    let app = Router::new()
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/rhythms", get(list_rhythms).post(create_rhythm))
        .route(
            "/rhythms/:id",
            get(get_rhythm).put(update_rhythm).delete(delete_rhythm),
        )
        .route("/rhythms/:id/done", post(mark_done))
        .route("/rhythms/:id/defer", post(defer_rhythm))
        .route("/schedule", get(get_schedule))
        .route("/delinquent", get(get_delinquent))
        .route("/convergence", get(get_convergence))
        .route("/spoons", get(get_spoons).put(set_spoons))
        .route("/user", get(get_user).put(update_user))
        .layer(cors)
        .with_state(state);

    let addr: SocketAddr = format!("0.0.0.0:{port}").parse()?;
    println!("Rhythm server listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn extract_bearer_token(headers: &HeaderMap) -> Result<&str, AppError> {
    let auth_header = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError(
            StatusCode::UNAUTHORIZED,
            "Missing Authorization header".to_string(),
        ))?;

    auth_header.strip_prefix("Bearer ").ok_or(AppError(
        StatusCode::UNAUTHORIZED,
        "Invalid Authorization format".to_string(),
    ))
}

async fn extract_user_from_headers(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<Uuid, AppError> {
    let token = extract_bearer_token(headers)?;

    let claims = auth::validate_jwt(token, &state.jwt_secret)
        .map_err(|e| AppError(StatusCode::UNAUTHORIZED, format!("Invalid token: {e}")))?;

    let user_id = Uuid::parse_str(&claims.sub).map_err(|e| {
        AppError(
            StatusCode::UNAUTHORIZED,
            format!("Invalid user ID in token: {e}"),
        )
    })?;

    Ok(user_id)
}

async fn register(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, AppError> {
    let password_hash = auth::hash_password(&req.password)?;

    let user_id = db::create_user(
        &state.pool,
        &req.username,
        &req.email,
        &password_hash,
        &req.timezone,
    )
    .await?;

    Ok(Json(RegisterResponse {
        user_id: user_id.to_string(),
    }))
}

async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, AppError> {
    let user_opt = db::get_user_by_username(&state.pool, &req.username).await?;

    let (user_id, password_hash, _timezone) = user_opt.ok_or(AppError(
        StatusCode::UNAUTHORIZED,
        "Invalid username or password".to_string(),
    ))?;

    let valid = auth::verify_password(&req.password, &password_hash)?;

    if !valid {
        return Err(AppError(
            StatusCode::UNAUTHORIZED,
            "Invalid username or password".to_string(),
        ));
    }

    let token = auth::generate_jwt(user_id, &state.jwt_secret, 7)?;
    let token_hash = auth::hash_token_for_storage(&token);
    let expires_at = auth::token_expiration(7);

    db::create_session(&state.pool, user_id, &token_hash, expires_at).await?;

    Ok(Json(LoginResponse { token }))
}

async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<StatusCode, AppError> {
    let token = extract_bearer_token(&headers)?;
    let token_hash = auth::hash_token_for_storage(token);
    db::delete_session(&state.pool, &token_hash).await?;

    Ok(StatusCode::OK)
}

fn rhythm_def_to_response(def: RhythmDefinition) -> RhythmResponse {
    RhythmResponse {
        id: def.id.to_string(),
        rhythm: def.rhythm,
        description: def.description,
        created_at: def.created_at,
        created_at_tz: def.created_at_tz,
        modified_at: def.modified_at,
        modified_at_tz: def.modified_at_tz,
    }
}

async fn list_rhythms(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<RhythmResponse>>, AppError> {
    let user_id = extract_user_from_headers(&headers, &state).await?;
    let rhythms = db::load_rhythms(&state.pool, user_id).await?;
    let response: Vec<RhythmResponse> = rhythms.into_iter().map(rhythm_def_to_response).collect();
    Ok(Json(response))
}

async fn create_rhythm(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<CreateRhythmRequest>,
) -> Result<Json<RhythmResponse>, AppError> {
    let user_id = extract_user_from_headers(&headers, &state).await?;
    let timezone = db::get_user_timezone(&state.pool, user_id).await?;

    let id = RhythmID::generate().ok_or(AppError(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Failed to generate ID".to_string(),
    ))?;

    let rhythm_def = RhythmDefinition {
        id,
        rhythm: req.rhythm,
        description: req.description,
        created_at: Utc::now(),
        created_at_tz: timezone.clone(),
        modified_at: Utc::now(),
        modified_at_tz: timezone,
    };

    db::save_rhythm(&state.pool, user_id, &rhythm_def).await?;

    Ok(Json(rhythm_def_to_response(rhythm_def)))
}

async fn get_rhythm(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<RhythmResponse>, AppError> {
    let user_id = extract_user_from_headers(&headers, &state).await?;
    let rhythm_id = RhythmID::from_human_readable(&id).ok_or(AppError(
        StatusCode::BAD_REQUEST,
        "Invalid rhythm ID".to_string(),
    ))?;

    let rhythms = db::load_rhythms(&state.pool, user_id).await?;
    let rhythm = rhythms
        .into_iter()
        .find(|r| r.id == rhythm_id)
        .ok_or(AppError(
            StatusCode::NOT_FOUND,
            "Rhythm not found".to_string(),
        ))?;

    Ok(Json(rhythm_def_to_response(rhythm)))
}

async fn update_rhythm(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<CreateRhythmRequest>,
) -> Result<Json<RhythmResponse>, AppError> {
    let user_id = extract_user_from_headers(&headers, &state).await?;
    let rhythm_id = notapsychai::RhythmID::from_human_readable(&id).ok_or(AppError(
        StatusCode::BAD_REQUEST,
        "Invalid rhythm ID".to_string(),
    ))?;

    let rhythms = db::load_rhythms(&state.pool, user_id).await?;
    let existing = rhythms.iter().find(|r| r.id == rhythm_id).ok_or(AppError(
        StatusCode::NOT_FOUND,
        "Rhythm not found".to_string(),
    ))?;

    let timezone = db::get_user_timezone(&state.pool, user_id).await?;

    let rhythm_def = RhythmDefinition {
        id: rhythm_id,
        rhythm: req.rhythm,
        description: req.description,
        created_at: existing.created_at,
        created_at_tz: existing.created_at_tz.clone(),
        modified_at: Utc::now(),
        modified_at_tz: timezone,
    };

    db::save_rhythm(&state.pool, user_id, &rhythm_def).await?;

    Ok(Json(rhythm_def_to_response(rhythm_def)))
}

async fn delete_rhythm(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let user_id = extract_user_from_headers(&headers, &state).await?;
    let rhythm_id = RhythmID::from_human_readable(&id).ok_or(AppError(
        StatusCode::BAD_REQUEST,
        "Invalid rhythm ID".to_string(),
    ))?;

    db::delete_rhythm(&state.pool, user_id, rhythm_id).await?;

    Ok(StatusCode::OK)
}

async fn mark_done(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<MarkDoneRequest>,
) -> Result<StatusCode, AppError> {
    let user_id = extract_user_from_headers(&headers, &state).await?;
    let rhythm_id = RhythmID::from_human_readable(&id).ok_or(AppError(
        StatusCode::BAD_REQUEST,
        "Invalid rhythm ID".to_string(),
    ))?;

    let mut manager = db::load_rhythm_manager(&state.pool, user_id).await?;
    let when = req.when.unwrap_or_else(Utc::now);
    manager.mark_done_at(rhythm_id, when)?;

    for event in manager.events() {
        if event.rhythm_id == rhythm_id && event.when == when {
            db::save_event(&state.pool, user_id, event).await?;
            break;
        }
    }

    Ok(StatusCode::OK)
}

async fn defer_rhythm(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<DeferRequest>,
) -> Result<StatusCode, AppError> {
    let user_id = extract_user_from_headers(&headers, &state).await?;
    let rhythm_id = RhythmID::from_human_readable(&id).ok_or(AppError(
        StatusCode::BAD_REQUEST,
        "Invalid rhythm ID".to_string(),
    ))?;

    let mut manager = db::load_rhythm_manager(&state.pool, user_id).await?;
    let when = req.when.unwrap_or_else(Utc::now);
    manager.defer_rhythm_at(rhythm_id, when)?;

    for event in manager.events() {
        if event.rhythm_id == rhythm_id && event.when == when {
            db::save_event(&state.pool, user_id, event).await?;
            break;
        }
    }

    Ok(StatusCode::OK)
}

async fn get_schedule(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<ScheduleQuery>,
) -> Result<Json<Vec<ScheduleItem>>, AppError> {
    let user_id = extract_user_from_headers(&headers, &state).await?;
    let manager = db::load_rhythm_manager(&state.pool, user_id).await?;

    let limit = query
        .start
        .checked_add_days(chrono::Days::new(query.days as u64))
        .ok_or(AppError(
            StatusCode::BAD_REQUEST,
            "Invalid date range".to_string(),
        ))?;

    let schedule = manager.schedule(query.start, limit);

    let items: Vec<ScheduleItem> = schedule
        .into_iter()
        .map(|(datetime, rhythm_def)| ScheduleItem {
            rhythm_id: rhythm_def.id.to_string(),
            description: rhythm_def.description,
            datetime: datetime.with_timezone(&Utc),
        })
        .collect();

    Ok(Json(items))
}

async fn get_delinquent(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<DelinquentItem>>, AppError> {
    let user_id = extract_user_from_headers(&headers, &state).await?;
    let manager = db::load_rhythm_manager(&state.pool, user_id).await?;

    let delinquent = manager.delinquent();

    let items: Vec<DelinquentItem> = delinquent
        .into_iter()
        .map(|rhythm_def| DelinquentItem {
            rhythm_id: rhythm_def.id.to_string(),
            description: rhythm_def.description,
        })
        .collect();

    Ok(Json(items))
}

async fn get_convergence(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<ConvergenceResponse>, AppError> {
    let user_id = extract_user_from_headers(&headers, &state).await?;
    let manager = db::load_rhythm_manager(&state.pool, user_id).await?;

    let convergence = manager.convergence();

    Ok(Json(ConvergenceResponse {
        datetime: Some(convergence),
    }))
}

async fn get_spoons(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<SpoonsQuery>,
) -> Result<Json<SpoonsResponse>, AppError> {
    let user_id = extract_user_from_headers(&headers, &state).await?;
    let spoons_map = db::load_spoons(&state.pool, user_id).await?;

    let date = query.date.unwrap_or_else(|| Utc::now().date_naive());
    let value = spoons_map.get(&date).copied().unwrap_or(5);

    Ok(Json(SpoonsResponse { date, value }))
}

async fn set_spoons(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<SetSpoonsRequest>,
) -> Result<StatusCode, AppError> {
    let user_id = extract_user_from_headers(&headers, &state).await?;
    let value = req.value.min(10);
    db::save_spoons(&state.pool, user_id, req.date, value).await?;
    Ok(StatusCode::OK)
}

async fn get_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<UserResponse>, AppError> {
    let user_id = extract_user_from_headers(&headers, &state).await?;
    let timezone = db::get_user_timezone(&state.pool, user_id).await?;
    Ok(Json(UserResponse { timezone }))
}

async fn update_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<UpdateUserRequest>,
) -> Result<StatusCode, AppError> {
    let user_id = extract_user_from_headers(&headers, &state).await?;
    db::update_user_timezone(&state.pool, user_id, &req.timezone).await?;
    Ok(StatusCode::OK)
}
