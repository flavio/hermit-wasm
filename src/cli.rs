use crate::settings::Settings;

use anyhow::{anyhow, Result};
use getopts::Options;
use std::{env, usize};

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} FILE [options]", program);
    print!("{}", opts.usage(&brief));
}

pub fn parse_cli() -> Result<Option<Settings>> {
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.optopt("r", "redis-host", "host running Redis", "NAME");
    opts.optopt(
        "",
        "redis-thread-pool-size",
        "size of the thread pool used to manage Redis connections",
        "SIZE",
    );
    opts.optopt(
        "",
        "http-server-worker-pool-size",
        "size of the worker pool used to manage HTTP server",
        "SIZE",
    );

    opts.optflag("v", "verbose", "enable verbose output");
    opts.optflag("h", "help", "print this help menu");
    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(e) => return Err(anyhow!("error parsing cli flags: {:?}", e)),
    };
    if matches.opt_present("h") {
        print_usage(&program, opts);
        return Ok(None);
    }

    let redis_host = matches
        .opt_str("r")
        .ok_or_else(|| anyhow!("The redis connection parameter must be provided"))?;
    if !matches.free.is_empty() {
        print_usage(&program, opts);
        return Err(anyhow!("Unknown args: {:?}", matches.free));
    };

    let redis_thread_pool_size = matches
        .opt_str("redis-thread-pool-size")
        .map_or_else(|| Ok(1), |s| s.parse::<usize>())
        .map_err(|e| {
            anyhow!(
                "Cannot convert {:?} to number: {}",
                matches.opt_str("redis-thread-pool-size"),
                e
            )
        })?;

    let http_server_worker_pool_size = matches
        .opt_str("http-server-worker-pool-size")
        .map_or_else(|| Ok(2), |s| s.parse::<usize>())
        .map_err(|e| {
            anyhow!(
                "Cannot convert {:?} to number: {}",
                matches.opt_str("redis-thread-pool-size"),
                e
            )
        })?;

    Ok(Some(Settings {
        redis_host,
        redis_thread_pool_size,
        http_server_worker_pool_size,
        verbose: matches.opt_present("v"),
    }))
}
