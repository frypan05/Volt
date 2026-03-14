use actix_web::{get, web, App, HttpResponse};

#[get("/health")]
async fn health() -> HttpResponse {
    HttpResponse::Ok().finish()
}

fn app() -> App<()> {
    App::new().route("/metrics", web::get().to(health))
}
