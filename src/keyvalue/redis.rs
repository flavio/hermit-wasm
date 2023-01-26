use anyhow::{anyhow, Result};
use log::{debug, info};
use r2d2::Pool;
use redis::Commands;
use scheduled_thread_pool::ScheduledThreadPool;
use std::{fmt, sync::Arc};

use super::keyvalue::{Keyvalue, KeyvalueError, KeyvalueTables};

pub struct RedisDriver {
    container_name: String,
    pool: Pool<redis::Client>,
}

impl fmt::Debug for RedisDriver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RedisDriver")
            .field("collection", &self.container_name)
            .finish()
    }
}

impl RedisDriver {
    fn new(collection_name: &str, connection_pool: Pool<redis::Client>) -> Self {
        Self {
            container_name: collection_name.to_owned(),
            pool: connection_pool,
        }
    }

    /// get the payload for a given key
    fn keyvalue_get(&self, key: &str) -> Result<Vec<u8>, KeyvalueError> {
        debug!(key = key, container_name = self.container_name.as_str(); "redis get key");
        let mut client = self
            .pool
            .get()
            .map_err(|e| KeyvalueError::ConnectionError(e.to_string()))?;

        let key = format!("{}:{}", self.container_name, key);
        let val: Vec<u8> = client
            .get(key.clone())
            .map_err(|e| KeyvalueError::ConnectionError(e.to_string()))?;
        // Redis GET returns [:ok; nil] for non-existent keys
        if val.is_empty() {
            return Err(KeyvalueError::KeyNotFound(key));
        }
        Ok(val)
    }

    /// set the payload for a given key
    fn keyvalue_set(&self, key: &str, value: &[u8]) -> Result<(), KeyvalueError> {
        debug!("redis set key");
        let mut client = self
            .pool
            .get()
            .map_err(|e| KeyvalueError::ConnectionError(e.to_string()))?;

        let key = format!("{}:{}", self.container_name, key);

        client
            .set(key, value)
            .map_err(|e| KeyvalueError::UnexpectedError(e.to_string()))
    }

    /// list the keys in the store
    fn keyvalue_keys(&self) -> Result<Vec<String>, KeyvalueError> {
        debug!("redis keys");
        let mut client = self
            .pool
            .get()
            .map_err(|e| KeyvalueError::ConnectionError(e.to_string()))?;

        let keys: Vec<String> = client
            .keys(format!("{}:*", self.container_name))
            .map_err(|e| KeyvalueError::UnexpectedError(e.to_string()))?;
        // remove prefix
        let keys: Vec<String> = keys
            .iter()
            .map(|k| k.replace(format!("{}:", self.container_name).as_str(), ""))
            .collect();
        Ok(keys)
    }

    /// delete the payload for a given key
    fn keyvalue_delete(&self, key: &str) -> Result<(), KeyvalueError> {
        debug!("redis delete key");
        let mut client = self
            .pool
            .get()
            .map_err(|e| KeyvalueError::ConnectionError(e.to_string()))?;

        let key = format!("{}:{}", self.container_name, key);
        client
            .del(key)
            .map_err(|e| KeyvalueError::UnexpectedError(e.to_string()))
    }
}

pub struct RedisImplementor {
    connection_pool: Pool<redis::Client>,
}

impl RedisImplementor {
    pub fn new(pool: Pool<redis::Client>) -> Self {
        Self {
            connection_pool: pool,
        }
    }
}

impl Keyvalue for RedisImplementor {
    type Keyvalue = RedisDriver;

    fn keyvalue_open(&mut self, name: &str) -> Result<Self::Keyvalue, KeyvalueError> {
        Ok(RedisDriver::new(name, self.connection_pool.clone()))
    }

    /// get the payload for a given key
    fn keyvalue_get(
        &mut self,
        self_: &Self::Keyvalue,
        key: &str,
    ) -> Result<Vec<u8>, KeyvalueError> {
        self_.keyvalue_get(key)
    }

    /// set the payload for a given key
    fn keyvalue_set(
        &mut self,
        self_: &Self::Keyvalue,
        key: &str,
        value: &[u8],
    ) -> Result<(), KeyvalueError> {
        self_.keyvalue_set(key, value)
    }

    /// list the keys in the store
    fn keyvalue_keys(&mut self, self_: &Self::Keyvalue) -> Result<Vec<String>, KeyvalueError> {
        self_.keyvalue_keys()
    }

    /// delete the payload for a given key
    fn keyvalue_delete(&mut self, self_: &Self::Keyvalue, key: &str) -> Result<(), KeyvalueError> {
        self_.keyvalue_delete(key)
    }
}

pub struct RedisKeyvalueContext {
    pub kv: RedisImplementor,
    pub table: KeyvalueTables<RedisImplementor>,
}

impl RedisKeyvalueContext {
    pub fn new(redis_host: &str, max_pool_size: usize) -> Result<Self> {
        info!("connecting to redis database: {}", redis_host);
        let client = redis::Client::open(format!("redis://{}/", redis_host))
            .map_err(|e| anyhow!("error opening connection: {e}"))?;

        let thread_pool = Arc::new(ScheduledThreadPool::with_name(
            "r2d2-worker-{}",
            max_pool_size,
        ));

        debug!("creating connection pool");
        let pool = r2d2::Pool::builder()
            .thread_pool(thread_pool)
            .build(client)
            .map_err(|e| anyhow!("error building pool: {}", e))?;

        Ok(Self {
            kv: RedisImplementor::new(pool),
            table: KeyvalueTables::<RedisImplementor>::default(),
        })
    }
}
