use axum::{routing::{get, post}, Router};

async fn login() {}
async fn users() {}

fn app() -> Router {
    Router::new()
        .route("/v1/login", post(login))
        .route("/users", get(users))
}
