#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("migration up failed: {err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let database_url =
        std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL environment variable not set")?;

    let pool = sqlx::PgPool::connect(&database_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    println!("migrations applied");
    Ok(())
}
