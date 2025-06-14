use colored::*;

use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time;
use std::sync::Arc;

use secp256k1::Secp256k1;
use secp256k1::rand::rngs::OsRng;
use secp256k1::hashes::{sha256, Hash};
use hex::encode;
use num_bigint::BigUint;

use std::io::Write;
use crossterm::terminal::size;

mod config;
mod get_job;
mod submit;
mod report;
mod gpu;
use submit::Solution;
use config::Reporting;
use get_job::Job;
use gpu::GPUMiningPool;

pub fn pad_start_256_bit_int(value: &BigUint) -> String {
    let mut hex_string = value.to_str_radix(16); // Convert to hex
    // Ensure the string is 64 characters long (256 bits)
    let padding = 64 - hex_string.len();
    if padding > 0 {
        hex_string = format!("{}{}", "0".repeat(padding), hex_string);
    }

    hex_string
}


#[tokio::main]
async fn main() {
    let config = match config::load() {
        Ok(config) => Arc::new(tokio::sync::RwLock::new(config)),
        Err(_) => {
            eprintln!("{}", "[WARN] Using default config values...".yellow());
            Arc::new(tokio::sync::RwLock::new(config::CLCMinerConfig {
                server: String::from("https://master.centrix.fi"),
                submit_server: String::from("https://master.centrix.fi"),
                rewards_dir: String::from("./rewards"),
                thread: -1,
                gpu: 0,
                gpu_platform: String::from("auto"),
                gpu_workgroup_size: 256,
                gpu_batch_size: 1048576,
                on_mined: String::from(""),
                report_interval: 10,
                job_interval: 1,
                reporting: Reporting {
                    report_server: String::from(""),
                    report_user: String::from(""),
                },
                pool_secret: String::from(""),
            }))
        }
    };
    
    // Log values if optional settings are specified
    if config.read().await.reporting.report_server != "" {
        println!("{} {}/report", "[INFO] Going to report to:".blue(), config.read().await.reporting.report_server);
    }
    if config.read().await.on_mined != "" {
        println!("{} {}{}", "[INFO] Going to run:".blue(), config.read().await.on_mined, ", every time a coin is mined!".yellow());
    }
    
    // Initialize GPU mining if enabled
    let gpu_pool = if config.read().await.gpu > 0 {
        let gpu_config = config.read().await;
        println!("{} Initializing GPU mining with {} devices...", "[GPU]".green(), gpu_config.gpu);
        println!("{} GPU Platform: {}", "[GPU]".green(), gpu_config.get_gpu_platform());
        println!("{} GPU Workgroup Size: {}", "[GPU]".green(), gpu_config.get_gpu_workgroup_size());
        println!("{} GPU Batch Size: {}", "[GPU]".green(), gpu_config.gpu_batch_size);
        drop(gpu_config);
        
        match GPUMiningPool::new(config.read().await.gpu as usize).await {
            Ok(pool) => {
                println!("{} GPU mining initialized successfully", "[GPU]".green());
                println!("{} Total compute units: {}", "[GPU]".green(), pool.get_total_compute_units());
                println!("{} Active miners: {}", "[GPU]".green(), pool.get_active_miners());
                Some(Arc::new(tokio::sync::RwLock::new(pool)))
            }
            Err(e) => {
                println!("{} Failed to initialize GPU mining: {}", "[GPU]".red(), e);
                println!("{} Falling back to CPU-only mining", "[WARN]".yellow());
                None
            }
        }
    } else {
        None
    };

    // Job handling
    let current_job = Arc::new(tokio::sync::RwLock::new(Job::get_wait_job()));
    
    // Stats
    let hash_count = Arc::new(tokio::sync::RwLock::new(0_u64));
    let calced_hash_count = Arc::new(tokio::sync::RwLock::new(0_f64));
    let total_mined = Arc::new(tokio::sync::RwLock::new(0_f64));
    let best: Arc<tokio::sync::RwLock<BigUint>> = Arc::new(tokio::sync::RwLock::new(BigUint::parse_bytes("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF".as_bytes(), 16).unwrap()));

    // Log data
    let hash_count_clone = Arc::clone(&hash_count);
    let calced_hash_count_clone = Arc::clone(&calced_hash_count);
    let best_clone = Arc::clone(&best);
    tokio::spawn(async move {
        loop {
            time::sleep(Duration::from_secs(3)).await;
            {
                let mut hash_count_unlocked = hash_count_clone.write().await;
                let mut calced_hash_count_unlocked = calced_hash_count_clone.write().await;
                let rate: f32;
                let unit: &str;
    
                if *hash_count_unlocked >= (3 * 1_000_000_000_000_u64) {
                    rate = (*hash_count_unlocked as f32) / (3.0 * 1e12);
                    unit = "TH/s";
                } else if *hash_count_unlocked >= (3 * 1_000_000_000) {
                    rate = (*hash_count_unlocked as f32) / (3.0 * 1e9);
                    unit = "GH/s";
                } else if *hash_count_unlocked >= (3 * 1_000_000) {
                    rate = (*hash_count_unlocked as f32) / (3.0 * 1e6);
                    unit = "M/s";
                } else if *hash_count_unlocked >= (3 * 1_000) {
                    rate = (*hash_count_unlocked as f32) / (3.0 * 1e3);
                    unit = "KH/s";
                } else {
                    rate = *hash_count_unlocked as f32;
                    unit = "H/s";
                }
    
                // Replaces the previous printed line
                let (width, _height) = size().unwrap();
                let out = format!("\r{} {}{}", "[INFO]".blue(), rate, unit);
                print!("\r\r{}{}", out, " ".repeat(width as usize - out.len()));
                std::io::stdout().flush().unwrap(); // Ensure immediate output
                
                *calced_hash_count_unlocked = (*hash_count_unlocked as f64) / (3.0 * 1e3);
                *hash_count_unlocked = 0;
                {
                    let mut best_unlocked = best_clone.write().await;
                    *best_unlocked = BigUint::parse_bytes("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF".as_bytes(), 16).unwrap();
                }
            }
        }
    });
    

    // Update job at interval
    let current_job_clone = Arc::clone(&current_job);
    let config_clone = Arc::clone(&config);
    tokio::spawn(async move {
        loop {
            let server_url = config_clone.read().await.server.clone();
            let job = match get_job::get_job(server_url).await {
                Ok(job) => job,
                Err(e) => {
                    eprintln!("{} {}", "[ERROR] Error fetching job:".red(), e);
                    continue;
                }
            };
            {
                let mut job_mut = current_job_clone.write().await;
                if job_mut.seed != job.seed {
                    *job_mut = job;
                    
                    let duration_since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
                    let elapsed_secs = duration_since_epoch.as_secs();
        
                    println!("\n\n{}", "[INFO] New job".blue());
                    println!("{} {} {}", "[INFO]".blue(), "seed:", job_mut.seed);
                    println!("{} {} {}", "[INFO]".blue(), "diff:", pad_start_256_bit_int(&job_mut.diff));
                    println!("{} {} {}", "[INFO]".blue(), "reward:", job_mut.reward.to_string().green());
        
                    let time_since_last_found = elapsed_secs - job_mut.last_found / 1000;
                    println!("{} {} {}s ago\n\n", "[INFO]".blue(), "Last mined", time_since_last_found);
                }
            }
            time::sleep(Duration::from_secs(config_clone.read().await.job_interval as u64)).await;
        }
    });

    // Reporting
    let config_clone = Arc::clone(&config);
    let calced_hash_count_unlocked = Arc::clone(&calced_hash_count);
    let total_mined_clone = Arc::clone(&total_mined);
    let best_clone = Arc::clone(&best);
    tokio::spawn(async move {
        loop {
            let res = report::report(
                &config_clone.read().await.reporting.report_server,
                &config_clone.read().await.reporting.report_user,
                &*calced_hash_count_unlocked.read().await,
                &*total_mined_clone.read().await,
                &pad_start_256_bit_int(&*best_clone.read().await)
            ).await;
            if res != "" {
                println!("\n{} Error reporting: {}", "[ERROR]".red(), res);
            }
            time::sleep(Duration::from_secs(config_clone.read().await.report_interval as u64)).await;
        }
    });

    // Threading
    let thread_num: usize = if config.read().await.thread == -1 { std::thread::available_parallelism().unwrap().get() } else { config.read().await.thread as usize };
    println!("{} Using {} CPU threads", "[INFO]".blue(), thread_num.to_string().green());
    let mut handles = vec![];
    
    // GPU Mining in main thread to avoid Send + Sync issues
    if let Some(_gpu_pool_arc) = gpu_pool.clone() {
        let current_job_clone = Arc::clone(&current_job);
        let hash_count_clone = Arc::clone(&hash_count);
        let config_clone = Arc::clone(&config);
        let total_mined_clone = Arc::clone(&total_mined);
        let best_clone = Arc::clone(&best);

        let gpu_handle = tokio::task::spawn(async move {
            // Create a new GPU pool for this thread
            let mut local_gpu_pool = match gpu::GPUMiningPool::new(
                config_clone.read().await.gpu as usize
            ).await {
                Ok(pool) => pool,
                Err(e) => {
                    println!("{} Failed to create GPU pool: {}", "[GPU]".red(), e);
                    return;
                }
            };
            
            let mut gpu_nonce_base = 0u64;
            
            loop {
                if current_job_clone.read().await.seed == "wait" {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
                
                let job = {
                    let job_guard = current_job_clone.read().await;
                    Job {
                        seed: job_guard.seed.clone(),
                        diff: job_guard.diff.clone(),
                        reward: job_guard.reward,
                        last_found: job_guard.last_found,
                    }
                };
                let batch_size = config_clone.read().await.gpu_batch_size;
                
                // GPU mining batch
                let result = local_gpu_pool.mine_parallel(&job.diff, &job.seed, gpu_nonce_base).await;
                
                match result {
                    Ok(Some((secret_key, public_key, hash))) => {
                        let key_diff = BigUint::from_bytes_be(&hex::decode(&hash[2..]).unwrap_or_default());
                        
                        if key_diff < *best_clone.read().await {
                            let mut best_setter = best_clone.write().await;
                            *best_setter = key_diff.clone();
                        }
                        
                        if job.diff >= key_diff {
                            println!("\n\n{} GPU Found {}CLCs!", "[GPU]".green(), job.reward.to_string().green());
                            let solution = Solution {
                                public_key: public_key,
                                private_key: secret_key,
                                server: config_clone.read().await.submit_server.clone(),
                                hash: hash,
                                on_mined: config_clone.read().await.on_mined.clone(),
                                rewards_dir: config_clone.read().await.rewards_dir.clone(),
                                reward: job.reward,
                                pool_secret: config_clone.read().await.pool_secret.clone()
                            };
                            
                            {
                                let mut job_setter = current_job_clone.write().await;
                                *job_setter = job_setter.get_pause_job();
                            }
                            
                            {
                                let secp = Secp256k1::new();
                                let mut total_setter = total_mined_clone.write().await;
                                solution.submit(&secp, &mut total_setter).await;
                            }
                        }
                        
                        // Update hash count for GPU work
                        {
                            let mut hash_count_setter = hash_count_clone.write().await;
                            *hash_count_setter += batch_size as u64;
                        }
                    }
                    Ok(None) => {
                        // No solution found in this batch
                        let mut hash_count_setter = hash_count_clone.write().await;
                        *hash_count_setter += batch_size as u64;
                    }
                    Err(e) => {
                        println!("{} GPU mining error: {}", "[GPU]".red(), e);
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
                
                gpu_nonce_base = gpu_nonce_base.wrapping_add(batch_size as u64);
            }
        });
        
        handles.push(gpu_handle);
        println!("{} GPU mining thread started", "[GPU]".green());
    }

    for _ in 0..thread_num {
        let current_job_clone = Arc::clone(&current_job);
        let hash_count_clone = Arc::clone(&hash_count);
        let config_clone = Arc::clone(&config);
        let total_mined_clone = Arc::clone(&total_mined);
        let best_clone = Arc::clone(&best);

        let handle = tokio::spawn(async move {
            let secp = Secp256k1::new();
            let mut rate: u64 = 0;
            loop {
                if current_job_clone.read().await.seed == "wait" {
                    continue;
                }
                // Actually mining
                let (secret_key, public_key) = secp.generate_keypair(&mut OsRng);
                let hashed_public_key = sha256::Hash::hash(format!("{}{}", encode(public_key.serialize_uncompressed()), current_job_clone.read().await.seed).as_bytes());
                
                // The difficulty of the key we just created and hashed
                let key_diff = BigUint::from_bytes_be(&hashed_public_key.to_byte_array()[..]);
                if key_diff < *best_clone.read().await {
                    let mut best_setter = best_clone.write().await;
                    *best_setter = key_diff.clone();
                }
                if current_job_clone.read().await.diff >= key_diff {
                    println!("\n\n{} Found {}CLCs!", "[INFO]".blue(), current_job_clone.read().await.reward.to_string().green());
                    let solution = Solution {
                        public_key: public_key,
                        private_key: secret_key,
                        server: config_clone.read().await.submit_server.clone(),
                        hash: hashed_public_key.to_string(),
                        on_mined: config_clone.read().await.on_mined.clone(),
                        rewards_dir: config_clone.read().await.rewards_dir.clone(),
                        reward: current_job_clone.read().await.reward,
                        pool_secret: config_clone.read().await.pool_secret.clone()
                    };
                    {
                        let mut job_setter = current_job_clone.write().await;
                        *job_setter = job_setter.get_pause_job();
                    }
                    {
                        let mut total_setter = total_mined_clone.write().await;
                        solution.submit(&secp, &mut total_setter).await;
                    }
                }
                rate += 1;
                if rate == 100 {
                    let mut hash_count_setter = hash_count_clone.write().await;
                    *hash_count_setter += 100;
                    rate = 0;
                }
            }
        });
        handles.push(handle);
    }

    // Await all tasks
    for handle in handles {
        handle.await.unwrap();
    }
}
