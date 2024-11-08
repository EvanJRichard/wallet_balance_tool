use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::time::sleep;

static LAST_REQUEST: AtomicU64 = AtomicU64::new(0);
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(250);

pub async fn enforce_rate_limit() {
    let last = LAST_REQUEST.load(Ordering::Relaxed);
    let now = Instant::now().elapsed().as_millis() as u64;
    let elapsed = now.saturating_sub(last);

    if elapsed < MIN_REQUEST_INTERVAL.as_millis() as u64 {
        sleep(Duration::from_millis(
            MIN_REQUEST_INTERVAL.as_millis() as u64 - elapsed,
        ))
        .await;
    }

    LAST_REQUEST.store(now, Ordering::Relaxed);
}
