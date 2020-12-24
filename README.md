# tide-governor

A [tide] middleware that provides rate-limiting functionality backed by [governor]

# Example
```rust
use tide_governor::GovernorMiddleware;
use std::env;

#[async_std::main]
async fn main() -> tide::Result<()> {
    let mut app = tide::new();
    app.at("/")
        .with(GovernorMiddleware::per_minute(4)?)
        .get(|_| async move { todo!() });
    app.at("/foo/:bar")
        .with(GovernorMiddleware::per_hour(360)?)
        .put(|_| async move { todo!() });

    app.listen(format!("http://localhost:{}", env::var("PORT")?))
        .await?;
    Ok(())
}
```

[tide]: https://github.com/http-rs/tide/
[governor]: https://github.com/antifuchs/governor
