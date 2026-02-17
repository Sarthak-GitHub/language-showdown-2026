use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use redis::aio::ConnectionManager;
use redis::{AsyncCommands, RedisError};
use serde_json::json;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::{info, Level};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::models::{Claims, User, UserCreate};

mod models;

#[derive(Clone)]
struct AppState {
    db: PgPool,
    redis: ConnectionManager,
    secret: Vec<u8>,
}

const RATE_LIMIT: i64 = 100;
const RATE_WINDOW: usize = 60;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env().add_directive(Level::INFO.into()))
        .init();

    let db = PgPool::connect("postgres://benchmark:benchmark@localhost:5432/benchmark").await?;
    let redis_client = redis::Client::open("redis://localhost:6379")?;
    let redis = ConnectionManager::new(redis_client).await?;

    let state = Arc::new(AppState {
        db,
        redis,
        secret: b"super-secret-key-for-benchmark-only".to_vec(),
    });

    let app = Router::new()
        .route("/login", post(login))
        .route("/users", post(create_user))
        .route("/users/:id", get(get_user))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .layer(middleware::from_fn_with_state(state.clone(), rate_limit_middleware))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    info!("Rust Axum starting on :8002");
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8002").await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn auth_middleware<B>(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    mut req: Request<B>,
    next: Next<B>,
) -> Result<Response, StatusCode> {
    let auth = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
    let token = match auth {
        Some(t) if t.starts_with("Bearer ") => &t[7..],
        _ => return Err(StatusCode::UNAUTHORIZED),
    };

    let validation = Validation::new(jsonwebtoken::Algorithm::HS256);
    let token_data = decode::<Claims>(token, &DecodingKey::from_secret(&state.secret), &validation)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    req.extensions_mut().insert(token_data.claims.sub);
    Ok(next.run(req).await)
}

async fn rate_limit_middleware<B>(
    State(state): State<Arc<AppState>>,
    req: Request<B>,
    next: Next<B>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let user_id: &String = req.extensions().get::<String>().ok_or((
        StatusCode::UNAUTHORIZED,
        Json(json!({"error": "No user context"})),
    ))?;

    let key = format!("rate:{}", user_id);
    let mut conn = state.redis.clone();

    let count: i64 = conn.incr(&key, 1).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Redis error"})),
        )
    })?;

    if count == 1 {
        let _: () = conn.expire(&key, RATE_WINDOW).await.unwrap_or(());
    }

    if count > RATE_LIMIT {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error": "Rate limit exceeded"})),
        ));
    }

    Ok(next.run(req).await)
}

async fn login(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let claims = Claims {
        sub: "benchmark-user".to_string(),
        iat: time::OffsetDateTime::now_utc().unix_timestamp() as usize,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(&state.secret),
    )
    .unwrap();

    Json(json!({
        "access_token": token,
        "token_type": "bearer"
    }))
}

async fn create_user(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UserCreate>,
) -> Result<Json<User>, StatusCode> {
    let row = sqlx::query("INSERT INTO users (name, email) VALUES ($1, $2) RETURNING id, name, email")
        .bind(&payload.name)
        .bind(&payload.email)
        .fetch_one(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(User {
        id: row.get("id"),
        name: row.get("name"),
        email: row.get("email"),
    }))
}

async fn get_user(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
) -> Result<Json<User>, StatusCode> {
    let row = sqlx::query("SELECT id, name, email FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match row {
        Some(r) => Ok(Json(User {
            id: r.get("id"),
            name: r.get("name"),
            email: r.get("email"),
        })),
        None => Err(StatusCode::NOT_FOUND),
    }
}
