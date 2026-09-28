//! # SQL Server Idempotent MERGE CLI Demo
//!
//! A terse, educational Rust CLI demonstrating:
//! 1. Connecting to Microsoft SQL Server via TDS protocol (`tiberius` + `tokio`).
//! 2. DDL table creation with schema idempotency (`IF OBJECT_ID(...) IS NULL`).
//! 3. Idempotent upserting using SQL Server's `MERGE` statement.
//! 4. Querying and mapping rows into strongly typed Rust domain structs.
//!
//! ### Rosetta Stone for .NET Developers:
//! - `SqlConnection` -> `tiberius::Client<Compat<TcpStream>>`
//! - `SqlCommand.ExecuteNonQueryAsync()` -> `client.execute(query, &params).await?`
//! - `SqlCommand.ExecuteReaderAsync()` -> `client.query(query, &params).await?`
//! - `SqlDataReader["col"]` -> `row.get::<T, _>("col")`
//! - `SqlParameter (@P1)` -> Positional parameter slice `&[&val1, &val2]`

use anyhow::{Context, Result};
use clap::Parser;
use tiberius::{AuthMethod, Client, Config};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

/// Type alias bridging Tokio's TcpStream to Tiberius's futures::AsyncRead/Write.
/// In .NET terms, think of this as an active `SqlConnection`.
pub type SqlConnection = Client<Compat<TcpStream>>;

/// Strongly typed domain model representing a row in `dbo.products`.
#[derive(Debug, Clone, PartialEq)]
pub struct Product {
    pub id: i32,
    pub sku: String,
    pub name: String,
    pub price: f64,
    pub stock: i32,
}

#[derive(Parser, Debug, Clone)]
#[command(
    name = "sqlserver-merge",
    author = "dotnetrust",
    version = "0.1.0",
    about = "Terse educational Rust CLI connecting to SQL Server to perform an idempotent MERGE"
)]
pub struct Args {
    #[arg(short = 'H', long, default_value = "127.0.0.1", help = "SQL Server host")]
    pub host: String,

    #[arg(short = 'P', long, default_value_t = 1433, help = "SQL Server port")]
    pub port: u16,

    #[arg(short = 'u', long, default_value = "sa", help = "Database username")]
    pub user: String,

    #[arg(
        short = 'p',
        long,
        default_value = "YourStrong@Password123",
        help = "Database password"
    )]
    pub password: String,

    #[arg(short = 'd', long, default_value = "tempdb", help = "Target database")]
    pub database: String,

    #[arg(
        long,
        default_value_t = false,
        help = "Reset table before running demo (DROP TABLE IF EXISTS)"
    )]
    pub reset: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    println!("============================================================");
    println!("  Rust + SQL Server: Idempotent MERGE & Query Demo          ");
    println!("============================================================");

    // Step 1: Establish Connection
    let mut client = connect(&args).await?;
    println!("  Connected to SQL Server at {}:{}", args.host, args.port);

    // Step 2: Ensure Table Exists
    ensure_table(&mut client, args.reset).await?;

    // Step 3: Perform Idempotent MERGE (Upserts)
    // Run Batch 1: Initial Seed
    println!("\n--- Step 3A: Seeding Initial Products (Batch 1) ---");
    let batch_1 = vec![
        Product {
            id: 1,
            sku: "KB-01".into(),
            name: "Mechanical Keyboard (Tenkeyless)".into(),
            price: 89.99,
            stock: 20,
        },
        Product {
            id: 2,
            sku: "MS-02".into(),
            name: "Ergonomic Vertical Mouse".into(),
            price: 49.50,
            stock: 50,
        },
        Product {
            id: 3,
            sku: "MN-03".into(),
            name: "4K IPS Ultra-Wide Monitor".into(),
            price: 349.00,
            stock: 12,
        },
    ];

    for product in &batch_1 {
        let affected = merge_product(&mut client, product).await?;
        println!(
            "  [MERGE] ID {:<2} | {:<7} | {:<32} -> {} row(s) affected",
            product.id, product.sku, product.name, affected
        );
    }

    // Step 3B: Re-run with updates & a new insert to prove idempotence!
    println!("\n--- Step 3B: Re-running MERGE with updates + additions (Batch 2) ---");
    println!("  (Notice: ID 1 updated, ID 2 unchanged, ID 4 newly inserted, no duplicate errors)");
    let batch_2 = vec![
        Product {
            id: 1,
            sku: "KB-01".into(),
            name: "Mechanical Keyboard (RGB Backlit)".into(), // Updated name
            price: 99.99,                                     // Updated price
            stock: 15,                                        // Updated stock
        },
        Product {
            id: 2,
            sku: "MS-02".into(),
            name: "Ergonomic Vertical Mouse".into(), // Unchanged
            price: 49.50,
            stock: 50,
        },
        Product {
            id: 4,
            sku: "HS-04".into(),
            name: "ANC Wireless Studio Headset".into(), // New insert
            price: 129.99,
            stock: 35,
        },
    ];

    for product in &batch_2 {
        let affected = merge_product(&mut client, product).await?;
        println!(
            "  [MERGE] ID {:<2} | {:<7} | {:<32} -> {} row(s) affected",
            product.id, product.sku, product.name, affected
        );
    }

    // Step 4: SQL SELECT & Map to Rust Structs
    println!("\n--- Step 4: SQL SELECT and Strongly Typed Mapping ---");
    let products = select_all_products(&mut client).await?;
    display_products(&products);

    println!("\n All operations completed successfully and idempotently.");
    Ok(())
}

/// 1. Configure and establish connection to Microsoft SQL Server.
/// In Tiberius, `Config` mirrors ADO.NET connection strings:
/// `Server=127.0.0.1,1433;Database=tempdb;User Id=sa;Password=...;TrustServerCertificate=True;`
pub async fn connect(args: &Args) -> Result<SqlConnection> {
    print!("[1/4] Connecting to SQL Server [{}] ...", args.database);
    let mut config = Config::new();
    config.host(&args.host);
    config.port(args.port);
    config.authentication(AuthMethod::sql_server(&args.user, &args.password));
    config.database(&args.database);

    // SQL Server in Docker typically uses a self-signed TLS cert.
    // Equivalent to `TrustServerCertificate=True` in ADO.NET / EF Core.
    config.trust_cert();

    let tcp = TcpStream::connect(config.get_addr())
        .await
        .with_context(|| format!("Failed to connect to TCP socket at {}", config.get_addr()))?;
    tcp.set_nodelay(true)?;

    // Wrap Tokio TcpStream with compat_write() to satisfy Tiberius's futures I/O traits
    let client = Client::connect(config, tcp.compat_write())
        .await
        .context("TDS handshake / login failed")?;

    Ok(client)
}

/// 2. Idempotent DDL: Creates table `dbo.products` if it doesn't already exist.
pub async fn ensure_table(client: &mut SqlConnection, reset: bool) -> Result<()> {
    if reset {
        println!("[2/4] Resetting table `dbo.products` (DROP IF EXISTS)...");
        client
            .execute(
                "IF OBJECT_ID(N'dbo.products', N'U') IS NOT NULL DROP TABLE dbo.products;",
                &[],
            )
            .await
            .context("Failed to drop existing table")?;
    } else {
        println!("[2/4] Ensuring table `dbo.products` exists...");
    }

    let ddl = "
        IF OBJECT_ID(N'dbo.products', N'U') IS NULL
        BEGIN
            CREATE TABLE dbo.products (
                id    INT           NOT NULL PRIMARY KEY,
                sku   NVARCHAR(50)  NOT NULL,
                name  NVARCHAR(100) NOT NULL,
                price FLOAT         NOT NULL,
                stock INT           NOT NULL
            );
        END
    ";

    client
        .execute(ddl, &[])
        .await
        .context("Failed to execute CREATE TABLE DDL")?;

    println!("  Table `dbo.products` is ready.");
    Ok(())
}

/// 3. Idempotent MERGE (Upsert):
/// Evaluates whether a row matching `target.id = source.id` exists:
/// - If MATCHED: updates existing fields.
/// - If NOT MATCHED: inserts new row.
///
/// Notice the positional parameters: `@P1`, `@P2`, `@P3`, `@P4`, `@P5`.
/// Parameters are passed as a slice of references: `&[&p1, &p2, ...]`.
pub async fn merge_product(client: &mut SqlConnection, product: &Product) -> Result<u64> {
    let merge_sql = "
        MERGE dbo.products AS target
        USING (VALUES (@P1, @P2, @P3, @P4, @P5)) AS source (id, sku, name, price, stock)
        ON target.id = source.id
        WHEN MATCHED THEN
            UPDATE SET
                target.sku   = source.sku,
                target.name  = source.name,
                target.price = source.price,
                target.stock = source.stock
        WHEN NOT MATCHED THEN
            INSERT (id, sku, name, price, stock)
            VALUES (source.id, source.sku, source.name, source.price, source.stock);
    ";

    let result = client
        .execute(
            merge_sql,
            &[
                &product.id,
                &product.sku.as_str(),
                &product.name.as_str(),
                &product.price,
                &product.stock,
            ],
        )
        .await
        .with_context(|| format!("Failed to merge product ID {}", product.id))?;

    Ok(result.total())
}

/// 4. Query `dbo.products` and deserialize rows into Rust `Product` structs.
/// Tiberius rows are strongly typed; `row.get::<T, _>("column_name")` retrieves
/// the value converted into the Rust target type `T`.
pub async fn select_all_products(client: &mut SqlConnection) -> Result<Vec<Product>> {
    println!("[4/4] Executing SELECT query from `dbo.products`...");

    let select_sql = "
        SELECT id, sku, name, price, stock
        FROM dbo.products
        ORDER BY id ASC;
    ";

    let rows = client
        .query(select_sql, &[])
        .await
        .context("Query execution failed")?
        .into_first_result()
        .await
        .context("Failed collecting result rows")?;

    let mut products = Vec::with_capacity(rows.len());
    for row in rows {
        // row.get returns Option<T> (None if SQL value is NULL)
        let product = Product {
            id: row
                .get::<i32, _>("id")
                .context("Missing or invalid 'id'")?,
            sku: row
                .get::<&str, _>("sku")
                .context("Missing or invalid 'sku'")?
                .to_string(),
            name: row
                .get::<&str, _>("name")
                .context("Missing or invalid 'name'")?
                .to_string(),
            price: row
                .get::<f64, _>("price")
                .context("Missing or invalid 'price'")?,
            stock: row
                .get::<i32, _>("stock")
                .context("Missing or invalid 'stock'")?,
        };
        products.push(product);
    }

    Ok(products)
}

/// Helper to render an ASCII summary table for the terminal.
pub fn display_products(products: &[Product]) {
    println!("\n+----+---------+----------------------------------+---------+-------+");
    println!("| ID | SKU     | Name                             | Price   | Stock |");
    println!("+----+---------+----------------------------------+---------+-------+");
    for p in products {
        println!(
            "| {:<2} | {:<7} | {:<32} | ${:>6.2} | {:>5} |",
            p.id, p.sku, p.name, p.price, p.stock
        );
    }
    println!("+----+---------+----------------------------------+---------+-------+");
    println!("Total rows returned: {}", products.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sql_server_idempotent_merge() -> Result<()> {
        let args = Args {
            host: "127.0.0.1".into(),
            port: 1433,
            user: "sa".into(),
            password: "YourStrong@Password123".into(),
            database: "tempdb".into(),
            reset: true,
        };

        let mut client = connect(&args).await?;
        ensure_table(&mut client, true).await?;

        let product = Product {
            id: 999,
            sku: "TEST-SKU".into(),
            name: "Initial Unit Test Name".into(),
            price: 19.99,
            stock: 10,
        };

        // First merge: INSERT
        let affected = merge_product(&mut client, &product).await?;
        assert_eq!(affected, 1);

        // Second merge with same ID: UPDATE (Idempotent upsert)
        let updated_product = Product {
            id: 999,
            sku: "TEST-SKU".into(),
            name: "Updated Unit Test Name".into(),
            price: 29.99,
            stock: 15,
        };
        let affected_again = merge_product(&mut client, &updated_product).await?;
        assert_eq!(affected_again, 1);

        // Query back and assert values updated without creating duplicate rows
        let products = select_all_products(&mut client).await?;
        let found = products
            .iter()
            .find(|p| p.id == 999)
            .expect("Product 999 should exist");

        assert_eq!(found.name, "Updated Unit Test Name");
        assert_eq!(found.price, 29.99);
        assert_eq!(found.stock, 15);

        Ok(())
    }
}
