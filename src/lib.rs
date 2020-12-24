//! A [tide] middleware that implements rate-limiting using [governor].
//! # Example
//! ```rust
//! use tide_governor::GovernorMiddleware;
//!
//! #[async_std::main]
//! async fn main() -> tide::Result<()> {
//!     let mut app = tide::new();
//!     app.at("/")
//!         .with(GovernorMiddleware::per_minute(4)?)
//!         .get(|_| async move { todo!() });
//!     app.at("/foo/:bar")
//!         .with(GovernorMiddleware::per_hour(360)?)
//!         .put(|_| async move { todo!() });
//!
//!     app.listen(format!("http://localhost:{}", env::var("PORT")?))
//!         .await?;
//!     Ok(())
//! }
//! ```
// TODO: figure out how to add jitter support using `governor::Jitter`.
// TODO: add usage examples (both in the docs and in an examples directory).
// TODO: add unit tests.
use governor::{
    clock::{Clock, DefaultClock},
    state::keyed::DefaultKeyedStateStore,
    Quota, RateLimiter,
};
use lazy_static::lazy_static;
use std::{
    convert::TryInto,
    error::Error,
    net::{IpAddr, SocketAddr},
    num::NonZeroU32,
    sync::Arc,
    time::Duration,
};
use tide::{
    http::StatusCode,
    log::{debug, trace},
    utils::async_trait,
    Middleware, Next, Request, Response, Result,
};

lazy_static! {
    static ref CLOCK: DefaultClock = DefaultClock::default();
}

#[derive(Debug, Clone)]
pub struct GovernorMiddleware {
    limiter: Arc<RateLimiter<IpAddr, DefaultKeyedStateStore<IpAddr>, DefaultClock>>,
}

impl GovernorMiddleware {
    /// Constructs a rate-limiting middleware from a [`Duration`] that allows one request in the given time interval.
    /// If the time interval is zero, returns `None`.
    #[must_use]
    pub fn with_period(duration: Duration) -> Option<Self> {
        Some(Self {
            limiter: Arc::new(RateLimiter::<IpAddr, _, _>::keyed(Quota::with_period(
                duration,
            )?)),
        })
    }

    /// Constructs a rate-limiting middleware that allows a specified number of requests every second.
    /// Returns an error if `times` can't be converted into a [`NonZeroU32`].
    pub fn per_second<T>(times: T) -> Result<Self>
    where
        T: TryInto<NonZeroU32>,
        T::Error: Error + Send + Sync + 'static,
    {
        Ok(Self {
            limiter: Arc::new(RateLimiter::<IpAddr, _, _>::keyed(Quota::per_second(
                times.try_into()?,
            ))),
        })
    }

    /// Constructs a rate-limiting middleware that allows a specified number of requests every minute.
    /// Returns an error if `times` can't be converted into a [`NonZeroU32`].
    pub fn per_minute<T>(times: T) -> Result<Self>
    where
        T: TryInto<NonZeroU32>,
        T::Error: Error + Send + Sync + 'static,
    {
        Ok(Self {
            limiter: Arc::new(RateLimiter::<IpAddr, _, _>::keyed(Quota::per_minute(
                times.try_into()?,
            ))),
        })
    }

    /// Constructs a rate-limiting middleware that allows a specified number of requests every hour.
    /// Returns an error if `times` can't be converted into a [`NonZeroU32`].
    pub fn per_hour<T>(times: T) -> Result<Self>
    where
        T: TryInto<NonZeroU32>,
        T::Error: Error + Send + Sync + 'static,
    {
        Ok(Self {
            limiter: Arc::new(RateLimiter::<IpAddr, _, _>::keyed(Quota::per_hour(
                times.try_into()?,
            ))),
        })
    }
}

#[async_trait]
impl<State: Clone + Send + Sync + 'static> Middleware<State> for GovernorMiddleware {
    async fn handle(&self, req: Request<State>, next: Next<'_, State>) -> tide::Result {
        let remote: SocketAddr = req
            .remote()
            .ok_or_else(|| {
                tide::Error::from_str(
                    StatusCode::InternalServerError,
                    "failed to get request remote address",
                )
            })?
            .parse()?;
        trace!("remote: {}", remote);
        match self.limiter.check_key(&remote.ip()) {
            Ok(_) => {
                debug!("allowing remote {}", remote);
                Ok(next.run(req).await)
            }
            Err(negative) => {
                let wait_time = negative.wait_time_from(CLOCK.now());
                let res = Response::builder(StatusCode::TooManyRequests)
                    .header(
                        tide::http::headers::RETRY_AFTER,
                        wait_time.as_secs().to_string(),
                    )
                    .build();
                debug!(
                    "blocking address {} for {} seconds",
                    remote.ip(),
                    wait_time.as_secs()
                );
                Ok(res)
            }
        }
    }
}
