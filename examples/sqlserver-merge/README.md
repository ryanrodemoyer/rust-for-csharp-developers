# SQL Server Idempotent MERGE CLI Demo

A terse, educational Rust CLI demonstrating how to connect to Microsoft SQL Server, ensure table schema idempotency, execute an idempotent `MERGE` (upsert), and query results into strongly typed Rust structs.

Designed as an educational reference for **.NET / C# engineers mastering Rust** (and vice-versa).

---

## 🚀 Quick Start

Ensure SQL Server is running (e.g. in Docker on port 1433).

### Run with defaults (connects to `127.0.0.1:1433`, user `sa`, database `tempdb`):
```bash
cargo run -p sqlserver-merge
```

### Run with table reset:
```bash
cargo run -p sqlserver-merge -- --reset
```

### Run automated integration tests:
```bash
cargo test -p sqlserver-merge
```

### CLI Options:
```
Options:
  -H, --host <HOST>          SQL Server host [default: 127.0.0.1]
  -P, --port <PORT>          SQL Server port [default: 1433]
  -u, --user <USER>          Database username [default: sa]
  -p, --password <PASSWORD>  Database password [default: YourStrong@Password123]
  -d, --database <DATABASE>  Target database [default: tempdb]
      --reset                Reset table before running demo (DROP TABLE IF EXISTS)
  -h, --help                 Print help
  -V, --version              Print version
```

---

## 🧠 .NET ⇄ Rust Rosetta Stone

| .NET / C# Concept | Rust Equivalent (`tiberius` + `tokio`) | Notes |
| :--- | :--- | :--- |
| `SqlConnection` | `tiberius::Client<Compat<TcpStream>>` | Active connection over asynchronous TDS protocol |
| `SqlConnectionStringBuilder` | `tiberius::Config` | Configures host, port, auth, TLS trust, and database |
| `TrustServerCertificate=True` | `config.trust_cert()` | Accepts self-signed dev/Docker certificates |
| `SqlCommand.ExecuteNonQueryAsync()` | `client.execute(sql, &params).await?` | Runs DDL or DML statements returning affected rows |
| `SqlCommand.ExecuteReaderAsync()` | `client.query(sql, &params).await?` | Runs query statements returning a row stream |
| `SqlParameter` (`@P1`) | Borrowed slice `&[&p1, &p2, ...]` | Positional parameters preventing SQL injection |
| `SqlDataReader["col"]` | `row.get::<T, _>("col")` | Returns `Option<T>` (`None` if SQL `NULL`) |
| ThreadPool Async | `#[tokio::main]` | Multi-threaded async runtime driven by work-stealing |

---

## 🔍 Key Architectural Details

### 1. Pure Rust TLS with `rustls`
By selecting `tiberius` with `default-features = false` and `features = ["rustls"]`, the binary has **zero dependency on system OpenSSL libraries (`libssl-dev`)**. This compiles cleanly anywhere without OS-level C library dependencies.

### 2. Idempotent Schema Creation
```sql
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
```
Safe to execute any number of times without throwing an error if the table already exists.

### 3. Idempotent MERGE (Upsert)
```sql
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
```
- **Atomicity:** Matches rows on the primary key `id`.
- **Idempotency:** Re-running with the same values produces no duplicate key errors and leaves the table in the desired target state.
- **Param Passing:** Values are passed as references: `client.execute(sql, &[&product.id, &product.sku.as_str(), ...])`.

### 4. Strongly Typed Query Mapping
```rust
let rows = client
    .query("SELECT id, sku, name, price, stock FROM dbo.products ORDER BY id ASC;", &[])
    .await?
    .into_first_result()
    .await?;

for row in rows {
    let product = Product {
        id: row.get::<i32, _>("id").unwrap(),
        sku: row.get::<&str, _>("sku").unwrap().to_string(),
        name: row.get::<&str, _>("name").unwrap().to_string(),
        price: row.get::<f64, _>("price").unwrap(),
        stock: row.get::<i32, _>("stock").unwrap(),
    };
}
```
`row.get::<T, _>` performs compile-time verified type extraction directly into native Rust primitives.
