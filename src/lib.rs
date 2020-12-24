//! A [tide] middleware that implements rate-limiting using [governor].
// TODO: figure out how to add jitter support using `governor::Jitter`.
use governor::{
    clock::{Clock, DefaultClock},
    state::keyed::DefaultKeyedStateStore,
    Quota, RateLimiter,
};
use lazy_static::lazy_static;
use std::{convert::TryInto, error::Error, net::{IpAddr, SocketAddr}, num::NonZeroU32, sync::Arc, time::Duration};
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
    /// Construct a middleware from a [`Duration`] that limits requests to one in a time interval.
    /// If the time interval is zero, returns `None`.
    #[must_use]
    pub fn with_period(duration: Duration) -> Option<Self> {
        Some(Self {
            limiter: Arc::new(RateLimiter::<IpAddr, _, _>::keyed(Quota::with_period(
                duration,
            )?)),
        })
    }

    pub fn per_second<T>(seconds: T) -> Result<Self> where
        T: TryInto<NonZeroU32>,
        T::Error: Error + Send + Sync + 'static
    {
        Ok(Self {
            limiter: Arc::new(RateLimiter::<IpAddr, _, _>::keyed(Quota::per_second(
                seconds.try_into()?,
            ))),
        })
    }

    pub fn per_minute<T>(minutes: T) -> Result<Self> where
        T: TryInto<NonZeroU32>,
        T::Error: Error + Send + Sync + 'static
    {
        Ok(Self {
            limiter: Arc::new(RateLimiter::<IpAddr, _, _>::keyed(Quota::per_minute(
                minutes.try_into()?,
            ))),
        })
    }

    pub fn per_hour<T>(hours: T) -> Result<Self> where
        T: TryInto<NonZeroU32>,
        T::Error: Error + Send + Sync + 'static
    {
        Ok(Self {
            limiter: Arc::new(RateLimiter::<IpAddr, _, _>::keyed(Quota::per_hour(
                hours.try_into()?,
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
