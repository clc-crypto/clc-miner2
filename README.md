# clc-miner2
## Installation
`git` and `rust` is required (`cargo` too!)
If you do not have the newest `rustc` version or `cargo` run:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
export PATH="$HOME/.cargo/bin:$PATH"
bash
```

1. Clone the repository locally
   ```bash
   git clone https://github.com/clc-crypto/clc-miner2
   ```
2. Build the miner
   ```bash
   cargo build --release
   ```
3. Run the miner
   ```bash
   ./target/release/clc-miner2
      

## Configuration
The configuration is stored in the clcminer.toml in the project root directory

```toml
server = "https://clc.ix.tc"
rewards_dir = "rewards"
thread = -1
gpu = 1
gpu_platform = "auto"
gpu_workgroup_size = 256
gpu_batch_size = 1048576
```
Where:

  thread - amount of threads to run the miner on (-1 is max)

  gpu - 0 is disable ; 1 enable

  gpu_batch_size = 1048576 or the more the hashrate
  
  rewars_dir - directory to store the rewards in
  
  server - the clc-daemon to connect to

Optional:
```toml
job_interval = 10
report_interval = 2
on_mined = "clc-wallet add-coin rewards/%cid%.coin"
```
Where:

  on_mined - command to execute every time a coin is mined, %cid% is the mined coin id
  
  report_interval - how often should the miner report performance
  
  job_interval - how often to scan for new jobs

### Set up performance reporting
To set up reporting add the following to your clcminer.toml
```toml
[reporting]
report_user = "xxxx"
report_server = "https://clc.ix.tc:3000"
```
Where:

  report_user - the username for the report server (do not share!)
  
  report_server - the server to report performance to
  
(If you use https://clc.ix.tc:3000 You can see your miners performance at [CLC Wallet](https://clc-crypto.github.io/miners/)
