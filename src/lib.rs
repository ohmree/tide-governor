// TODO: figure out how to add jitter support using `governor::Jitter`.
use governor::{clock::DefaultClock, state::keyed::DefaultKeyedStateStore, RateLimiter};
use std::sync::Arc;
use tide::{http::StatusCode, utils::async_trait, Middleware, Next, Request};

#[derive(Debug, Clone)]
pub struct GovernorMiddleware {
    limiter: Arc<RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>>,
    param: String,
}

impl GovernorMiddleware {
    #[must_use]
    pub fn new(param: impl Into<String>, quota: Quota) -> Self {
        Self {
            limiter: Arc::new(RateLimiter::<String, _, _>::keyed(quota)),
            param: param.into(),
        }
    }
}

#[async_trait]
impl<State: Clone + Send + Sync + 'static> Middleware<State> for GovernorMiddleware {
    async fn handle(&self, req: Request<State>, next: Next<'_, State>) -> tide::Result {
        let param = req.param(&self.param)?.to_owned();
        match self.limiter.check_key(&param) {
            Ok(_) => Ok(next.run(req).await),
            Err(negative) => Err(tide::Error::from_str(
                StatusCode::TooManyRequests,
                negative.to_string(),
            )),
        }
    }
}

pub use governor::Quota;
