#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("migration down failed: {err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let database_url =
        std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL environment variable not set")?;

    let pool = sqlx::PgPool::connect(&database_url).await?;
    let migrator = sqlx::migrate!("./migrations");
    migrator.undo(&pool, 1).await?;
    println!("rolled back one migration");
    Ok(())
}
