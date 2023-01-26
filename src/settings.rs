#[derive(Debug)]
pub struct Settings {
    pub redis_host: String,
    pub redis_thread_pool_size: usize,
    pub http_server_worker_pool_size: usize,
    pub verbose: bool,
}
