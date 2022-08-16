pub use bb8_redis::bb8::ErrorSink as Bb8ErrorSync;
pub use bb8_redis::redis::RedisError;

use tracing::warn;

#[derive(Debug, Clone)]
pub struct RedisErrorSink;

impl Bb8ErrorSync<RedisError> for RedisErrorSink {
    fn sink(&self, err: RedisError) {
        warn!(?err, "redis error");
    }

    fn boxed_clone(&self) -> Box<dyn Bb8ErrorSync<RedisError>> {
        Box::new(self.clone())
    }
}